// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::error::Error;
use std::fmt;

mod exception_cause;

pub use exception_cause::ExceptionCause;

#[derive(Debug)]
pub enum EmulatorError {
    Bus {
        address: u32,
        explanation: String,
    },
    CheriotTrap {
        address: u32,
        cause: u32,
        value: u32,
    },
    Configuration(String),
    Execution(String),
    Firmware(String),
    HostIo(std::io::Error),
    Interrupt {
        address: u32,
        cause: u32,
    },
    Trap {
        cause: ExceptionCause,
        address: u32,
        value: u32,
    },
}

impl fmt::Display for EmulatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(formatter, "configuration error: {message}"),
            Self::Execution(message) => write!(formatter, "execution error: {message}"),
            Self::Firmware(message) => write!(formatter, "firmware error: {message}"),
            Self::Bus {
                address,
                explanation,
            } => {
                write!(formatter, "bus error at 0x{address:08x}: {explanation}")
            }
            Self::CheriotTrap {
                address,
                cause,
                value,
            } => write!(
                formatter,
                "CHERIoT trap cause {cause:#x} at 0x{address:08x} (value 0x{value:08x})"
            ),
            Self::Trap {
                cause,
                address,
                value,
            } => write!(
                formatter,
                "processor exception {cause:?} at 0x{address:08x} (value 0x{value:08x})"
            ),
            Self::Interrupt { address, cause } => write!(
                formatter,
                "unhandled machine interrupt cause {cause} at 0x{address:08x}"
            ),
            Self::HostIo(error) => write!(formatter, "host I/O error: {error}"),
        }
    }
}

impl Error for EmulatorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::HostIo(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for EmulatorError {
    fn from(error: std::io::Error) -> Self {
        Self::HostIo(error)
    }
}
