// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

use crate::exec::machine::EmulatorError;

pub(super) fn validate(path: &Path, kind: &str) -> Result<(), EmulatorError> {
    validate_absolute(path, kind)?;
    #[cfg(target_os = "linux")]
    {
        let metadata = std::fs::metadata(path).map_err(|error| {
            EmulatorError::HostIo(std::io::Error::new(
                error.kind(),
                format!("cannot open {kind} device {}: {error}", path.display()),
            ))
        })?;
        validate_character_device(&metadata, path, kind)
    }
    #[cfg(not(target_os = "linux"))]
    Err(EmulatorError::Configuration(format!(
        "a host {kind} bus is reached through a Linux device file, which this system does not have"
    )))
}

#[cfg(target_os = "linux")]
pub(super) fn open(path: &Path, kind: &str) -> Result<std::fs::File, EmulatorError> {
    validate_absolute(path, kind)?;
    let file = super::open_flags::non_blocking(std::fs::OpenOptions::new().read(true).write(true))
        .open(path)?;
    validate_character_device(&file.metadata()?, path, kind)?;
    Ok(file)
}

fn validate_absolute(path: &Path, kind: &str) -> Result<(), EmulatorError> {
    if !path.is_absolute() {
        return Err(EmulatorError::Configuration(format!(
            "{kind} requires an absolute device path"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_character_device(
    metadata: &std::fs::Metadata,
    path: &Path,
    kind: &str,
) -> Result<(), EmulatorError> {
    use std::os::unix::fs::FileTypeExt;

    if !metadata.file_type().is_char_device() {
        return Err(EmulatorError::Configuration(format!(
            "{kind} path must name a character device: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/host/device_path.rs"]
mod tests;
