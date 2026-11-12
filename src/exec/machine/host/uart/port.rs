// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

use crate::exec::machine::EmulatorError;

use super::settings::UartSettings;

#[cfg(unix)]
pub struct UartPort {
    file: std::fs::File,
    original: Option<nix::sys::termios::Termios>,
}

#[cfg(unix)]
impl UartPort {
    pub fn open(path: &Path, settings: &UartSettings) -> Result<Self, EmulatorError> {
        use nix::sys::termios::{SetArg, cfgetispeed, cfgetospeed, tcgetattr, tcsetattr};

        use crate::exec::machine::host::open_flags::non_blocking;

        settings.validate()?;
        let file = non_blocking(std::fs::OpenOptions::new().read(true).write(true)).open(path)?;
        let original = tcgetattr(&file).map_err(std::io::Error::from)?;
        let mut configured = original.clone();
        settings.apply(&mut configured)?;
        let port = Self {
            file,
            original: Some(original),
        };
        tcsetattr(&port.file, SetArg::TCSANOW, &configured).map_err(std::io::Error::from)?;
        let actual = tcgetattr(&port.file).map_err(std::io::Error::from)?;
        if actual.control_flags != configured.control_flags
            || actual.input_flags != configured.input_flags
            || actual.output_flags != configured.output_flags
            || actual.local_flags != configured.local_flags
            || cfgetispeed(&actual) != cfgetispeed(&configured)
            || cfgetospeed(&actual) != cfgetospeed(&configured)
        {
            return Err(EmulatorError::Configuration(format!(
                "host UART {} did not accept the requested settings",
                path.display()
            )));
        }
        Ok(port)
    }

    pub fn restore(&mut self) -> Result<(), EmulatorError> {
        use nix::sys::termios::{SetArg, tcdrain, tcsetattr};

        if let Some(original) = &self.original {
            tcdrain(&self.file).map_err(std::io::Error::from)?;
            tcsetattr(&self.file, SetArg::TCSANOW, original).map_err(std::io::Error::from)?;
            self.original = None;
        }
        Ok(())
    }
}

#[cfg(not(unix))]
pub struct UartPort;

#[cfg(not(unix))]
impl UartPort {
    pub fn open(path: &Path, settings: &UartSettings) -> Result<Self, EmulatorError> {
        settings.validate()?;
        Err(EmulatorError::Configuration(format!(
            "UART baud, framing and flow settings for {} need a Unix system: this one has no terminal settings to change",
            path.display()
        )))
    }

    pub fn restore(&mut self) -> Result<(), EmulatorError> {
        Ok(())
    }
}

impl Drop for UartPort {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
