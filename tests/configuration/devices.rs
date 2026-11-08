// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::uart::Uart;
use std::fs;
use std::process::Command;

use cheriot_vm::cheriot::CheriotSystem;
use cheriot_vm::memory::Tagging;
use cheriot_vm::MachineConfiguration;

const BASE: &str = "[machine]\nname='sections'\n[processor]\ntype='rv32i'\n\
    [[memory]]\nname='ram'\ntype='ram'\nbase=0\nsize=4096\n";

#[test]
fn uart_16550_registers_and_cli_output_work_on_an_integer_profile() {
    let text = include_str!("../../profiles/rv32imc.toml").replace(
        "[[uart]]\nname = \"uart0\"\nbase = 0x10000000\ninterrupt_source = 1",
        "[[uart]]\nlayout='16550'\nname = \"uart0\"\nbase = 0x10000000",
    );
    let mut computer = MachineConfiguration::parse(&text)
        .unwrap()
        .build(0x8000_0000)
        .unwrap();
    assert!(computer.bus.device::<Uart>("uart0").is_some());
    computer.bus.write(0x1000_0000, 4, u32::from(b'A')).unwrap();
    assert_eq!(computer.bus.read(0x1000_0014, 4).unwrap(), 0x20);
    assert_eq!(computer.bus.read(0x1000_0000, 4).unwrap(), 0);

    let profile =
        std::env::temp_dir().join(format!("console-sections-{}.toml", std::process::id()));
    fs::write(&profile, text).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--raw",
            "examples/assembly/hello-uart.bin",
            "--address",
            "0x80000000",
            "--steps",
            "100",
        ])
        .output()
        .unwrap();
    fs::remove_file(profile).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("UART [uart0]: A"));
}

#[test]
fn default_cheriot_system_uses_consistent_device_names() {
    let system =
        CheriotSystem::from_elf(include_bytes!("../../examples/hello_world/hello_world.elf"))
            .unwrap();
    let names: Vec<_> = system
        .computer
        .bus
        .devices()
        .iter()
        .map(|device| device.name.as_str())
        .collect();

    assert_eq!(names.len(), 8);

    for name in [
        "uart0",
        "ethernet0",
        "gpio0",
        "timer0",
        "interrupts",
        "i2c0",
        "spi0",
        "storage0",
    ] {
        assert!(names.contains(&name));
    }
}

#[test]
fn named_devices_and_memory_preserve_interleaved_source_order() {
    let text = format!(
        "{BASE}\n\
        [[uart]]\nname='first'\nbase=0x1000\n\
        [[gpio]]\nname='second'\nbase=0x2000\n\
        [[uart]]\nname='third'\nbase=0x3000\n\
        [[timer32]]\nname='fourth'\nbase=0x4000\n\
        [[gpio]]\nname='fifth'\nbase=0x5000\n\
        [[cheriot-memory]]\nname='tagged'\ntype='ram'\nbase=0x8000\nsize=4096\n\
        [[memory]]\nname='last'\ntype='rom'\nbase=0x9000\nsize=4096\n"
    );
    let configuration = MachineConfiguration::parse(&text).unwrap();
    let computer = configuration.build(0).unwrap();
    let names: Vec<_> = computer
        .bus
        .devices()
        .iter()
        .map(|device| device.name.as_str())
        .collect();
    assert_eq!(names, ["first", "second", "third", "fourth", "fifth"]);
    let memory: Vec<_> = computer
        .bus
        .memory()
        .iter()
        .map(|region| (region.name.as_str(), region.tagging()))
        .collect();
    assert_eq!(
        memory,
        [
            ("ram", Tagging::None),
            ("tagged", Tagging::Cheriot),
            ("last", Tagging::None)
        ]
    );
}

#[test]
fn named_sections_keep_map_limits_duplicate_checks_and_host_access_rejection() {
    for devices in [
        "[[gpio]]\nname='same'\nbase=0x1000\n[[uart]]\nname='same'\nbase=0x2000\n",
        "[[gpio]]\nname='one'\nbase=0x1000\n[[uart]]\nname='two'\nbase=0x1000\n",
        "[[ethernet]]\nname='network'\nbase=0x1000\nhost_connection='tap:unused'\n",
        "[[uart]]\nlayout='16550'\nname='console'\nbase=0x1000\ninterrupt_source=1\n",
    ] {
        assert!(MachineConfiguration::parse(&format!("{BASE}\n{devices}"))
            .unwrap()
            .build(0)
            .is_err());
    }

    let mut text = BASE.to_string();
    for index in 0..65 {
        text.push_str(&format!(
            "\n[[gpio]]\nname='pins{index}'\nbase={}\n",
            0x1000 + index * 256
        ));
    }
    assert!(MachineConfiguration::parse(&text)
        .unwrap()
        .build(0)
        .is_err());
}

#[test]
fn obsolete_ambiguous_and_unsupported_sections_are_rejected() {
    for section in [
        "[[console]]\nname='removed'\nbase=0x1000\n",
        "[[devices]]\nname='serial'\ntype='uart'\nbase=0x1000\n",
        "[[clint]]\nname='timer'\nbase=0x2000000\n",
        "[[timer]]\nname='timer'\nbase=0x1000\n",
        "[[cheriot-uart]]\nname='serial'\nbase=0x1000\n",
        "[[cheriot-v2-memory]]\nname='future'\ntype='ram'\nbase=0x1000\nsize=8\n",
        "[[uart]]\nname='serial'\nbase=0x1000\ntype='console'\n",
        "[[gpio]]\nname='pins'\nbase=0x1000\nkind='uart'\n",
        "[[uart]]\nname='missing-base'\n",
        "[uart]\nname='not-an-array'\nbase=0x1000\n",
        "[[memory]]\nname='old-tags'\ntype='ram'\nbase=0x1000\nsize=8\ntagging='cheriot'\n",
    ] {
        assert!(
            MachineConfiguration::parse(&format!("{BASE}\n{section}")).is_err(),
            "{section}"
        );
    }
}

#[test]
fn input_on_one_uart_and_output_on_another_are_independent() {
    let directory =
        std::env::temp_dir().join(format!("cheriot-vm-console-uart-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();

    let profile = directory.join("profile.toml");
    let firmware = directory.join("firmware.bin");
    let input = directory.join("input.bin");
    let configuration = format!(
        "{}\n[[uart]]\nlayout='16550'\nname = 'uart1'\nbase = 0x10004000\n",
        include_str!("../../profiles/rv32imc.toml")
    );
    let instructions = [
        0x1000_00b7_u32,
        0x0000_a103,
        0x1000_41b7,
        0x0021_a023,
        0x0010_0073,
    ];
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&instructions);

    fs::write(&profile, configuration).unwrap();
    fs::write(&firmware, bytes).unwrap();
    fs::write(&input, b"Q").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--raw",
            firmware.to_str().unwrap(),
            "--address",
            "0x80000000",
            "--uart",
            &format!("device=uart0,in={}", input.display()),
            "--steps",
            "10",
        ])
        .output()
        .unwrap();

    let rejected = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--raw",
            firmware.to_str().unwrap(),
            "--address",
            "0x80000000",
            "--uart",
            &format!("device=gpio0,in={}", input.display()),
        ])
        .output()
        .unwrap();
    fs::remove_dir_all(directory).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("UART [uart1]: Q"));
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("no UART device named gpio0"));
}

#[test]
fn supplied_profiles_use_consistent_names_and_device_order() {
    let integer_devices: &[(&str, &str)] = &[
        ("timer64", "timer0"),
        ("interrupt-controller", "interrupts"),
        ("uart", "uart0"),
        ("gpio", "gpio0"),
        ("spi", "spi0"),
        ("i2c", "i2c0"),
        ("ethernet", "ethernet0"),
        ("storage", "storage0"),
    ];
    let cheriot_devices: &[(&str, &str)] = &[
        ("timer64", "timer0"),
        ("interrupt-controller", "interrupts"),
        ("uart", "uart0"),
        ("gpio", "gpio0"),
        ("spi", "spi0"),
        ("i2c", "i2c0"),
        ("ethernet", "ethernet0"),
        ("storage", "storage0"),
    ];
    let profiles = [
        (include_str!("../../profiles/cheriot.toml"), cheriot_devices),
        (include_str!("../../profiles/rv32e.toml"), integer_devices),
        (include_str!("../../profiles/rv32imc.toml"), integer_devices),
    ];

    for (profile, expected) in profiles {
        let configuration = MachineConfiguration::parse(profile).unwrap();
        let devices: Vec<_> = configuration
            .devices
            .iter()
            .map(|device| (device.kind.as_str(), device.name.as_str()))
            .collect();

        assert_eq!(configuration.memory[0].name, "ram0");
        assert_eq!(devices, expected);
    }
}

#[test]
fn supplied_rv32_profiles_use_the_uart_for_output() {
    for profile in ["profiles/rv32e.toml", "profiles/rv32imc.toml"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "--profile",
                profile,
                "--raw",
                "examples/assembly/hello-uart.bin",
                "--address",
                "0x80000000",
                "--steps",
                "10",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{profile}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("UART [uart0]: A"));
    }
}

#[test]
fn explicitly_supplied_foreign_device_options_are_rejected_even_at_defaults() {
    for kind in [
        "uart",
        "gpio",
        "timer32",
        "timer64",
        "ethernet",
        "interrupt-controller",
    ] {
        for option in [
            "attached_device='none'",
            "attached_device='loopback'",
            "sectors=16",
            "sectors=17",
            "read_only=false",
            "read_only=true",
        ] {
            let text = format!("{BASE}\n[[{kind}]]\nname='device'\nbase=0x10000000\n{option}\n");
            assert!(
                MachineConfiguration::parse(&text).is_err(),
                "{kind}: {option}"
            );
        }
    }
    for (kind, options) in [
        ("uart", ""),
        ("spi", "attached_device='none'"),
        ("i2c", "attached_device='eeprom'"),
        ("storage", "sectors=16\nread_only=false"),
    ] {
        MachineConfiguration::parse(&format!(
            "{BASE}\n[[{kind}]]\nname='device'\nbase=0x10000000\n{options}\n"
        ))
        .unwrap()
        .build(0)
        .unwrap();
    }
}

#[test]
fn uart_layout_is_rejected_for_other_device_types_in_both_parsed_and_public_configuration() {
    use cheriot_vm::devices::uart::UartLayout;
    for kind in [
        "ethernet", "gpio", "i2c", "spi", "storage", "timer32", "timer64",
    ] {
        let text = format!("{BASE}\n[[{kind}]]\nname='device'\nbase=0x10000000\nlayout='simple'\n");
        assert!(MachineConfiguration::parse(&text).is_err());
        let text = text.replace("layout='simple'\n", "");
        let mut config = MachineConfiguration::parse(&text).unwrap();
        config.devices[0].layout = Some(UartLayout::Simple);
        assert!(config.build(0).is_err(), "{kind}");
    }
}
