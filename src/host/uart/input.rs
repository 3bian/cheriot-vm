// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::devices::uart::Uart;
use crate::EmulatorError;

pub struct UartInput {
    #[cfg(target_os = "linux")]
    descriptor: std::fs::File,
    device: String,
    eof: bool,
    #[cfg(target_os = "linux")]
    terminal: bool,
}

impl UartInput {
    #[cfg(target_os = "linux")]
    const READ_BUFFER_BYTES: usize = 1024;

    pub fn open(bus: &Bus, device: &str, path: &std::path::Path) -> Result<Self, EmulatorError> {
        Self::remaining(bus, device)?;
        let device = device.to_owned();
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let descriptor = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY)
                .open(path)?;
            if descriptor.metadata()?.is_dir() {
                return Err(EmulatorError::Configuration(
                    "UART input must be a readable stream".into(),
                ));
            }
            let terminal = nix::unistd::isatty(&descriptor).map_err(std::io::Error::from)?;
            Ok(Self {
                descriptor,
                device,
                eof: false,
                terminal,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (device, path);
            Err(EmulatorError::Configuration(
                "streaming uart input requires Linux".into(),
            ))
        }
    }

    pub fn poll(&mut self, bus: &mut Bus) -> Result<(), EmulatorError> {
        if self.eof {
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            use std::io::{ErrorKind, Read};
            let count = Self::remaining(bus, &self.device)?.min(Self::READ_BUFFER_BYTES);
            if count == 0 {
                return Ok(());
            }
            let terminal_hung_up = self.terminal && self.terminal_hung_up()?;
            let mut bytes = [0; Self::READ_BUFFER_BYTES];
            let length = match self.descriptor.read(&mut bytes[..count]) {
                Ok(0) => {
                    self.eof = !self.terminal || terminal_hung_up;
                    return Ok(());
                }
                Ok(length) => length,
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    return Ok(())
                }
                Err(error) => return Err(error.into()),
            };
            if let Some(uart) = bus.device_mut::<Uart>(&self.device) {
                uart.inject_receive(&bytes[..length])?;
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = (&self.device, bus);
        Ok(())
    }

    fn remaining(bus: &Bus, name: &str) -> Result<usize, EmulatorError> {
        if let Some(uart) = bus.device::<Uart>(name) {
            return Ok(uart.receive_remaining());
        }
        Err(EmulatorError::Configuration(format!(
            "no UART device named {name}"
        )))
    }

    #[cfg(target_os = "linux")]
    fn terminal_hung_up(&self) -> Result<bool, EmulatorError> {
        use std::os::fd::AsFd;

        use nix::errno::Errno;
        use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

        let mut descriptor = [PollFd::new(self.descriptor.as_fd(), PollFlags::POLLIN)];
        match poll(&mut descriptor, PollTimeout::ZERO) {
            Ok(_) => Ok(descriptor[0]
                .revents()
                .is_some_and(|events| events.contains(PollFlags::POLLHUP))),
            Err(Errno::EINTR) => Ok(false),
            Err(error) => Err(std::io::Error::from(error).into()),
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/host/uart/input.rs"]
mod tests;
