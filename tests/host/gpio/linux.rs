// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(target_os = "linux")]

use std::fs;
use std::path::PathBuf;

use cheriot_vm::cheriot::{permissions, Capability, StepOutcome};
use cheriot_vm::devices::InterruptLine;
use cheriot_vm::host::gpio::linux;
use cheriot_vm::host::HostConnections;
use cheriot_vm::MachineConfiguration;

#[test]
#[ignore = "requires the Linux gpio-sim kernel driver; run ../verification/tools/test-gpio.sh"]
fn linux_gpio_character_device_reads_drives_and_releases_selected_lines() {
    let chip = PathBuf::from(std::env::var_os("CHERIOT_TEST_GPIO_CHIP").expect("GPIO chip path"));
    let simulator =
        PathBuf::from(std::env::var_os("CHERIOT_TEST_GPIO_SIM").expect("gpio-sim sysfs path"));
    let simulator = fs::canonicalize(simulator).unwrap();
    assert_eq!(simulator.file_name(), chip.file_name());
    assert!(simulator
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("gpio-sim."));

    let set_pull = |line, high| {
        fs::write(
            simulator.join(format!("sim_gpio{line}/pull")),
            if high { "pull-up" } else { "pull-down" },
        )
        .unwrap();
    };
    let value = |line| {
        fs::read_to_string(simulator.join(format!("sim_gpio{line}/value")))
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap()
    };

    set_pull(0, true);
    set_pull(1, true);
    set_pull(2, false);
    set_pull(3, false);

    let instructions = [0x0020_a223_u32, 0x0020_a423, 0x0000_a183];
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&instructions);
    let mut system = MachineConfiguration::parse(include_str!("../../../profiles/cheriot.toml"))
        .unwrap()
        .build_cheriot_raw(0x8000_0000, &bytes)
        .unwrap();
    HostConnections {
        gpio_chip: Some(("gpio0".into(), chip.to_str().unwrap().into())),
        gpio_lines: Some(vec![2, 0]),
        ..HostConnections::default()
    }
    .connect(&mut system.computer.bus)
    .unwrap();

    assert!(linux::open(&chip, &[2], Default::default()).is_err());
    assert_eq!(system.computer.bus.read(0x1000_3000, 4).unwrap(), 2);
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

    for _ in instructions {
        system.step().unwrap();
        assert!(matches!(
            system.last_outcome,
            Some(StepOutcome::Retired { .. })
        ));
    }

    assert_eq!(
        system
            .computer
            .processor
            .read_register(3)
            .unwrap()
            .address(),
        3
    );
    assert_eq!(value(2), 1);
    assert_eq!(value(0), 1);

    let bus = &mut system.computer.bus;
    bus.write(0x1000_3004, 4, 0).unwrap();
    assert_eq!(value(2), 0);
    bus.write(0x1000_3008, 4, 2).unwrap();
    assert_eq!(value(0), 0);
    bus.write(0x1000_3004, 4, 2).unwrap();
    assert_eq!(value(0), 1);
    bus.write(0x1000_3008, 4, 0).unwrap();
    bus.write(0x1000_300c, 4, 2).unwrap();
    bus.write(0x1000_3010, 4, u32::MAX).unwrap();
    set_pull(0, false);
    bus.tick_devices(1);
    assert_eq!(bus.read(0x1000_3000, 4).unwrap(), 0);
    set_pull(0, true);
    bus.tick_devices(1);
    assert!(bus.interrupt_pending(InterruptLine::MachineExternal));
    assert_eq!(bus.read(0x1000_3010, 4).unwrap(), 2);
    bus.write(0x1000_3010, 4, 2).unwrap();
    assert!(!bus.interrupt_pending(InterruptLine::MachineExternal));
    bus.check_host_errors().unwrap();

    assert_eq!(value(1), 1);
    assert_eq!(value(3), 0);
    let unselected = linux::open(&chip, &[1, 3], Default::default()).unwrap();
    drop(unselected);
    drop(system);
    let released = linux::open(&chip, &[0, 2], Default::default()).unwrap();
    drop(released);
}
