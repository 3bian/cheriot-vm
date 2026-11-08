// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::process::Command;

use cheriot_vm::devices::ethernet::Ethernet;
use cheriot_vm::devices::gpio::Gpio;
use cheriot_vm::host::HostConnections;
use cheriot_vm::MachineConfiguration;

const PROFILE: &str = include_str!("../../profiles/cheriot.toml");

#[cfg(target_os = "linux")]
#[test]
fn absent_gpio_chip_fails_with_its_path_instead_of_using_simulated_pins() {
    let chip =
        std::env::temp_dir().join(format!("cheriot-vm-absent-gpiochip-{}", std::process::id()));
    assert!(!chip.exists());

    for profile in ["profiles/cheriot.toml", "profiles/rv32imc.toml"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .args([
                "--profile",
                profile,
                "--raw",
                "examples/assembly/hello-uart.bin",
                "--address",
                "0x80000000",
                "--gpio",
                &format!("path={},lines=0", chip.display()),
                "--steps",
                "1",
            ])
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);

        assert!(!output.status.success());
        assert!(error.contains("cannot open GPIO chip"), "{error}");
        assert!(error.contains(chip.to_str().unwrap()), "{error}");
        assert!(!error.contains("panicked"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn cheriot_always_provides_in_memory_network_and_gpio_without_opening_host_devices() {
    let stripped = PROFILE
        .replace(
            "\n[[ethernet]]\nname = \"ethernet0\"\nbase = 0x20000000\ninterrupt_source = 5\n",
            "",
        )
        .replace(
            "\n[[gpio]]\nname = \"gpio0\"\nbase = 0x10003000\ninterrupt_source = 2\n",
            "",
        );
    assert!(!stripped.contains("[[ethernet]]"));
    assert!(!stripped.contains("[[gpio]]"));

    for profile in [PROFILE, stripped.as_str()] {
        let mut system = MachineConfiguration::parse(profile)
            .unwrap()
            .build_cheriot_raw(0x80000000, &0x00000013_u32.to_le_bytes())
            .unwrap();
        let bus = &mut system.computer.bus;
        HostConnections::default().connect(bus).unwrap();
        let network = bus.device_mut::<Ethernet>("ethernet0").unwrap();
        network.inject_frame(&[42; 14]).unwrap();
        assert_eq!(network.pending_frames(), 1);
        assert_eq!(bus.read(0x20000000, 4).unwrap(), 1);
        assert_eq!(bus.device::<Gpio>("gpio0").unwrap().outputs(), 0);
        assert_eq!(bus.read(0x10003000, 4).unwrap(), 0);
        assert_eq!(bus.devices().len(), 8);
    }

    let collision = stripped.replace("base = 0x10000000", "base = 0x20000000");
    assert!(MachineConfiguration::parse(&collision)
        .unwrap()
        .build_cheriot_raw(0x80000000, &0x00000013_u32.to_le_bytes())
        .is_err());

    let custom = PROFILE
        .replace("base = 0x20000000", "base = 0x21000000")
        .replace("name = \"ethernet0\"", "name = \"custom-network\"");
    let system = MachineConfiguration::parse(&custom)
        .unwrap()
        .build_cheriot_raw(0x80000000, &0x00000013_u32.to_le_bytes())
        .unwrap();
    assert!(system
        .computer
        .bus
        .device::<Ethernet>("custom-network")
        .is_some());
    assert!(system
        .computer
        .bus
        .device::<Ethernet>("ethernet0")
        .is_none());
}

#[test]
fn cli_host_options_reject_invalid_pairs_targets_and_repeated_values() {
    let cases = [
        (
            vec!["--tap", "interface=tap0", "--tap", "interface=tap1"],
            "duplicate TAP binding",
        ),
        (vec!["--tap", "device=ethernet0"], "requires interface="),
        (vec!["--tap", "interface="], "must not be empty"),
        (vec!["--tap", "device=,interface=tap0"], "must not be empty"),
        (
            vec!["--tap", "device=uart0,interface=tap0"],
            "no Ethernet device",
        ),
        (vec!["--tap", "interface=vm-absent0"], "TAP"),
        (vec!["--gpio", "path=/dev/gpiochip0"], "requires path="),
        (vec!["--gpio", "lines=1+2"], "requires path="),
        (
            vec!["--gpio", "device=uart0,path=/dev/gpiochip0,lines=1"],
            "no GPIO device",
        ),
        (
            vec!["--gpio", "path=gpiochip0,lines=1"],
            "absolute chip path",
        ),
        (vec!["--gpio", "path=/dev/gpiochip0,lines=1+1"], "unique"),
        (
            vec!["--gpio", "path=/dev/gpiochip0,lines=-1"],
            "invalid number",
        ),
        (
            vec!["--gpio", "path=/dev/gpiochip0,lines=4294967296"],
            "exceeds 32 bits",
        ),
        (
            vec!["--gpio", "path=/dev/gpiochip0,lines=1+"],
            "invalid number",
        ),
        (
            vec!["--gpio", "path=/dev/gpiochip0,lines=1,lines=2"],
            "only be supplied once",
        ),
        (
            vec!["--tap", "device=ethernet0,interface=tap0,device=ethernet1"],
            "only be supplied once",
        ),
    ];

    for (arguments, expected) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "--profile",
                "profiles/cheriot.toml",
                "--elf",
                "examples/hello_world/hello_world.elf",
                "--breakpoint",
                "0x80000000",
            ])
            .args(&arguments)
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{arguments:?}");
        assert!(error.contains(expected), "{arguments:?}: {error}");
        assert!(!error.contains("panicked"));
    }
}

#[test]
fn old_profile_backends_produce_a_migration_error() {
    for backend in ["in-memory", "disconnected", "tap:tap0"] {
        let profile = PROFILE.replace(
            "[[ethernet]]",
            &format!("[[ethernet]]\nhost_connection = '{backend}'"),
        );
        let error = match MachineConfiguration::parse(&profile)
            .unwrap()
            .build_cheriot_raw(0x80000000, &0x00000013_u32.to_le_bytes())
        {
            Ok(_) => panic!("profile accepted a host backend"),
            Err(error) => error.to_string(),
        };
        assert!(
            error.contains("--tap device=DEVICE,interface=INTERFACE"),
            "{error}"
        );
    }
}
