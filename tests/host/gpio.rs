// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;

use cheriot_vm::cheriot::StepOutcome;
use cheriot_vm::cheriot::{permissions, Capability};
use cheriot_vm::devices::gpio::{Gpio, LineTransport};
use cheriot_vm::devices::{Device, InterruptLine};
use cheriot_vm::host::gpio::linux;
use cheriot_vm::MachineConfiguration;

#[derive(Default)]
struct LineState {
    fail_read: bool,
    fail_write: bool,
    inputs: u32,
    reads: usize,
    writes: Vec<(u32, u32)>,
}

struct TestLines(Rc<RefCell<LineState>>);

impl LineTransport for TestLines {
    fn read(&mut self) -> io::Result<u32> {
        let mut state = self.0.borrow_mut();
        state.reads += 1;

        if state.fail_read {
            return Err(io::Error::other("test input disconnected"));
        }

        Ok(state.inputs)
    }

    fn write(&mut self, direction: u32, outputs: u32) -> io::Result<()> {
        let mut state = self.0.borrow_mut();

        if state.fail_write {
            return Err(io::Error::other("test output disconnected"));
        }

        state.writes.push((direction, outputs));
        Ok(())
    }
}

#[test]
fn capability_checks_precede_host_gpio_access_and_preserve_state_on_fault() {
    let root = Capability::from_memory(0x7e3e_0000_1000_3000, true);
    let sealed = Capability::from_memory(root.to_memory().0 | (1 << 54), true);
    let short = root.set_bounds(2, true).0;
    let cases = [
        (root.clear_tag(), 0x0020a423_u32, 28, 34),
        (sealed, 0x0020a423, 28, 35),
        (
            root.restrict_permissions(permissions::LOAD),
            0x0020a423,
            28,
            51,
        ),
        (short, 0x0020a423, 28, 33),
        (root.clear_tag(), 0x0000a183, 28, 34),
        (sealed, 0x0000a183, 28, 35),
        (
            root.restrict_permissions(permissions::STORE),
            0x0000a183,
            28,
            50,
        ),
        (short, 0x0000a183, 28, 33),
        (root, 0x0000b183, 5, 0x10003000),
        (root, 0x0020b023, 7, 0x10003000),
        (root, 0x0000a803, 2, 0),
    ];

    for (authority, instruction, cause, value) in cases {
        let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot.toml"))
            .unwrap()
            .build_cheriot_raw(0x80000000, &instruction.to_le_bytes())
            .unwrap();
        let (device, state) = host_device();
        *system.computer.bus.device_mut::<Gpio>("gpio0").unwrap() = device;
        system
            .computer
            .processor
            .write_register(1, authority)
            .unwrap();
        system
            .computer
            .processor
            .write_integer_register(2, 1)
            .unwrap();
        system
            .computer
            .processor
            .write_integer_register(3, 99)
            .unwrap();
        let reads = state.borrow().reads;
        let writes = state.borrow().writes.clone();

        assert_eq!(system.step().unwrap(), None);
        assert_eq!(
            system.last_outcome,
            Some(StepOutcome::Trapped {
                address: 0x80000000,
                cause,
                value,
            }),
            "{instruction:08x}, {authority:?}"
        );
        assert_eq!(
            system.computer.processor.read_register(1).unwrap(),
            authority
        );
        assert_eq!(
            system
                .computer
                .processor
                .read_register(3)
                .unwrap()
                .address(),
            99
        );
        assert_eq!(system.computer.last_store, None);
        assert_eq!(state.borrow().reads, reads);
        assert_eq!(state.borrow().writes, writes);
    }
}

#[test]
fn cheriot_guest_drives_outputs_and_reads_input_through_a_bounded_capability() {
    let instructions: [u32; 4] = [0x0020a223, 0x0020a423, 0x0000a183, 0x0000a403];
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&instructions);
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot.toml"))
        .unwrap()
        .build_cheriot_raw(0x80000000, &bytes)
        .unwrap();
    let (device, state) = host_device();
    *system.computer.bus.device_mut::<Gpio>("gpio0").unwrap() = device;
    let authority = Capability::from_memory(0x7e3e_0000_1000_3000, true)
        .set_bounds(20, true)
        .0
        .restrict_permissions(permissions::LOAD | permissions::STORE);
    system
        .computer
        .processor
        .write_register(1, authority)
        .unwrap();
    system
        .computer
        .processor
        .write_integer_register(2, 1)
        .unwrap();
    state.borrow_mut().inputs = 2;

    for _ in 0..instructions.len() {
        system.step().unwrap();
        assert!(matches!(
            system.last_outcome,
            Some(StepOutcome::Retired { .. })
        ));
    }

    assert_eq!(state.borrow().writes, [(0, 0), (0, 1), (1, 1)]);
    assert_eq!(
        system.computer.processor.read_register(3).unwrap(),
        Capability::integer(3)
    );
    assert_eq!(
        system.computer.processor.read_register(8).unwrap(),
        Capability::integer(3)
    );
}

#[test]
fn gpio_mapping_requires_an_absolute_chip_and_unique_bounded_line_list() {
    for (chip, lines) in [
        ("gpiochip0", vec![1]),
        ("/dev/gpiochip0", vec![]),
        ("/dev/gpiochip0", vec![1, 1]),
        ("/dev/gpiochip0", (0..33).collect()),
    ] {
        assert!(linux::validate_lines(Path::new(chip), &lines).is_err());
    }

    assert!(
        linux::validate_lines(Path::new("/dev/gpiochip0"), &(0..32).collect::<Vec<_>>()).is_ok()
    );
    assert!(linux::open(
        Path::new("/dev/cheriot-test-missing-gpiochip"),
        &[0],
        Default::default()
    )
    .is_err());
}

#[test]
fn host_gpio_inputs_outputs_and_interrupt_acknowledgement_match_simulated_registers() {
    let (mut device, state) = host_device();
    assert_eq!(state.borrow().writes, [(0, 0)]);
    device.write(4, 4, 1).unwrap();
    device.write(8, 4, 1).unwrap();
    device.write(12, 4, 3).unwrap();
    state.borrow_mut().inputs = 3;
    device.tick(1);

    assert_eq!(device.read(0, 4).unwrap(), 3);
    assert_eq!(device.read(16, 4).unwrap(), 2);
    assert_eq!(device.interrupt(), Some(InterruptLine::MachineExternal));
    assert_eq!(device.outputs(), 1);
    device.write(16, 4, 2).unwrap();
    assert_eq!(device.interrupt(), None);
    device.tick(1);
    assert_eq!(device.interrupt(), None);
    state.borrow_mut().inputs = 0;
    device.tick(1);
    state.borrow_mut().inputs = 2;
    device.tick(1);
    assert_eq!(device.read(16, 4).unwrap(), 2);
    assert_eq!(state.borrow().writes, [(0, 0), (0, 1), (1, 1)]);

    let reads = state.borrow().reads;
    assert!(device.read(0, 1).is_err());
    assert!(device.write(9, 4, 0).is_err());
    assert!(device.write(0, 4, 0).is_err());
    assert_eq!(state.borrow().reads, reads);
    assert_eq!(state.borrow().writes.len(), 3);
}

#[test]
fn host_gpio_failures_are_sticky_and_do_not_commit_output_registers() {
    let (mut device, state) = host_device();
    device.write(4, 4, 1).unwrap();
    device.write(8, 4, 1).unwrap();
    state.borrow_mut().fail_write = true;

    assert!(device.write(4, 4, 0).is_err());
    assert_eq!(device.outputs(), 1);
    assert!(device
        .host_error()
        .unwrap()
        .contains("test output disconnected"));
    let reads = state.borrow().reads;
    device.tick(1);
    assert_eq!(state.borrow().reads, reads);
    assert!(device.write(8, 4, 0).is_err());
    assert!(device.read(0, 4).is_err());
}

#[test]
fn asynchronous_host_gpio_failure_stops_cheriot_with_diagnostics() {
    let bytes = [0x13, 0, 0, 0].repeat(100);
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot.toml"))
        .unwrap()
        .build_cheriot_raw(0x80000000, &bytes)
        .unwrap();
    system.computer.timing = cheriot_vm::TimingPolicy::PerStep;
    system.computer.set_steps_per_tick(1).unwrap();
    let (device, state) = host_device();
    *system.computer.bus.device_mut::<Gpio>("gpio0").unwrap() = device;
    state.borrow_mut().fail_read = true;

    let error = system.step().unwrap_err().to_string();
    assert!(error.contains("gpio0"), "{error}");
    assert!(error.contains("test input disconnected"), "{error}");
    let steps = system.computer.attempted_steps;
    assert!(system.step().is_err());
    assert_eq!(system.computer.attempted_steps, steps);
}

#[test]
fn replacing_simulated_gpio_keeps_host_error_detection_active() {
    for typed_access in [false, true] {
        let mut bus = cheriot_vm::bus::Bus::new();
        bus.add_device("gpio0".into(), 0, 256, Box::new(Gpio::new()))
            .unwrap();
        bus.check_host_errors().unwrap();
        assert!(!bus.device::<Gpio>("gpio0").unwrap().activity().host_errors);

        let (device, state) = host_device();
        assert!(device.activity().host_errors);

        if typed_access {
            *bus.device_mut::<Gpio>("gpio0").unwrap() = device;
        } else {
            bus.replace_device("gpio0", Box::new(device)).unwrap();
        }

        state.borrow_mut().fail_read = true;
        bus.tick_devices(1);
        let error = bus.check_host_errors().unwrap_err().to_string();
        assert!(error.contains("gpio0"));
        assert!(error.contains("test input disconnected"));

        *bus.device_mut::<Gpio>("gpio0").unwrap() = Gpio::new();
        bus.check_host_errors().unwrap();
    }
}

fn host_device() -> (Gpio, Rc<RefCell<LineState>>) {
    let state = Rc::new(RefCell::new(LineState::default()));
    let device = Gpio::with_host(Box::new(TestLines(state.clone()))).unwrap();
    (device, state)
}
