// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::Cell;
use std::rc::Rc;

use cheriot_vm::bus::Bus;
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::devices::{Device, DeviceActivity, InterruptLine};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::EmulatorError;

struct CountedActivity(Rc<Cell<usize>>);

impl Device for CountedActivity {
    fn activity(&self) -> DeviceActivity {
        self.0.set(self.0.get() + 1);
        DeviceActivity::default()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn read(&mut self, _offset: u32, _width: usize) -> Result<u32, EmulatorError> {
        Ok(0)
    }

    fn write(&mut self, _offset: u32, _width: usize, _value: u32) -> Result<(), EmulatorError> {
        Ok(())
    }
}

#[test]
fn attaching_modified_detached_memory_revalidates_address_and_tag_invariants() {
    let mut bus = Bus::new();
    let mut memory =
        MemoryRegion::new("ram".into(), 0, 16, MemoryKind::Ram, Tagging::Cheriot).unwrap();
    memory.base = 1;
    assert!(bus.add_memory(memory).is_err());
    assert!(bus.memory().is_empty());
    let mut memory =
        MemoryRegion::new("ram".into(), 0, 16, MemoryKind::Ram, Tagging::None).unwrap();
    memory.base = u32::MAX;
    assert!(bus.add_memory(memory).is_err());
    assert!(bus.memory().is_empty());
}

#[test]
fn checked_topology_changes_reject_overlaps_duplicates_and_invalid_routes_atomically() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), 0x1000, 16, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();
    bus.install_firmware(0x1000, &[1, 2, 3, 4]).unwrap();
    bus.add_device("uart".into(), 0x2000, 256, Box::new(Uart::new()))
        .unwrap();
    for (name, base) in [("overlap", 0x100f), ("ram", 0x3000)] {
        assert!(bus
            .add_memory(
                MemoryRegion::new(name.into(), base, 16, MemoryKind::Ram, Tagging::None).unwrap()
            )
            .is_err());
    }
    assert_eq!(bus.memory().len(), 1);
    assert_eq!(bus.inspect(0x1000, 4).unwrap(), 0x0403_0201);
    bus.set_interrupt_source("uart", Some(3)).unwrap();
    for source in [0, 32, 255] {
        assert!(bus.set_interrupt_source("uart", Some(source)).is_err());
        assert_eq!(bus.devices()[0].interrupt_source, Some(3));
    }
    assert!(bus
        .replace_device("missing", Box::new(Uart::new()))
        .is_err());
    assert!(bus.set_memory_executable("missing", false).is_err());
    bus.set_memory_executable("ram", false).unwrap();
    assert!(bus.fetch(0x1000, 4).is_err());
    assert_eq!(bus.inspect(0x1000, 4).unwrap(), 0x0403_0201);
    let memory = bus.remove_memory("ram").unwrap();
    assert!(bus.inspect(0x1000, 4).is_err());
    bus.add_memory(memory).unwrap();
    assert!(bus.fetch(0x1000, 4).is_err());
}

#[test]
fn labelled_output_drain_preserves_polling_indexes_and_reuses_capture_capacity() {
    let mut bus = Bus::new();
    let activities = Rc::new(Cell::new(0));
    bus.add_device(
        "activity".into(),
        0x1000,
        256,
        Box::new(CountedActivity(activities.clone())),
    )
    .unwrap();
    bus.add_device("uart".into(), 0x2000, 256, Box::new(Uart::new()))
        .unwrap();
    bus.interrupt_pending(InterruptLine::MachineExternal);
    assert_eq!(activities.get(), 1);
    bus.write(0x2000, 4, u32::from(b'A')).unwrap();
    let mut scratch = Vec::with_capacity(4096);
    let original_capacity = scratch.capacity();
    let mut calls = 0;
    bus.drain_output_each(&mut scratch, |kind, name, bytes| {
        assert_eq!((kind, name, bytes), ("UART", "uart", b"A".as_slice()));
        calls += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(scratch.capacity(), original_capacity);
    bus.interrupt_pending(InterruptLine::MachineExternal);
    assert_eq!(activities.get(), 1);
    bus.drain_output_each(&mut scratch, |_, _, _| panic!("output drained twice"))
        .unwrap();
    bus.interrupt_pending(InterruptLine::MachineExternal);
    assert_eq!(activities.get(), 1);
}

#[test]
fn mapping_name_and_count_limits_are_enforced_for_embedding_builders() {
    let mut bus = Bus::new();
    for name in [String::new(), " ".into(), "x".repeat(4097)] {
        assert!(bus.add_device(name, 0, 1, Box::new(Uart::new())).is_err());
    }
    for index in 0..64 {
        bus.add_memory(
            MemoryRegion::new(
                format!("ram{index}"),
                index,
                1,
                MemoryKind::Ram,
                Tagging::None,
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert!(bus
        .add_memory(
            MemoryRegion::new("extra".into(), 64, 1, MemoryKind::Ram, Tagging::None).unwrap()
        )
        .is_err());
    assert_eq!(bus.memory().len(), 64);
}
