// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::uart::{Uart, UartLayout};
use std::any::Any;

use cheriot_vm::bus::Bus;
use cheriot_vm::devices::gpio::Gpio;
use cheriot_vm::devices::interrupt_controller::InterruptController;
use cheriot_vm::devices::timer64::Timer64;
use cheriot_vm::devices::{Device, DeviceActivity, InterruptLine};
use cheriot_vm::EmulatorError;

#[derive(Default)]
struct CustomDevice {
    active: bool,
    failed: bool,
}

impl Device for CustomDevice {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn host_error(&self) -> Option<&str> {
        self.failed.then_some("test failure")
    }

    fn interrupt_pending(&self, _line: InterruptLine) -> bool {
        self.active
    }

    fn read(&mut self, _offset: u32, _width: usize) -> Result<u32, EmulatorError> {
        Ok(0)
    }

    fn write(&mut self, _offset: u32, _width: usize, value: u32) -> Result<(), EmulatorError> {
        self.active = value & 1 != 0;
        self.failed = value & 2 != 0;
        Ok(())
    }
}

#[test]
fn attachment_removal_and_replacement_rebuild_device_indices() {
    let mut bus = Bus::new();
    bus.add_device(
        "serial".into(),
        0,
        256,
        Box::new(Uart::with_layout(UartLayout::Ns16550)),
    )
    .unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineSoftware));
    bus.check_host_errors().unwrap();

    bus.add_device("timer".into(), 0x1000, 0xc0000, Box::new(Timer64::new()))
        .unwrap();
    bus.write(0x1000, 4, 1).unwrap();
    assert!(bus.interrupt_pending(InterruptLine::MachineSoftware));

    bus.remove_device("serial").unwrap();
    assert!(bus.interrupt_pending(InterruptLine::MachineSoftware));
    bus.replace_device("timer", Box::new(Uart::with_layout(UartLayout::Ns16550)))
        .unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineSoftware));

    bus.replace_device(
        "timer",
        Box::new(CustomDevice {
            active: true,
            failed: true,
        }),
    )
    .unwrap();
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    assert!(bus.check_host_errors().is_err());
    bus.remove_device("timer").unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.check_host_errors().unwrap();
}

#[test]
fn custom_devices_remain_fully_polled_without_activity_overrides() {
    let activity = DeviceActivity::default();
    assert!(activity.host_errors);
    assert_eq!(activity.interrupt_lines.len(), 3);

    let mut bus = Bus::new();
    bus.add_device("custom".into(), 0, 256, Box::<CustomDevice>::default())
        .unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineTimer));
    bus.check_host_errors().unwrap();

    bus.write(0, 4, 3).unwrap();

    for line in [
        InterruptLine::MachineExternal,
        InterruptLine::MachineSoftware,
        InterruptLine::MachineTimer,
    ] {
        assert!(bus.interrupt_pending(line));
    }

    assert!(bus
        .check_host_errors()
        .unwrap_err()
        .to_string()
        .contains("custom"));
    bus.write(0, 4, 0).unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineTimer));
    bus.check_host_errors().unwrap();
}

#[test]
fn routed_interrupts_are_not_direct_and_reconfiguration_clears_old_levels() {
    let mut bus = Bus::new();
    bus.add_device("pins".into(), 0x1000, 256, Box::new(Gpio::new()))
        .unwrap();
    bus.add_device(
        "controller".into(),
        0x2000,
        256,
        Box::new(InterruptController::new()),
    )
    .unwrap();
    bus.write(0x100c, 4, 1).unwrap();
    bus.device_mut::<Gpio>("pins").unwrap().set_inputs(1);
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));

    bus.set_interrupt_source("pins", Some(3)).unwrap();
    bus.refresh_interrupts();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(0x2000, 4).unwrap(), 8);
    bus.write(0x2004, 4, 8).unwrap();
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));

    bus.set_interrupt_source("pins", Some(4)).unwrap();
    bus.refresh_interrupts();
    assert_eq!(bus.read(0x2000, 4).unwrap(), 16);
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));

    bus.remove_device("pins").unwrap();
    bus.refresh_interrupts();
    assert_eq!(bus.read(0x2000, 4).unwrap(), 0);
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
}

#[test]
fn typed_mutable_access_keeps_interrupt_state_live() {
    let mut bus = Bus::new();
    bus.add_device("pins".into(), 0, 256, Box::new(Gpio::new()))
        .unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    let pins = bus.device_mut::<Gpio>("pins").unwrap();
    pins.write(12, 4, 1).unwrap();
    pins.set_inputs(1);
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));

    *bus.device_mut::<Gpio>("pins").unwrap() = Gpio::new();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
}
