// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::guarded_output;
use crate::config::diagnostic::{Diagnostic, Result};
use crate::file_lock::FileLock;
use std::{fs, path::Path};

pub(super) struct BuildLock {
    _lock: FileLock,
}
impl BuildLock {
    pub(super) fn acquire(build_dir: &Path) -> Result<Self> {
        let path = build_dir.join(".cheriot.lock");
        guarded_output(&path, build_dir)?;
        fs::create_dir_all(build_dir).map_err(|e| Diagnostic::io(build_dir, e))?;
        let lock = FileLock::try_exclusive(&path).map_err(|e| {
            Diagnostic::new(format!(
                "build directory is locked or inaccessible: {}: {e}",
                path.display()
            ))
        })?;
        Ok(Self { _lock: lock })
    }
}
