// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs::File;
use std::io;
use std::path::Path;

pub(super) fn character_device(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let metadata = std::fs::metadata(path).ok()?;
        metadata
            .file_type()
            .is_char_device()
            .then(|| metadata.rdev())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

pub(super) fn of(file: &File) -> io::Result<Option<(u64, u64)>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(Some((metadata.dev(), metadata.ino())))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(None)
    }
}

pub(super) fn standard_output_file() -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        let output = File::from(io::stdout().as_fd().try_clone_to_owned().ok()?);
        if !output.metadata().ok()?.is_file() {
            return None;
        }
        of(&output).ok()?
    }
    #[cfg(not(unix))]
    None
}
