// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};

pub(crate) fn directories(kind: &str) -> Vec<PathBuf> {
    let Some(bin) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    [
        bin.join(kind),
        bin.join("..").join("share").join("cheriot").join(kind),
        bin.join("..").join("..").join(kind),
    ]
    .into_iter()
    .map(|directory| std::fs::canonicalize(&directory).unwrap_or(directory))
    .collect()
}
