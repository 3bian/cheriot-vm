// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::uart::Uart;
use cheriot_vm::devices::InterruptLine;
use cheriot_vm::MachineConfiguration;

#[test]
fn routed_interrupt_claim_complete_and_source_validation() {
    let text = include_str!("../../profiles/rv32imc.toml");
    let mut c = MachineConfiguration::parse(text)
        .unwrap()
        .build(0x8000_0000)
        .unwrap();
    c.bus
        .device_mut::<Uart>("uart0")
        .unwrap()
        .inject_receive(b"x")
        .unwrap();
    c.bus.write(0x10000008, 4, 1).unwrap();
    c.bus.refresh_interrupts();
    assert!(!c.bus.interrupt_pending(InterruptLine::MachineExternal));
    c.bus.write(0x0c000004, 4, 1 << 1).unwrap();
    assert!(c.bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(c.bus.read(0x0c000008, 4).unwrap(), 1);
    assert!(!c.bus.interrupt_pending(InterruptLine::MachineExternal));
    assert!(c.bus.write(0x0c000008, 4, 2).is_err());
    c.bus.write(0x0c000008, 4, 1).unwrap();
    assert!(c.bus.interrupt_pending(InterruptLine::MachineExternal));
    c.bus.read(0x10000000, 4).unwrap();
    c.bus.refresh_interrupts();
    assert!(!c.bus.interrupt_pending(InterruptLine::MachineExternal));
    assert!(MachineConfiguration::parse(
        &text.replace("interrupt_source = 1", "interrupt_source = 32")
    )
    .unwrap()
    .build(0x8000_0000)
    .is_err());
}
