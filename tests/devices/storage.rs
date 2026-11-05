// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::storage::Storage;
use cheriot_vm::devices::Device;

#[test]
fn storage_image_roundtrip_errors_and_irq_ack() {
    let mut image = vec![0u8; 512];
    image[508..].copy_from_slice(&0x12345678u32.to_le_bytes());
    let mut storage = Storage::from_image(&image, false).unwrap();
    storage.write(20, 4, 1).unwrap();
    storage.write(12, 4, 1).unwrap();
    storage.write(4, 4, 127).unwrap();
    assert_eq!(storage.read(8, 4).unwrap(), 0x12345678);
    storage.write(8, 4, 0xdeadbeef).unwrap();
    storage.write(12, 4, 2).unwrap();
    assert_eq!(
        &storage.media_bytes().unwrap()[508..],
        &0xdeadbeefu32.to_le_bytes()
    );
    assert!(storage.interrupt().is_some());
    storage.write(16, 4, 3).unwrap();
    assert!(storage.interrupt().is_none());
    storage.write(0, 4, u32::MAX).unwrap();
    storage.write(12, 4, 2).unwrap();
    assert_eq!(storage.read(16, 4).unwrap(), 6);
    assert!(storage.write(4, 4, 128).is_err());
    assert_eq!(storage.read(4, 4).unwrap(), 127);
    assert!(Storage::from_image(&[], false).is_err());
    assert!(Storage::from_image(&[0; 513], false).is_err());
}
