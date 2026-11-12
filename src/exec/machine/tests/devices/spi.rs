// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::devices::Device;
use crate::exec::machine::devices::spi::{LoopbackPeripheral, Spi};

#[test]
fn spi_completion_ack_and_invalid_selection_are_atomic() {
    let mut s = Spi::new();
    s.attach(0, Box::new(LoopbackPeripheral)).unwrap();
    s.write(8, 4, 0).unwrap();
    s.write(16, 4, 1).unwrap();

    for mode in 0..4 {
        s.write(12, 4, mode).unwrap();
        s.write(0, 4, 0xa5).unwrap();
        assert_eq!(s.read(0, 4).unwrap(), 0xa5);
    }

    assert!(s.interrupt().is_some());
    s.write(20, 4, 1).unwrap();
    assert!(s.interrupt().is_none());
    assert!(s.write(8, 4, 4).is_err());
    assert_eq!(s.read(8, 4).unwrap(), 0);
}
