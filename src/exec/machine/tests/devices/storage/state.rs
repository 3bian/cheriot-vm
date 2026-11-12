// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn sector_roundtrip_and_read_only_and_missing_media() {
    let mut storage = Storage::new(1, false).unwrap();
    storage.write(8, 4, 0x12345678).unwrap();
    storage.write(12, 4, 2).unwrap();
    storage.write(8, 4, 0).unwrap();
    storage.write(12, 4, 1).unwrap();
    assert_eq!(storage.read(8, 4).unwrap(), 0x12345678);
    storage.eject();
    storage.write(12, 4, 1).unwrap();
    assert_eq!(storage.read(16, 4).unwrap() & 2, 2);
    let mut readonly = Storage::new(1, true).unwrap();
    readonly.write(12, 4, 2).unwrap();
    assert_eq!(readonly.read(16, 4).unwrap() & 2, 2);
}
