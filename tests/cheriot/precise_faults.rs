// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{permissions, Capability, CapabilityFaultCause, Computer, StepOutcome};
use cheriot_vm::devices::timer64::Timer64;
use cheriot_vm::devices::Device;
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::EmulatorError;

#[derive(Default)]
struct EffectDevice {
    reads: usize,
    writes: usize,
}

impl Device for EffectDevice {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn read(&mut self, _: u32, _: usize) -> Result<u32, EmulatorError> {
        self.reads += 1;
        Ok(0x1234_5678)
    }
    fn write(&mut self, _: u32, _: usize, _: u32) -> Result<(), EmulatorError> {
        self.writes += 1;
        Ok(())
    }
}

fn machine(encoded: u32) -> Computer {
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
    bus.add_memory(
        MemoryRegion::new(
            "shadow".into(),
            0x8300_0000,
            4096,
            MemoryKind::Ram,
            Tagging::None,
        )
        .unwrap(),
    )
    .unwrap();
    bus.install_firmware(0x8000_0000, &encoded.to_le_bytes())
        .unwrap();
    bus.add_device(
        "effects".into(),
        0x4000_0000,
        256,
        Box::<EffectDevice>::default(),
    )
    .unwrap();
    Computer::new(bus, 0x8000_0000)
}

fn root(address: u32) -> Capability {
    Capability::from_memory(0x7e3e_0000_0000_0000 | u64::from(address), true)
}

#[test]
fn failed_authority_and_register_checks_never_reach_mmio() {
    for encoded in [0x0000_a183, 0x0020_a023] {
        for (authority, expected) in [
            (root(0x4000_0000).clear_tag(), CapabilityFaultCause::Tag),
            (
                Capability::from_memory(0x7e7e_0000_4000_0000, true),
                CapabilityFaultCause::Seal,
            ),
            (
                root(0x4000_0000).restrict_permissions(0),
                if encoded & 0x7f == 3 {
                    CapabilityFaultCause::LoadPermission
                } else {
                    CapabilityFaultCause::StorePermission
                },
            ),
            (
                root(0x4000_0000).set_bounds(2, true).0,
                CapabilityFaultCause::Bounds,
            ),
        ] {
            let mut computer = machine(encoded);
            computer.processor.write_register(1, authority).unwrap();
            computer
                .processor
                .write_register(2, root(0x8000_0200))
                .unwrap();
            computer
                .processor
                .write_register(3, root(0x8000_0300))
                .unwrap();
            let registers = *computer.processor.registers();
            assert_eq!(
                computer.step(),
                StepOutcome::Trapped {
                    address: 0x8000_0000,
                    cause: 28,
                    value: 32 | expected as u32
                }
            );
            assert_eq!(computer.processor.registers(), &registers);
            let device = computer.bus.device::<EffectDevice>("effects").unwrap();
            assert_eq!((device.reads, device.writes), (0, 0));
            assert_eq!(computer.last_store, None);
            assert_eq!(computer.processor.machine_registers.retired, 0);
        }
    }
    for encoded in [0x0000_a803, 0x0100_a023] {
        let mut computer = machine(encoded);
        computer
            .processor
            .write_register(1, root(0x4000_0000))
            .unwrap();
        assert!(matches!(
            computer.step(),
            StepOutcome::Trapped { cause: 2, .. }
        ));
        let device = computer.bus.device::<EffectDevice>("effects").unwrap();
        assert_eq!((device.reads, device.writes), (0, 0));
    }
}

#[test]
fn capability_store_fault_precedence_preserves_payload_tags_and_completion_observation() {
    let address = 0x8000_0203;
    let bounded = root(0x8000_0200).set_bounds(2, true).0.set_address(address);
    for (authority, cause, value) in [
        (bounded.clear_tag(), 28, 34),
        (
            Capability::from_memory(bounded.to_memory().0 | (1 << 54), true),
            28,
            35,
        ),
        (bounded.restrict_permissions(permissions::LOAD), 28, 51),
        (bounded.restrict_permissions(permissions::STORE), 28, 53),
        (bounded, 28, 33),
        (root(address), 6, address),
        (root(0x9000_0000), 7, 0x9000_0000),
    ] {
        let mut computer = machine(0x0020_b023);
        let sentinel = root(0x8000_0100);
        computer
            .bus
            .write_capability(0x8000_0200, sentinel)
            .unwrap();
        computer
            .bus
            .write_capability(0x8000_0208, sentinel)
            .unwrap();
        computer.processor.write_register(1, authority).unwrap();
        computer
            .processor
            .write_register(2, root(0x8000_0400))
            .unwrap();
        assert_eq!(
            computer.step(),
            StepOutcome::Trapped {
                address: 0x8000_0000,
                cause,
                value
            }
        );
        assert_eq!(
            computer.bus.inspect_capability(0x8000_0200).unwrap(),
            sentinel
        );
        assert_eq!(
            computer.bus.inspect_capability(0x8000_0208).unwrap(),
            sentinel
        );
        assert_eq!(computer.last_store, None);
        assert_eq!(computer.processor.machine_registers.retired, 0);
    }
}

#[test]
fn revocation_bitmap_boundaries_use_base_and_leave_stored_tags_untouched() {
    for base in [0x8000_0000, 0x8000_0038, 0x8000_0040, 0x8003_fff8] {
        let mut computer = machine(0x0000_b183);
        let stored = root(base).set_bounds(8, true).0.set_address(base + 4);
        computer.bus.write_capability(0x8000_0200, stored).unwrap();
        computer
            .processor
            .write_register(1, root(0x8000_0200))
            .unwrap();
        computer
            .bus
            .write(
                0x8300_0000 + ((base - 0x8000_0000) >> 6),
                1,
                1 << ((base >> 3) & 7),
            )
            .unwrap();
        assert!(matches!(computer.step(), StepOutcome::Retired { .. }));
        assert_eq!(
            computer.processor.read_register(3).unwrap(),
            stored.clear_tag()
        );
        assert_eq!(
            computer.bus.inspect_capability(0x8000_0200).unwrap(),
            stored
        );
    }
    let mut computer = machine(0x0000_b183);
    let stored = root(0x8004_0000).set_bounds(8, true).0;
    let destination = root(0x8000_0100);
    computer.bus.write_capability(0x8000_0200, stored).unwrap();
    computer
        .processor
        .write_register(1, root(0x8000_0200))
        .unwrap();
    computer.processor.write_register(3, destination).unwrap();
    assert_eq!(
        computer.step(),
        StepOutcome::Trapped {
            address: 0x8000_0000,
            cause: 5,
            value: 0x8000_0200
        }
    );
    assert_eq!(computer.processor.read_register(3).unwrap(), destination);
    assert_eq!(
        computer.bus.inspect_capability(0x8000_0200).unwrap(),
        stored
    );
}

#[test]
fn sentry_enable_takes_pending_interrupt_at_next_instruction_boundary() {
    let mut computer = machine(0x0005_00e7);
    computer
        .bus
        .add_device(
            "timer".into(),
            0x0200_0000,
            0xc0000,
            Box::new(Timer64::new()),
        )
        .unwrap();
    computer.bus.write(0x0200_0000, 4, 1).unwrap();
    computer.processor.machine_registers.interrupt_enable = 8;
    let sentry = Capability::from_memory(0x5efe_0000_8000_0010, true);
    computer.processor.write_register(10, sentry).unwrap();
    assert!(matches!(computer.step(), StepOutcome::Retired { .. }));
    assert_eq!(computer.processor.program_counter, 0x8000_0010);
    assert_eq!(computer.processor.machine_registers.machine_status & 8, 8);
    assert_eq!(
        computer.processor.read_register(1).unwrap().object_type(),
        4
    );
    let return_capability = computer.processor.read_register(1).unwrap();
    assert_eq!(
        computer.step(),
        StepOutcome::Trapped {
            address: 0x8000_0010,
            cause: 0x8000_0003,
            value: 0
        }
    );
    assert_eq!(
        computer.processor.read_register(1).unwrap(),
        return_capability
    );
    assert_eq!(computer.processor.machine_exception.address(), 0x8000_0010);
    assert!(computer.processor.machine_exception.tag());
    assert!(!computer.processor.machine_exception.sealed());
    assert_eq!(computer.processor.machine_registers.retired, 1);
    assert_eq!(computer.fetched_instructions, 1);
}
