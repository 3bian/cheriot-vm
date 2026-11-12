// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::socket::{MsgFlags, send};
use nix::sys::stat::{SFlag, fstat, stat};
use nix::unistd::{dup, isatty, write};

use crate::exec::machine::host::open_flags::non_blocking;
use crate::exec::machine::output::debugger_escape;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delivery {
    NonBlocking,
    Polled,
    Socket,
}

pub struct DebugOutput {
    delivery: Delivery,
    descriptor: OwnedFd,
    pending: VecDeque<u8>,
}

impl DebugOutput {
    pub const CAPACITY: usize = 64 * 1024;

    const POLLED_WRITE_BYTES: usize = 512;

    pub fn new(descriptor: impl AsFd) -> io::Result<Self> {
        let original = descriptor.as_fd();
        let metadata = fstat(original)?;
        let kind = SFlag::from_bits_truncate(metadata.st_mode);
        let null_device = kind == SFlag::S_IFCHR
            && stat("/dev/null").is_ok_and(|null| null.st_rdev == metadata.st_rdev);
        if !matches!(kind, SFlag::S_IFIFO | SFlag::S_IFSOCK)
            && !null_device
            && !isatty(original).unwrap_or(false)
        {
            return Err(io::Error::other(
                "debugger output requires a pipe, terminal or /dev/null; pipe through tee for file capture",
            ));
        }
        let (descriptor, delivery) = if kind == SFlag::S_IFSOCK {
            (dup(original)?, Delivery::Socket)
        } else if let Some(reopened) = reopen(original) {
            (reopened, Delivery::NonBlocking)
        } else {
            (dup(original)?, Delivery::Polled)
        };
        Ok(Self {
            delivery,
            descriptor,
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
        let bytes = self.pending.as_slices().0;
        let result = match self.delivery {
            Delivery::NonBlocking => write(&self.descriptor, bytes),
            Delivery::Polled => {
                if !self.can_write()? {
                    return Ok(());
                }
                let length = bytes.len().min(Self::POLLED_WRITE_BYTES);
                write(&self.descriptor, &bytes[..length])
            }
            Delivery::Socket => send(
                self.descriptor.as_raw_fd(),
                bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::from_bits_retain(nix::libc::MSG_NOSIGNAL),
            ),
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

    fn can_write(&self) -> io::Result<bool> {
        let mut descriptor = [PollFd::new(self.descriptor.as_fd(), PollFlags::POLLOUT)];
        match poll(&mut descriptor, PollTimeout::ZERO) {
            Ok(_) => Ok(descriptor[0]
                .revents()
                .is_some_and(|events| !events.is_empty())),
            Err(Errno::EINTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

fn reopen(original: BorrowedFd) -> Option<OwnedFd> {
    let is_non_blocking = |descriptor: BorrowedFd| {
        let flags = fcntl(descriptor, FcntlArg::F_GETFL).ok()?;
        Some(OFlag::from_bits_retain(flags).contains(OFlag::O_NONBLOCK))
    };
    if is_non_blocking(original)? {
        return None;
    }
    let reopened: OwnedFd = non_blocking(std::fs::OpenOptions::new().write(true))
        .open(format!("/proc/self/fd/{}", original.as_raw_fd()))
        .ok()?
        .into();
    (is_non_blocking(reopened.as_fd())? && !is_non_blocking(original)?).then_some(reopened)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn polled(descriptor: OwnedFd) -> DebugOutput {
        DebugOutput {
            delivery: Delivery::Polled,
            descriptor,
            pending: VecDeque::new(),
        }
    }

    #[test]
    fn a_descriptor_that_shares_its_blocking_state_is_never_waited_on() {
        let (reader, writer) = nix::unistd::pipe().unwrap();
        let mut reader = std::fs::File::from(reader);
        let mut output = polled(writer);
        let mut sent = 0;
        loop {
            output.enqueue(&[b'a'; 4096], true).unwrap();
            let before = output.pending();
            while output.pending() > 0 {
                let pending = output.pending();
                output.pump().unwrap();
                if output.pending() == pending {
                    break;
                }
            }
            sent += before - output.pending();
            if output.pending() > 0 {
                break;
            }
        }
        assert!(sent > 0);
        let waiting = output.pending();
        assert!(waiting > 0);
        output.pump().unwrap();
        assert_eq!(output.pending(), waiting, "a full pipe takes nothing");
        let mut received = vec![0; sent];
        reader.read_exact(&mut received).unwrap();
        output.pump().unwrap();
        assert!(output.pending() < waiting);
        assert!(received.iter().all(|byte| *byte == b'a'));
    }

    #[test]
    fn a_closed_reader_is_reported_and_not_waited_for() {
        let (reader, writer) = nix::unistd::pipe().unwrap();
        drop(reader);
        let mut output = polled(writer);
        output.enqueue(b"lost", true).unwrap();
        assert!(output.pump().is_err());
    }

    #[test]
    fn the_callers_descriptor_keeps_its_blocking_state() {
        let (_reader, writer) = nix::unistd::pipe().unwrap();
        let output = DebugOutput::new(&writer).unwrap();
        let flags = OFlag::from_bits_retain(fcntl(&writer, FcntlArg::F_GETFL).unwrap());
        assert!(!flags.contains(OFlag::O_NONBLOCK));
        assert_ne!(output.delivery, Delivery::Socket);
    }
}
