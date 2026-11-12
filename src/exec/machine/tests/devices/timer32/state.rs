// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn timer_has_no_host_sleep() {
    let mut timer = Timer32::new();
    timer.write(4, 4, 3).unwrap();
    timer.write(8, 4, 3).unwrap();
    timer.tick(2);
    assert_eq!(timer.interrupt(), None);
    timer.tick(1);
    assert_eq!(timer.interrupt(), Some(InterruptLine::MachineTimer));
    timer.write(12, 4, 1).unwrap();
    assert_eq!(timer.interrupt(), None);
}
