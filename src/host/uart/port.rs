// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

use crate::EmulatorError;

use super::settings::UartSettings;

pub struct UartPort {
    #[cfg(target_os = "linux")]
    file: std::fs::File,
    #[cfg(target_os = "linux")]
    original: Option<nix::sys::termios::Termios>,
}

impl UartPort {
    pub fn open(path: &Path, settings: &UartSettings) -> Result<Self, EmulatorError> {
        settings.validate()?;
        #[cfg(target_os = "linux")]
        {
            use nix::sys::termios::{tcgetattr, tcsetattr, SetArg};
            use std::os::unix::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY)
                .open(path)?;
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
                || nix::sys::termios::cfgetispeed(&actual)
                    != nix::sys::termios::cfgetispeed(&configured)
                || nix::sys::termios::cfgetospeed(&actual)
                    != nix::sys::termios::cfgetospeed(&configured)
            {
                return Err(EmulatorError::Configuration(format!(
                    "host UART {} did not accept the requested settings",
                    path.display()
                )));
            }
            Ok(port)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err(EmulatorError::Configuration(
                "host UART settings require Linux".into(),
            ))
        }
    }

    pub fn restore(&mut self) -> Result<(), EmulatorError> {
        #[cfg(target_os = "linux")]
        if let Some(original) = &self.original {
            nix::sys::termios::tcdrain(&self.file).map_err(std::io::Error::from)?;
            nix::sys::termios::tcsetattr(&self.file, nix::sys::termios::SetArg::TCSANOW, original)
                .map_err(std::io::Error::from)?;
            self.original = None;
        }
        Ok(())
    }
}

impl Drop for UartPort {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
