// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::Read;
use std::path::PathBuf;

#[track_caller]
pub(crate) fn open_reference(name: &str) -> std::fs::File {
    let value = std::env::var(name).unwrap_or_else(|error| panic!("{name} is required: {error}"));
    let path = PathBuf::from(value);
    std::fs::File::open(&path)
        .unwrap_or_else(|error| panic!("{name} ({}): {error}", path.display()))
}

#[track_caller]
pub(crate) fn read_reference_text(name: &str) -> String {
    let mut text = String::new();
    open_reference(name)
        .read_to_string(&mut text)
        .unwrap_or_else(|error| panic!("cannot read {name} as UTF-8: {error}"));
    text
}
