// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::EmulatorError;

use super::{flow::UartFlow, parity::UartParity};

#[derive(Clone, Copy, Debug)]
pub struct UartSettings {
    pub baud: Option<u32>,
    pub data_bits: u8,
    pub flow: UartFlow,
    pub parity: UartParity,
    pub stop_bits: u8,
}

impl Default for UartSettings {
    fn default() -> Self {
        Self {
            baud: None,
            data_bits: 8,
            flow: UartFlow::None,
            parity: UartParity::None,
            stop_bits: 1,
        }
    }
}

impl UartSettings {
    pub fn validate(&self) -> Result<(), EmulatorError> {
        if !(5..=8).contains(&self.data_bits) || !(1..=2).contains(&self.stop_bits) {
            return Err(EmulatorError::Configuration(
                "UART requires data-bits=5..8 and stop-bits=1..2".into(),
            ));
        }
        if self.data_bits == 5 && self.stop_bits == 2 {
            return Err(EmulatorError::Configuration(
                "5 data bits with two stop bits is unsupported by the host UART contract".into(),
            ));
        }
        #[cfg(target_os = "linux")]
        if self.baud.is_some_and(|baud| baud_rate(baud).is_none()) {
            return Err(EmulatorError::Configuration(
                "unsupported UART baud rate; select a standard Linux rate such as 9600 or 115200"
                    .into(),
            ));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub(super) fn apply(
        &self,
        settings: &mut nix::sys::termios::Termios,
    ) -> Result<(), EmulatorError> {
        use nix::sys::termios::{cfmakeraw, cfsetspeed, ControlFlags, InputFlags};
        self.validate()?;
        cfmakeraw(settings);
        settings.control_flags.remove(
            ControlFlags::CSIZE
                | ControlFlags::PARENB
                | ControlFlags::PARODD
                | ControlFlags::CSTOPB
                | ControlFlags::CRTSCTS
                | ControlFlags::from_bits_retain(nix::libc::CMSPAR),
        );
        settings.control_flags.insert(
            ControlFlags::CLOCAL
                | ControlFlags::CREAD
                | match self.data_bits {
                    5 => ControlFlags::CS5,
                    6 => ControlFlags::CS6,
                    7 => ControlFlags::CS7,
                    _ => ControlFlags::CS8,
                },
        );
        match self.parity {
            UartParity::None => (),
            UartParity::Even => settings.control_flags.insert(ControlFlags::PARENB),
            UartParity::Odd => settings
                .control_flags
                .insert(ControlFlags::PARENB | ControlFlags::PARODD),
        }
        if self.stop_bits == 2 {
            settings.control_flags.insert(ControlFlags::CSTOPB);
        }
        settings
            .input_flags
            .remove(InputFlags::IXON | InputFlags::IXOFF | InputFlags::IXANY);
        match self.flow {
            UartFlow::None => (),
            UartFlow::RtsCts => settings.control_flags.insert(ControlFlags::CRTSCTS),
            UartFlow::XonXoff => settings
                .input_flags
                .insert(InputFlags::IXON | InputFlags::IXOFF),
        }
        if let Some(baud) = self.baud {
            let baud = baud_rate(baud)
                .ok_or_else(|| EmulatorError::Configuration("unsupported UART baud rate".into()))?;
            cfsetspeed(settings, baud).map_err(std::io::Error::from)?;
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/host/uart/settings.rs"]
mod tests;

#[cfg(target_os = "linux")]
fn baud_rate(baud: u32) -> Option<nix::sys::termios::BaudRate> {
    use nix::sys::termios::BaudRate;
    Some(match baud {
        50 => BaudRate::B50,
        75 => BaudRate::B75,
        110 => BaudRate::B110,
        134 => BaudRate::B134,
        150 => BaudRate::B150,
        200 => BaudRate::B200,
        300 => BaudRate::B300,
        600 => BaudRate::B600,
        1200 => BaudRate::B1200,
        1800 => BaudRate::B1800,
        2400 => BaudRate::B2400,
        4800 => BaudRate::B4800,
        9600 => BaudRate::B9600,
        19200 => BaudRate::B19200,
        38400 => BaudRate::B38400,
        57600 => BaudRate::B57600,
        115200 => BaudRate::B115200,
        230400 => BaudRate::B230400,
        460800 => BaudRate::B460800,
        500000 => BaudRate::B500000,
        576000 => BaudRate::B576000,
        921600 => BaudRate::B921600,
        1000000 => BaudRate::B1000000,
        1152000 => BaudRate::B1152000,
        1500000 => BaudRate::B1500000,
        2000000 => BaudRate::B2000000,
        2500000 => BaudRate::B2500000,
        3000000 => BaudRate::B3000000,
        3500000 => BaudRate::B3500000,
        4000000 => BaudRate::B4000000,
        _ => return None,
    })
}
