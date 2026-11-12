// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

pub fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            if [".git", ".cheriot", "target", "build"]
                .iter()
                .any(|excluded| name == *excluded)
            {
                continue;
            }
            copy_tree(&entry.path(), &destination.join(name));
        } else if kind.is_file() {
            std::fs::copy(entry.path(), destination.join(name)).unwrap();
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(
                std::fs::read_link(entry.path()).unwrap(),
                destination.join(name),
            )
            .unwrap();
            #[cfg(not(unix))]
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &destination.join(name));
            } else {
                std::fs::copy(entry.path(), destination.join(name)).unwrap();
            }
        } else {
            panic!("unexpected source-tree entry: {}", entry.path().display());
        }
    }
}
