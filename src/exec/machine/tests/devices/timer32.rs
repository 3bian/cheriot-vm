// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::devices::Device;
use crate::exec::machine::devices::timer32::Timer32;

#[test]
fn timer_wrap_and_large_tick_counts() {
    let mut t = Timer32::new();
    t.write(0, 4, u32::MAX - 1).unwrap();
    t.write(4, 4, 1).unwrap();
    t.write(8, 4, 3).unwrap();
    t.tick(2);
    assert_eq!(t.read(0, 4).unwrap(), 0);
    assert!(t.interrupt().is_none());
    t.tick(1);
    assert!(t.interrupt().is_some());
    t.write(12, 4, 1).unwrap();
    t.tick((1u64 << 32) - 1);
    assert!(t.interrupt().is_none());
    t.tick(1);
    assert!(t.interrupt().is_some());
    t.tick(u64::MAX);
    assert_eq!(t.read(0, 4).unwrap(), 0);
}
