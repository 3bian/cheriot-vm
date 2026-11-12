// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{ErrorKind, Read};

use crate::exec::machine::EmulatorError;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::devices::uart::Uart;

use super::source::Source;

pub struct UartInput {
    device: String,
    eof: bool,
    source: Source,
}

impl UartInput {
    const READ_BUFFER_BYTES: usize = 1024;

    pub fn open(bus: &Bus, device: &str, path: &std::path::Path) -> Result<Self, EmulatorError> {
        Self::remaining(bus, device)?;
        Ok(Self {
            device: device.to_owned(),
            eof: false,
            source: Source::open(path)?,
        })
    }

    pub fn poll(&mut self, bus: &mut Bus) -> Result<(), EmulatorError> {
        if self.eof {
            return Ok(());
        }
        let count = Self::remaining(bus, &self.device)?.min(Self::READ_BUFFER_BYTES);
        if count == 0 {
            return Ok(());
        }
        let hung_up = self.source.hung_up()?;
        let mut bytes = [0; Self::READ_BUFFER_BYTES];
        let length = match self.source.read(&mut bytes[..count]) {
            Ok(0) => {
                self.eof = hung_up;
                return Ok(());
            }
            Ok(length) => length,
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        if let Some(uart) = bus.device_mut::<Uart>(&self.device) {
            uart.inject_receive(&bytes[..length])?;
        }
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
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../tests/host/uart/input.rs"]
mod tests;
