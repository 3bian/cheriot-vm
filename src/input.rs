// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

use crate::EmulatorError;

pub const MAX_FIRMWARE_BYTES: usize = 256 * 1024 * 1024;

pub fn read_file(
    path: impl AsRef<Path>,
    limit: usize,
    description: &str,
) -> Result<Vec<u8>, EmulatorError> {
    let path = path.as_ref();
    let validate = |metadata: fs::Metadata| {
        if !metadata.is_file() {
            return Err(EmulatorError::Configuration(format!(
                "{description} must be a regular file: {}",
                path.display()
            )));
        }
        if metadata.len() > limit as u64 {
            return Err(too_large(description, limit));
        }
        Ok(())
    };
    validate(fs::metadata(path)?)?;

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOCTTY);
    }
    let file = options.open(path)?;
    validate(file.metadata()?)?;
    read_bounded(file, limit, description)
}

fn read_bounded(
    reader: impl Read,
    limit: usize,
    description: &str,
) -> Result<Vec<u8>, EmulatorError> {
    let ceiling = u64::try_from(limit)
        .ok()
        .and_then(|limit| limit.checked_add(1))
        .ok_or_else(|| EmulatorError::Configuration("input limit is too large".into()))?;
    let mut bytes = Vec::new();
    reader.take(ceiling).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(too_large(description, limit));
    }
    Ok(bytes)
}

fn too_large(description: &str, limit: usize) -> EmulatorError {
    EmulatorError::Configuration(format!("{description} exceeds {limit} bytes"))
}

#[cfg(test)]
#[path = "../tests/input/tests.rs"]
mod tests;
