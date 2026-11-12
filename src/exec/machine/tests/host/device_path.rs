// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::os::unix::fs::symlink;
use std::path::Path;

use super::{open, validate};

#[test]
fn device_paths_require_absolute_character_devices_for_validation_and_open() {
    for kind in ["I2C", "SPI"] {
        for error in [
            validate(Path::new("relative"), kind).unwrap_err(),
            open(Path::new("relative"), kind).unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("absolute device path"),
                "{kind}: {error}"
            );
        }
        validate(Path::new("/dev/null"), kind).unwrap();
        open(Path::new("/dev/null"), kind).unwrap();
    }
}

#[test]
fn opened_descriptor_is_checked_again_after_the_path_changes() {
    let directory = std::env::temp_dir().join(format!("emu-device-path-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("device");
    let regular = directory.join("regular");
    std::fs::write(&regular, b"unchanged").unwrap();
    symlink("/dev/null", &path).unwrap();
    validate(&path, "SPI").unwrap();
    std::fs::remove_file(&path).unwrap();
    symlink(&regular, &path).unwrap();
    let error = open(&path, "SPI").unwrap_err();
    assert!(error.to_string().contains("character device"), "{error}");
    assert_eq!(std::fs::read(&regular).unwrap(), b"unchanged");
    std::fs::remove_dir_all(directory).unwrap();
}
