// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::uart::{Uart, UartLayout};
use cheriot_vm::memory::Tagging;
use cheriot_vm::MachineConfiguration;

#[test]
fn public_profiles_share_peripherals_with_explicit_serial_and_tag_differences() {
    let mut profiles: Vec<_> = std::fs::read_dir("profiles")
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    profiles.sort();
    assert_eq!(profiles, ["cheriot1.toml", "rv32e.toml", "rv32imc.toml"]);
    let configuration =
        MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml")).unwrap();
    assert_eq!(configuration.name, "cheriot1");
    let mut expected_routes: Vec<_> = configuration
        .devices
        .iter()
        .map(|device| {
            (
                device.kind.clone(),
                device.name.clone(),
                device.base,
                device.interrupt_source,
            )
        })
        .collect();
    let system = configuration
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0])
        .unwrap();
    assert_eq!(system.computer.bus.memory()[0].size(), 0x0400_0000);
    assert!(system.computer.bus.device::<Uart>("uart0").is_some());
    assert_eq!(
        system
            .computer
            .bus
            .device::<Uart>("uart0")
            .unwrap()
            .layout(),
        UartLayout::Ns16550
    );
    let mut expected = [
        ("timer0", 0x0200_0000),
        ("interrupts", 0x0c00_0000),
        ("uart0", 0x1000_0000),
        ("gpio0", 0x1000_3000),
        ("spi0", 0x1000_1000),
        ("i2c0", 0x1000_2000),
        ("ethernet0", 0x2000_0000),
        ("storage0", 0x3000_0000),
    ];
    assert_eq!(
        system
            .computer
            .bus
            .devices()
            .iter()
            .map(|device| (device.name.as_str(), device.base))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        expected_routes[2],
        ("uart".into(), "uart0".into(), 0x1000_0000, None)
    );
    expected_routes[2] = ("uart".into(), "uart0".into(), 0x1000_0000, Some(1));
    expected[2] = ("uart0", 0x1000_0000);
    for (name, source, embedded) in [
        ("rv32e", include_str!("../../profiles/rv32e.toml"), true),
        (
            "rv32imc",
            include_str!("../../profiles/rv32imc.toml"),
            false,
        ),
    ] {
        let configuration = MachineConfiguration::parse(source).unwrap();
        assert_eq!(configuration.name, name);
        assert_eq!(configuration.processor_type, name);
        assert_eq!(configuration.processor_profile.embedded, embedded);
        assert_eq!(
            configuration.processor_profile.multiply_divide,
            name == "rv32imc"
        );
        assert_eq!(
            configuration.processor_profile.compressed,
            name == "rv32imc"
        );
        assert_eq!(configuration.memory.len(), 1);
        assert_eq!(configuration.memory[0].base, 0x8000_0000);
        assert_eq!(configuration.memory[0].size, 0x0400_0000);
        assert_eq!(configuration.memory[0].tagging, Tagging::None);
        assert_eq!(
            configuration
                .devices
                .iter()
                .map(|device| (
                    device.kind.clone(),
                    device.name.clone(),
                    device.base,
                    device.interrupt_source
                ))
                .collect::<Vec<_>>(),
            expected_routes
        );
        let machine = configuration.build(0x8000_0000).unwrap();
        assert_eq!(
            machine
                .bus
                .devices()
                .iter()
                .map(|device| (device.name.as_str(), device.base))
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn cheriot_profile_accepts_16550_uart_setup_without_emitting_divisor_bytes() {
    let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0])
        .unwrap();
    let bus = &mut system.computer.bus;
    bus.write(0x1000_0004, 4, 0).unwrap();
    bus.write(0x1000_000c, 4, 0x80).unwrap();
    bus.write(0x1000_0000, 4, 13).unwrap();
    bus.write(0x1000_0004, 4, 0).unwrap();
    bus.write(0x1000_000c, 4, 3).unwrap();
    bus.write(0x1000_0008, 4, 1).unwrap();
    assert_eq!(bus.read(0x1000_0014, 4).unwrap(), 0x20);
    bus.write(0x1000_0000, 4, u32::from(b'Z')).unwrap();
    assert_eq!(bus.device::<Uart>("uart0").unwrap().transmitted(), b"Z");
    assert_eq!(bus.read(0x1000_0000, 4).unwrap(), 0);
    assert!(bus.write(0x1000_0000, 1, 0).is_err());
    assert!(bus.write(0x1000_0100, 4, 0).is_err());
}

#[test]
fn all_public_profiles_build_at_their_declared_entry() {
    for profile in [
        include_str!("../../profiles/rv32e.toml"),
        include_str!("../../profiles/rv32imc.toml"),
    ] {
        let configuration = MachineConfiguration::parse(profile).unwrap();
        assert!(!configuration.is_cheriot1());
        configuration.build(0x8000_0000).unwrap();
    }
    let configuration =
        MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml")).unwrap();
    assert!(configuration.is_cheriot1());
    configuration
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0])
        .unwrap();
}
