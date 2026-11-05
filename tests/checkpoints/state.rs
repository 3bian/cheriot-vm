// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot1::{Capability, Cheriot1System};
use cheriot_vm::devices::ethernet::Ethernet;
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::MachineConfiguration;

#[test]
fn checkpoints_include_stores_and_tag_clearing_without_observing_devices() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "ram0".into(),
            0x8000_0000,
            128,
            MemoryKind::Ram,
            Tagging::Cheriot1,
        )
        .unwrap(),
    )
    .unwrap();
    let mut system =
        Cheriot1System::from_raw_on_bus(bus, 0x8000_0000, &0x0020_b023_u32.to_le_bytes()).unwrap();
    let authority = Capability::from_memory(0x7e3e_0000_8000_0040, true);
    system
        .computer
        .processor
        .write_register(1, authority)
        .unwrap();
    system
        .computer
        .processor
        .write_register(2, authority)
        .unwrap();
    system.step().unwrap();
    let mut checkpoint = Vec::new();
    system.write_checkpoint_v1(&mut checkpoint).unwrap();
    assert_eq!(&checkpoint[..8], b"CHKP0001");
    assert_eq!(
        &checkpoint[619..627],
        &0x7e3e_0000_8000_0040_u64.to_le_bytes()
    );
    assert_eq!(&checkpoint[683..], &[0, 1]);
    system.computer.bus.write(0x8000_0047, 1, 0).unwrap();
    checkpoint.clear();
    system.write_checkpoint_v1(&mut checkpoint).unwrap();
    assert_eq!(&checkpoint[683..], &[0, 0]);
}

#[test]
fn faulting_capability_load_does_not_claim_a_pending_interrupt() {
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &0x0000_a103_u32.to_le_bytes())
        .unwrap();
    add_uart(&mut system);
    system
        .computer
        .bus
        .device_mut::<Uart>("uart1")
        .unwrap()
        .inject_receive(b"Q")
        .unwrap();
    system.computer.bus.write(0x1000_4008, 4, 1).unwrap();
    system.computer.bus.write(0x0c00_0004, 4, 2).unwrap();
    system
        .computer
        .processor
        .write_register(1, Capability::integer(0x0c00_0008))
        .unwrap();
    system.step().unwrap();
    assert_eq!(system.computer.processor.machine_registers.trap_cause, 28);
    assert_eq!(system.computer.bus.read(0x0c00_000c, 4).unwrap(), 0);
    assert_eq!(system.computer.bus.read(0x0c00_0008, 4).unwrap(), 1);
}

#[test]
fn faulting_capability_store_does_not_send_or_discard_a_queued_frame() {
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &0x0020_a023_u32.to_le_bytes())
        .unwrap();
    system.computer.bus.write(0x2000_0008, 4, 3).unwrap();
    system
        .computer
        .bus
        .write(0x2000_000c, 4, 0x0063_6261)
        .unwrap();
    system
        .computer
        .processor
        .write_register(1, Capability::integer(0x2000_0010))
        .unwrap();
    system
        .computer
        .processor
        .write_integer_register(2, 1)
        .unwrap();
    system.step().unwrap();
    assert_eq!(system.computer.processor.machine_registers.trap_cause, 28);
    assert_eq!(
        system
            .computer
            .bus
            .device_mut::<Ethernet>("ethernet0")
            .unwrap()
            .take_transmitted(),
        None
    );
    system.computer.bus.write(0x2000_0010, 4, 1).unwrap();
    assert_eq!(
        system
            .computer
            .bus
            .device_mut::<Ethernet>("ethernet0")
            .unwrap()
            .take_transmitted(),
        Some(b"abc".to_vec())
    );
}

#[test]
fn faulting_capability_load_preserves_uart_input() {
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &0x0000_a103_u32.to_le_bytes())
        .unwrap();
    add_uart(&mut system);
    system
        .computer
        .bus
        .device_mut::<Uart>("uart1")
        .unwrap()
        .inject_receive(b"Q")
        .unwrap();
    system
        .computer
        .processor
        .write_register(1, Capability::integer(0x1000_4000))
        .unwrap();
    system.step().unwrap();
    assert_eq!(system.computer.processor.machine_registers.trap_cause, 28);
    assert_eq!(
        system.computer.bus.read(0x1000_4000, 4).unwrap(),
        u32::from(b'Q')
    );
}

#[test]
fn faulting_capability_load_preserves_16550_uart_input() {
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &0x0000_a103_u32.to_le_bytes())
        .unwrap();
    system
        .computer
        .bus
        .device_mut::<Uart>("uart0")
        .unwrap()
        .inject_receive(b"Q")
        .unwrap();
    system
        .computer
        .processor
        .write_register(1, Capability::integer(0x1000_0000))
        .unwrap();
    system.step().unwrap();
    assert_eq!(system.computer.processor.machine_registers.trap_cause, 28);
    assert_eq!(system.computer.bus.read(0x1000_0014, 4).unwrap(), 0x21);
    assert_eq!(
        system.computer.bus.read(0x1000_0000, 4).unwrap(),
        u32::from(b'Q')
    );
}

fn add_uart(system: &mut Cheriot1System) {
    system
        .computer
        .bus
        .add_device("uart1".into(), 0x1000_4000, 256, Box::new(Uart::new()))
        .unwrap();
    system
        .computer
        .bus
        .set_interrupt_source("uart1", Some(1))
        .unwrap();
}
