// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use gdbstub::conn::Connection;

pub struct DebugConnection {
    delivered: usize,
    packet: Vec<u8>,
    packet_ready: bool,
    read_progress: Instant,
    pending: VecDeque<u8>,
    stream: TcpStream,
    write_progress: Instant,
}

impl DebugConnection {
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            delivered: 0,
            packet: Vec::new(),
            packet_ready: false,
            read_progress: Instant::now(),
            pending: VecDeque::new(),
            stream,
            write_progress: Instant::now(),
        })
    }

    pub fn next_byte(&mut self) -> io::Result<Option<u8>> {
        if self.packet_ready {
            let byte = self.packet[self.delivered];
            self.delivered += 1;
            if self.delivered == self.packet.len() {
                self.packet.clear();
                self.delivered = 0;
                self.packet_ready = false;
            }
            return Ok(Some(byte));
        }
        if !self.packet.is_empty() && self.read_progress.elapsed() > Duration::from_secs(30) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "debugger packet stalled",
            ));
        }
        for _ in 0..4096 {
            let mut byte = [0];
            match self.stream.read(&mut byte) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "debugger disconnected unexpectedly",
                    ));
                }
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
            if self.packet.is_empty() {
                self.read_progress = Instant::now();
            }
            if self.packet.is_empty() && byte[0] != b'$' {
                return Ok(Some(byte[0]));
            }
            if self.packet.len() == 4096 {
                return Err(io::Error::other("debugger packet exceeds 4096 bytes"));
            }
            self.packet.push(byte[0]);
            let length = self.packet.len();
            if length >= 4 && self.packet[length - 3] == b'#' {
                let payload = &self.packet[1..length - 3];
                if (matches!(payload.first(), Some(b'c' | b's')) && payload.len() > 1)
                    || (matches!(payload.first(), Some(b'C' | b'S')) && payload.contains(&b';'))
                {
                    return Err(io::Error::other(
                        "debugger resume with a changed PC is unsupported",
                    ));
                }
                self.packet_ready = true;
                return self.next_byte();
            }
        }
        Ok(None)
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    pub fn pump(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.write_progress.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "debugger client stopped reading",
            ));
        }
        match Write::write(&mut self.stream, self.pending.as_slices().0) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "debugger connection closed",
            )),
            Ok(count) => {
                self.pending.drain(..count);
                self.write_progress = Instant::now();
                Ok(())
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl Connection for DebugConnection {
    type Error = io::Error;

    fn write(&mut self, byte: u8) -> io::Result<()> {
        Connection::write_all(self, &[byte])
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > 64 * 1024 - self.pending.len() {
            return Err(io::Error::other("debugger response budget exhausted"));
        }
        if self.pending.is_empty() {
            self.write_progress = Instant::now();
        }
        self.pending.extend(bytes);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
