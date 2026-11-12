// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn machine_exception_state() {
    let mut registers = MachineRegisters {
        trap_vector: 0x101,
        machine_status: 1 << 3,
        ..Default::default()
    };
    assert_eq!(registers.enter_trap(0x200, 7, 0, true), 0x11c);
    assert_eq!(registers.exception_program_counter, 0x200);
    assert_eq!(registers.trap_cause, (1 << 31) | 7);
    assert_eq!(registers.machine_return(), 0x200);
    assert_ne!(registers.machine_status & (1 << 3), 0);
}

#[test]
fn zero_register_remains_zero() {
    let profile = ProcessorProfile {
        embedded: false,
        multiply_divide: false,
        compressed: false,
        control_registers: true,
    };
    let mut processor = Processor::new(profile, 0);
    processor.write_register_unchecked(0, 23);
    assert_eq!(processor.read_register_unchecked(0), 0);
}
