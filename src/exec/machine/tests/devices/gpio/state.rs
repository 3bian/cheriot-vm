// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn rising_input_triggers_masked_interrupt() {
    let mut gpio = Gpio::new();
    gpio.write(12, 4, 1).unwrap();
    gpio.set_inputs(1);
    assert_eq!(gpio.interrupt(), Some(InterruptLine::MachineExternal));
    gpio.write(16, 4, 1).unwrap();
    assert_eq!(gpio.interrupt(), None);
}
