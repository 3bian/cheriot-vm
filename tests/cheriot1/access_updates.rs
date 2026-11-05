// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot1::{permissions, Capability};
use cheriot_vm::cheriot1::{Computer, StepOutcome};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};

#[test]
fn consecutive_loads_observe_current_revocation_bits_and_authority_permissions() {
    let mut bus = Bus::new();

    for (name, base) in [("program", 0x8000_0000), ("revocation", 0x8300_0000)] {
        bus.add_memory(
            MemoryRegion::new(name.into(), base, 4096, MemoryKind::Ram, Tagging::Cheriot1).unwrap(),
        )
        .unwrap();
    }

    let instructions = [
        0x0020_b023_u32,
        0x0000_b183,
        0x0000_b183,
        0x0000_b183,
        0x0000_b183,
    ];
    let bytes: Vec<_> = instructions
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    bus.install_firmware(0x8000_0000, &bytes).unwrap();
    let mut computer = Computer::new(bus, 0x8000_0000);
    let root = Capability::from_memory(0x7e3e_0000_0000_0000, true);
    let value = root.set_address(0x8000_0100).set_bounds(32, true).0;
    let authority = root.set_address(0x8000_0200);
    computer.processor.write_register(1, authority).unwrap();
    computer.processor.write_register(2, value).unwrap();
    assert!(matches!(computer.step(), StepOutcome::Retired { .. }));

    assert!(matches!(computer.step(), StepOutcome::Retired { .. }));
    assert_eq!(computer.processor.read_register(3).unwrap(), value);
    computer.bus.write(0x8300_0004, 1, 1).unwrap();
    assert!(matches!(computer.step(), StepOutcome::Retired { .. }));
    assert_eq!(
        computer.processor.read_register(3).unwrap(),
        value.clear_tag()
    );
    assert_eq!(computer.bus.memory()[0].tag_at(0x8000_0200), Some(true));

    computer.bus.write(0x8300_0004, 1, 0).unwrap();
    assert!(matches!(computer.step(), StepOutcome::Retired { .. }));
    assert_eq!(computer.processor.read_register(3).unwrap(), value);
    computer
        .processor
        .write_register(1, authority.restrict_permissions(permissions::STORE))
        .unwrap();
    assert_eq!(
        computer.step(),
        StepOutcome::Trapped {
            address: 0x8000_0010,
            cause: 28,
            value: 50,
        }
    );
    assert_eq!(computer.processor.read_register(3).unwrap(), value);
}
