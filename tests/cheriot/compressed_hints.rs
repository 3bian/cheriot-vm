// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{Capability, Computer, StepOutcome};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::TimingPolicy;

fn computer(encoded: u16) -> Computer {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "code".into(),
            0x8000_0000,
            16,
            MemoryKind::Ram,
            Tagging::Cheriot,
        )
        .unwrap(),
    )
    .unwrap();
    bus.install_firmware(0x8000_0000, &encoded.to_le_bytes())
        .unwrap();
    Computer::new(bus, 0x8000_0000)
}

fn hint_encodings() -> BTreeSet<u16> {
    let mut result = BTreeSet::new();
    for immediate in 0_u16..64 {
        let field = ((immediate & 32) << 7) | ((immediate & 31) << 2);
        result.insert(0x0001 | field);
        result.insert(0x4001 | field);
        if immediate != 0 {
            result.insert(0x6001 | field);
        }
    }
    for register in 1_u16..32 {
        result.insert(0x0001 | (register << 7));
        result.insert(0x0002 | (register << 7));
        result.insert(0x8002 | (register << 2));
        result.insert(0x9002 | (register << 2));
    }
    for shift in 0_u16..32 {
        result.insert(0x0002 | (shift << 2));
    }
    for register in 0_u16..8 {
        result.insert(0x8001 | (register << 7));
        result.insert(0x8401 | (register << 7));
    }
    result
}

#[test]
fn all_compressed_hints_preserve_full_capabilities_and_retire_normally() {
    let encodings = hint_encodings();
    assert_eq!(encodings.len(), 363);
    for encoded in [0x0081, 0x0082, 0x8001, 0x8401] {
        assert!(encodings.contains(&encoded));
    }
    let states = [
        Capability::integer(0),
        Capability::from_memory(0x7e3e_0000_8000_0100, true),
        Capability::from_memory(0x7e7e_0000_8000_0100, true),
        Capability::from_memory(u64::MAX, false),
        Capability::from_memory(0x5f7e_0000_8000_0000, true),
    ];
    for timing in [TimingPolicy::PerStep, TimingPolicy::Elapsed] {
        for initial in states {
            for &encoded in &encodings {
                let mut machine = computer(encoded);
                machine.timing = timing;
                machine.fetched_instructions = 99;
                machine.processor.machine_registers.retired = 7;
                for index in 1..16 {
                    machine.processor.write_register(index, initial).unwrap();
                }
                let before = *machine.processor.registers();
                let pcc = machine.processor.pcc;
                assert_eq!(
                    machine.step(),
                    StepOutcome::Retired {
                        address: 0x8000_0000,
                        encoded: u32::from(encoded),
                    },
                    "{encoded:04x} {timing:?} {initial:?}"
                );
                assert_eq!(machine.processor.registers(), &before, "{encoded:04x}");
                assert_eq!(machine.processor.pcc, pcc);
                assert_eq!(machine.processor.program_counter, 0x8000_0002);
                assert_eq!(machine.processor.machine_registers.retired, 8);
                assert_eq!(machine.processor.machine_registers.cycle, 1);
                assert_eq!(
                    machine.processor.machine_registers.time,
                    u64::from(timing == TimingPolicy::PerStep)
                );
                assert_eq!(machine.fetched_instructions, 100);
                assert_eq!(machine.attempted_steps, 1);
                assert_eq!(machine.last_store, None);
            }
        }
    }
}

#[test]
fn zero_upper_immediates_and_rv32_shift_bit_five_remain_reserved() {
    for encoded in [0x6001, 0x6081, 0x6101, 0x1002, 0x1082, 0x9001, 0x9401] {
        let mut machine = computer(encoded);
        let initial = Capability::from_memory(0x7e3e_0000_8000_0100, true);
        for index in 1..16 {
            machine.processor.write_register(index, initial).unwrap();
        }
        let before = *machine.processor.registers();
        assert!(
            matches!(machine.step(), StepOutcome::Trapped { cause: 2, .. }),
            "{encoded:04x}"
        );
        assert_eq!(machine.processor.registers(), &before);
        assert_eq!(machine.processor.machine_registers.retired, 0);
    }
}
