// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;

use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::sys::socket::{send, MsgFlags};
use nix::sys::stat::{fstat, major, minor, SFlag};
use nix::unistd::{dup, isatty, write};

use crate::output::debugger_escape;

pub struct DebugOutput {
    descriptor: OwnedFd,
    socket: bool,
    pending: VecDeque<u8>,
}

impl DebugOutput {
    pub const CAPACITY: usize = 64 * 1024;

    pub fn new(descriptor: impl AsFd) -> io::Result<Self> {
        let original = descriptor.as_fd();
        let metadata = fstat(original)?;
        let kind = SFlag::from_bits_truncate(metadata.st_mode);
        let null_device =
            kind == SFlag::S_IFCHR && major(metadata.st_rdev) == 1 && minor(metadata.st_rdev) == 3;
        if !matches!(kind, SFlag::S_IFIFO | SFlag::S_IFSOCK)
            && !null_device
            && !isatty(original).unwrap_or(false)
        {
            return Err(io::Error::other("debugger output requires a pipe, terminal or /dev/null; pipe through tee for file capture"));
        }
        let socket = kind == SFlag::S_IFSOCK;
        let descriptor = if socket {
            dup(original)?
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .custom_flags((OFlag::O_NONBLOCK | OFlag::O_NOCTTY).bits())
                .open(format!("/proc/self/fd/{}", original.as_raw_fd()))?
                .into()
        };
        Ok(Self {
            descriptor,
            socket,
            pending: VecDeque::new(),
        })
    }

    pub fn enqueue(&mut self, bytes: &[u8], raw: bool) -> io::Result<()> {
        let length = bytes
            .iter()
            .map(|byte| if raw { 1 } else { debugger_escape(*byte).1 })
            .sum::<usize>();
        if length > Self::CAPACITY - self.pending.len() {
            return Err(io::Error::other("debugger host output budget exhausted"));
        }
        for byte in bytes {
            if raw {
                self.pending.push_back(*byte);
            } else {
                let (escaped, length) = debugger_escape(*byte);
                self.pending.extend(escaped[..length].iter().copied());
            }
        }
        Ok(())
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    pub fn pump(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let result = if self.socket {
            send(
                self.descriptor.as_raw_fd(),
                self.pending.as_slices().0,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            )
        } else {
            write(&self.descriptor, self.pending.as_slices().0)
        };
        match result {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "debug output closed",
            )),
            Ok(count) => {
                self.pending.drain(..count);
                Ok(())
            }
            Err(Errno::EAGAIN | Errno::EINTR) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
