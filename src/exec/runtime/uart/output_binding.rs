// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs::{File, OpenOptions};

use crate::exec::machine::EmulatorError;
use crate::exec::machine::host::open_flags::non_blocking;

use super::is_stdout_path;

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
        if is_stdout_path(path) {
            return Ok(None);
        }
        let file = non_blocking(OpenOptions::new().create(!debugger).write(true)).open(path)?;
        if debugger && file.metadata()?.is_file() {
            return Err(EmulatorError::Configuration(
                "debugger UART output requires a terminal, pipe or /dev/null; pipe stdout through tee for file capture".into(),
            ));
        }
        Ok(Some(Self { device, file }))
    }

    pub(crate) fn truncate(&self) -> Result<(), EmulatorError> {
        if self.file.metadata()?.is_file() {
            self.file.set_len(0)?;
        }
        Ok(())
    }
}
