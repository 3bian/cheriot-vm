// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::devices::Device;
use crate::exec::machine::devices::gpio::Gpio;

#[test]
fn gpio_does_not_interrupt_on_externally_changed_output_pins() {
    let mut g = Gpio::new();
    g.write(8, 4, 1).unwrap();
    g.write(12, 4, 3).unwrap();
    g.set_inputs(1);
    assert!(g.interrupt().is_none());
    g.set_inputs(3);
    assert!(g.interrupt().is_some());
    g.write(4, 4, 1).unwrap();
    assert_eq!(g.read(0, 4).unwrap(), 3);
    g.write(16, 4, 2).unwrap();
    assert!(g.interrupt().is_none());
}
