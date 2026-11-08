// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(feature = "verification")]

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{Capability, Computer, StepOutcome};
use cheriot_vm::cheriot::{MemoryAccess, RegisterAccess};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};

fn computer() -> Computer {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "ram".into(),
            0x8000_0000,
            4096,
            MemoryKind::Ram,
            Tagging::Cheriot,
        )
        .unwrap(),
    )
    .unwrap();
    Computer::new(bus, 0x8000_0000)
}

#[test]
fn injection_uses_real_execution_without_overwriting_instruction_memory_or_tags() {
    let mut injected = computer();
    let tagged = Capability::from_memory(0x7e3e_0000_8000_0000, true);
    injected.processor.write_register(1, tagged).unwrap();
    injected.processor.write_register(2, tagged).unwrap();
    assert!(matches!(
        injected.step_injected(0x0020_b023),
        StepOutcome::Retired { .. }
    ));
    injected.processor = cheriot_vm::cheriot::Processor::new(0x8000_0000);
    injected.fetched_instructions = 0;
    assert_eq!(
        injected.step_injected(0x02a0_0093),
        StepOutcome::Retired {
            address: 0x8000_0000,
            encoded: 0x02a0_0093,
        }
    );
    assert_eq!(
        injected.processor.read_register(1).unwrap(),
        Capability::integer(42)
    );
    assert_eq!(injected.processor.program_counter, 0x8000_0004);
    assert_eq!(injected.processor.machine_registers.retired, 1);
    assert_eq!(injected.fetched_instructions, 1);
    assert_eq!(
        injected.bus.inspect(0x8000_0000, 4).unwrap(),
        tagged.address()
    );
    assert_eq!(
        injected.bus.inspect(0x8000_0004, 4).unwrap(),
        (tagged.to_memory().0 >> 32) as u32
    );
    assert_eq!(injected.bus.memory()[0].tag_at(0x8000_0000), Some(true));
}

#[test]
fn injected_and_fetched_instructions_agree_for_every_halfword() {
    let mut fetched = computer();
    let mut injected = computer();
    for encoded in 0_u32..=0xffff {
        fetched.processor = cheriot_vm::cheriot::Processor::new(0x8000_0000);
        injected.processor = cheriot_vm::cheriot::Processor::new(0x8000_0000);
        fetched
            .bus
            .install_firmware(0x8000_0000, &encoded.to_le_bytes())
            .unwrap();
        let actual = injected.step_injected(encoded);
        let expected = fetched.step();
        assert_eq!(actual, expected, "{encoded:04x}");
        assert_eq!(
            injected.processor.registers(),
            fetched.processor.registers(),
            "{encoded:04x}"
        );
        assert_eq!(
            injected.processor.program_counter, fetched.processor.program_counter,
            "{encoded:04x}"
        );
        assert_eq!(
            injected.processor.machine_exception, fetched.processor.machine_exception,
            "{encoded:04x}"
        );
        assert_eq!(
            injected.processor.machine_registers.retired,
            fetched.processor.machine_registers.retired,
            "{encoded:04x}"
        );
    }
}

#[test]
fn injection_keeps_fetch_authority_bounds_and_alignment_checks() {
    for (length, address, tag, cause, value) in [
        (8, 0x8000_0000, false, 28, 1026),
        (2, 0x8000_0000, true, 28, 1025),
        (8, 0x8000_0001, true, 0, 0x8000_0001),
    ] {
        let mut machine = computer();
        let pcc = machine
            .processor
            .pcc
            .set_address(0x8000_0000)
            .set_bounds(length, true)
            .0;
        machine.processor.pcc = Capability::from_memory(pcc.to_memory().0, tag);
        machine.processor.program_counter = address;
        assert_eq!(
            machine.step_injected(0x02a0_0093),
            StepOutcome::Trapped {
                address,
                cause,
                value
            }
        );
        assert_eq!(
            machine.processor.read_register(1).unwrap(),
            Capability::integer(0)
        );
        assert_eq!(machine.fetched_instructions, 0);
        assert_eq!(machine.processor.machine_registers.retired, 0);
    }
}

#[test]
fn observations_record_unchanged_writes_and_actual_memory_effects_only() {
    let mut machine = computer();
    machine.processor.write_integer_register(1, 42).unwrap();
    machine.step_injected(0x0000_8093);
    assert_eq!(
        machine.processor.register_accesses(),
        [
            RegisterAccess::Read {
                index: 1,
                value: Capability::integer(42)
            },
            RegisterAccess::Write {
                index: 1,
                value: Capability::integer(42)
            },
        ]
    );
    machine.processor.read_register(1).unwrap();
    assert_eq!(machine.processor.register_accesses().len(), 2);

    machine
        .processor
        .write_register(1, Capability::from_memory(0x7e3e_0000_8000_0100, true))
        .unwrap();
    machine
        .processor
        .write_integer_register(2, 0x1234_5678)
        .unwrap();
    machine.step_injected(0x0020_a023);
    assert_eq!(
        machine.memory_accesses(),
        [MemoryAccess::Write {
            address: 0x8000_0100,
            width: 4,
            value: 0x1234_5678,
            tag: None,
        }]
    );
    machine.step_injected(0x0000_a183);
    assert_eq!(
        machine.memory_accesses(),
        [MemoryAccess::Read {
            address: 0x8000_0100,
            width: 4,
            value: 0x1234_5678,
            tag: None,
        }]
    );
    machine
        .processor
        .write_register(1, Capability::integer(0x8000_0100))
        .unwrap();
    assert!(matches!(
        machine.step_injected(0x0020_a023),
        StepOutcome::Trapped { .. }
    ));
    assert!(machine.memory_accesses().is_empty());
    assert!(!machine
        .processor
        .register_accesses()
        .iter()
        .any(|access| matches!(access, RegisterAccess::Write { .. })));
    assert_eq!(machine.bus.inspect(0x8000_0100, 4).unwrap(), 0x1234_5678);
}
