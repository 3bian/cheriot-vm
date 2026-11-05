// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::i2c::{Eeprom, I2c};
use cheriot_vm::devices::Device;

#[test]
fn i2c_repeated_start_and_completion_acknowledgement() {
    let mut i = I2c::new();
    i.attach(0x50, Box::new(Eeprom::new())).unwrap();
    i.write(0, 4, 0x50).unwrap();
    i.write(16, 4, 1).unwrap();

    for value in [0x10, 0xab, 0xcd] {
        i.write(4, 4, value).unwrap();
        i.write(8, 4, 1).unwrap();
    }

    i.write(8, 4, 4).unwrap();
    i.write(4, 4, 0x10).unwrap();
    i.write(8, 4, 1).unwrap();
    i.write(8, 4, 4).unwrap();
    i.write(8, 4, 2).unwrap();
    assert_eq!(i.read(4, 4).unwrap(), 0xab);
    i.write(8, 4, 2).unwrap();
    assert_eq!(i.read(4, 4).unwrap(), 0xcd);
    assert!(i.interrupt().is_some());
    i.write(20, 4, 4).unwrap();
    assert_eq!(i.interrupt(), None);
    let before = i.read(12, 4).unwrap();
    assert!(i.write(8, 4, 0xff).is_err());
    assert_eq!(i.read(12, 4).unwrap(), before);
    i.write(0, 4, 0x51).unwrap();
    i.write(8, 4, 4).unwrap();
    assert_eq!(i.read(12, 4).unwrap() & 9, 8);
}
