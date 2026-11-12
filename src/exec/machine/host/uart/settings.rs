// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

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
        #[cfg(unix)]
        if self.baud.is_some_and(|baud| line_speed::of(baud).is_none()) {
            return Err(EmulatorError::Configuration(
                "unsupported UART baud rate: this system's terminals take standard rates \
                 such as 9600 and 115200"
                    .into(),
            ));
        }
        Ok(())
    }

    #[cfg(unix)]
    pub(super) fn apply(
        &self,
        settings: &mut nix::sys::termios::Termios,
    ) -> Result<(), EmulatorError> {
        use nix::sys::termios::{ControlFlags, InputFlags, cfmakeraw, cfsetspeed};
        self.validate()?;
        cfmakeraw(settings);
        settings.control_flags.remove(
            ControlFlags::CSIZE
                | ControlFlags::PARENB
                | ControlFlags::PARODD
                | ControlFlags::CSTOPB
                | ControlFlags::CRTSCTS
                | ControlFlags::from_bits_retain(line_speed::STICK_PARITY),
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
            let speed = line_speed::of(baud)
                .ok_or_else(|| EmulatorError::Configuration("unsupported UART baud rate".into()))?;
            cfsetspeed(settings, speed).map_err(std::io::Error::from)?;
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../tests/host/uart/settings.rs"]
mod tests;

#[cfg(unix)]
mod line_speed {
    use nix::libc::{self, speed_t, tcflag_t};
    use nix::sys::termios::BaudRate;

    const RATES: &[(u32, speed_t)] = &[
        (50, libc::B50),
        (75, libc::B75),
        (110, libc::B110),
        (134, libc::B134),
        (150, libc::B150),
        (200, libc::B200),
        (300, libc::B300),
        (600, libc::B600),
        (1200, libc::B1200),
        (1800, libc::B1800),
        (2400, libc::B2400),
        (4800, libc::B4800),
        (9600, libc::B9600),
        (19200, libc::B19200),
        (38400, libc::B38400),
        (57600, libc::B57600),
        (115200, libc::B115200),
        (230400, libc::B230400),
    ];

    #[cfg(target_os = "linux")]
    const FAST_RATES: &[(u32, speed_t)] = &[
        (460800, libc::B460800),
        (500000, libc::B500000),
        (576000, libc::B576000),
        (921600, libc::B921600),
        (1000000, libc::B1000000),
        (1152000, libc::B1152000),
        (1500000, libc::B1500000),
        (2000000, libc::B2000000),
        (2500000, libc::B2500000),
        (3000000, libc::B3000000),
        (3500000, libc::B3500000),
        (4000000, libc::B4000000),
    ];
    #[cfg(not(target_os = "linux"))]
    const FAST_RATES: &[(u32, speed_t)] = &[];

    #[cfg(target_os = "linux")]
    pub(super) const STICK_PARITY: tcflag_t = libc::CMSPAR;
    #[cfg(not(target_os = "linux"))]
    pub(super) const STICK_PARITY: tcflag_t = 0;

    fn convert<T, U: TryFrom<T>>(value: T) -> Option<U> {
        U::try_from(value).ok()
    }

    pub(super) fn of(baud: u32) -> Option<BaudRate> {
        let named = RATES
            .iter()
            .chain(FAST_RATES)
            .find(|(rate, _)| *rate == baud)
            .map(|(_, speed)| *speed);
        let speeds_are_rates = convert::<_, u64>(libc::B9600) == Some(9600);
        let speed = match named {
            Some(speed) => speed,
            None if speeds_are_rates && baud != 0 => convert(baud)?,
            None => return None,
        };
        BaudRate::try_from(speed).ok()
    }
}
