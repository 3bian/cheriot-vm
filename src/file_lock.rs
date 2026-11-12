// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

pub struct FileLock {
    _file: File,
}
impl FileLock {
    pub fn try_exclusive(path: &Path) -> io::Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(meta) if !meta.is_file() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "lock path is not a regular file",
                ));
            }
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lock path is not a regular file",
            ));
        }
        file.try_lock().map_err(io::Error::other)?;
        Ok(Self { _file: file })
    }
}
