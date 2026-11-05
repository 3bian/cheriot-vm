// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

use crate::EmulatorError;

pub(super) fn validate(path: &Path, kind: &str) -> Result<(), EmulatorError> {
    if !path.is_absolute() {
        return Err(EmulatorError::Configuration(format!(
            "{kind} requires an absolute device path"
        )));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileTypeExt;
        let metadata = std::fs::metadata(path).map_err(|error| {
            EmulatorError::HostIo(std::io::Error::new(
                error.kind(),
                format!("cannot open {kind} device {}: {error}", path.display()),
            ))
        })?;
        if !metadata.file_type().is_char_device() {
            return Err(EmulatorError::Configuration(format!(
                "{kind} path must name a character device: {}",
                path.display()
            )));
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err(EmulatorError::Configuration(format!(
        "physical {kind} requires Linux"
    )))
}

#[cfg(target_os = "linux")]
pub(super) fn open(path: &Path, kind: &str) -> Result<std::fs::File, EmulatorError> {
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
    if !path.is_absolute() {
        return Err(EmulatorError::Configuration(format!(
            "{kind} requires an absolute device path"
        )));
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY)
        .open(path)?;
    if !file.metadata()?.file_type().is_char_device() {
        return Err(EmulatorError::Configuration(format!(
            "{kind} path must name a character device: {}",
            path.display()
        )));
    }
    Ok(file)
}
