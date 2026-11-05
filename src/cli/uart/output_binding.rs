// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs::{File, OpenOptions};

use cheriot_vm::EmulatorError;

pub(crate) struct UartOutputBinding {
    pub device: String,
    pub file: File,
}

impl UartOutputBinding {
    pub(crate) fn open(
        device: String,
        path: &str,
        debugger: bool,
    ) -> Result<Option<Self>, EmulatorError> {
        if matches!(path, "/dev/stdout" | "/dev/fd/1" | "/proc/self/fd/1") {
            return Ok(None);
        }
        if debugger && std::fs::metadata(path)?.is_file() {
            return Err(EmulatorError::Configuration(
                "debugger UART output requires a terminal, pipe or /dev/null; pipe stdout through tee for file capture".into(),
            ));
        }
        let mut options = OpenOptions::new();
        options.create(!debugger).truncate(!debugger).write(true);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY);
        }
        Ok(Some(Self {
            device,
            file: options.open(path)?,
        }))
    }
}
