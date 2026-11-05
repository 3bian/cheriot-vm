// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs;
use std::process::{Command, Output};

use cheriot_vm::cheriot1::Cheriot1System;
use cheriot_vm::MachineConfiguration;

const HELLO: &str = "examples/hello_world/hello_world.elf";
const PROFILE: &str = include_str!("../../profiles/cheriot1.toml");

#[test]
fn cheriot_configuration_rejects_invalid_memory_devices_and_instruction_groups() {
    let configurations = [
        (
            PROFILE.replace("[[cheriot1-memory]]", "[[memory]]"),
            "must be tagged",
        ),
        (
            PROFILE.replace("base = 0x80000000", "base = 0x00000000"),
            "start at or above",
        ),
        (
            PROFILE.replace("size = 0x04000000", "size = 0x02000000"),
            "revocation bitmap",
        ),
        (
            PROFILE.replace(
                "type = \"cheriot1\"",
                "type = \"cheriot1\"\n[processor.instruction_groups]\ncompressed = false",
            ),
            "cannot be overridden",
        ),
        (
            PROFILE.replace("base = 0x02000000", "base = 0x10000000"),
            "overlaps",
        ),
        (
            PROFILE.replace(r#"layout = "16550""#, r#"layout = "unsupported""#),
            "unknown variant",
        ),
    ];

    for (text, expected) in configurations {
        let result = MachineConfiguration::parse(&text)
            .and_then(|config| config.build_cheriot1_raw(0x8000_0000, &[0x85, 0x00]));
        let error = match result {
            Ok(_) => panic!("invalid configuration was accepted: {text}"),
            Err(error) => error.to_string(),
        };
        assert!(
            error.contains(expected),
            "expected {expected:?} in {error:?}"
        );
    }
}

#[test]
fn cheriot_uart_flushes_on_limits_traps_and_breakpoints() {
    let directory = std::env::temp_dir();
    let firmware = directory.join(format!("cheriot-console-{}.bin", std::process::id()));
    let mut instructions = vec![0x03d0_00db_u32, 0x1000_0137, 0x2020_80db, 0x0410_0193];
    instructions.extend(std::iter::repeat_n(0x0030_a023, 1500));
    instructions.push(0x0000_0000);
    fs::write(
        &firmware,
        instructions
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();

    for (extra, success) in [
        (vec!["--steps", "1504"], false),
        (vec!["--trap-policy", "stop"], false),
        (vec!["--breakpoint", "0x80001780"], true),
    ] {
        let mut arguments = vec![
            "--profile",
            "profiles/cheriot1.toml",
            "--raw",
            firmware.to_str().unwrap(),
            "--address",
            "0x80000000",
        ];
        arguments.extend(extra);
        let result = run(&arguments);
        assert_eq!(result.status.success(), success);
        assert_eq!(
            result.stdout.iter().filter(|byte| **byte == b'A').count(),
            1500
        );
        assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
    }

    fs::remove_file(firmware).unwrap();
}

#[test]
fn cheriot_elf_runs_with_default_and_zero_step_limits() {
    for arguments in [
        vec!["--profile", "profiles/cheriot1.toml", "--elf", HELLO],
        vec![
            "--profile",
            "profiles/cheriot1.toml",
            "--elf",
            HELLO,
            "--steps",
            "0",
        ],
    ] {
        let result = run(&arguments);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            result.stdout,
            b"\x1b[35mHello world compartment\x1b[0m: Hello world\n"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("Guest exit status: 0; steps:"));
    }
}

#[test]
fn cheriot_generic_uart_can_receive_and_transmit() {
    let directory = std::env::temp_dir();
    let prefix = format!("cheriot-uart-{}", std::process::id());
    let profile = directory.join(format!("{prefix}.toml"));
    let firmware = directory.join(format!("{prefix}.bin"));
    let input = directory.join(format!("{prefix}.input"));
    let configuration = format!("{PROFILE}\n[[uart]]\nname = 'uart1'\nbase = 0x10004000\n");
    let instructions: [u32; 6] = [
        0x03d0_00db,
        0x1000_4137,
        0x2020_80db,
        0x0000_a183,
        0x0030_a023,
        0x0010_0073,
    ];
    let bytes: Vec<u8> = instructions
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    fs::write(&profile, configuration).unwrap();
    fs::write(&firmware, bytes).unwrap();
    fs::write(&input, b"Z").unwrap();

    let input_argument = format!("device=uart1,in={}", input.display());
    let result = run(&[
        "--profile",
        profile.to_str().unwrap(),
        "--raw",
        firmware.to_str().unwrap(),
        "--address",
        "0x80000000",
        "--uart",
        &input_argument,
        "--trap-policy",
        "stop",
        "--steps",
        "10",
        "--registers",
    ]);
    let unsupported_input = format!("device=gpio0,in={}", input.display());
    let rejected = run(&[
        "--profile",
        profile.to_str().unwrap(),
        "--raw",
        firmware.to_str().unwrap(),
        "--address",
        "0x80000000",
        "--uart",
        &unsupported_input,
    ]);

    fs::remove_file(profile).unwrap();
    fs::remove_file(firmware).unwrap();
    fs::remove_file(input).unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.starts_with('Z'), "{output:?}");
    assert!(output.contains("c03: address=0x0000005a"));
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("no UART device named"));
}

#[test]
fn cheriot_uart_receives_cli_input() {
    let directory = std::env::temp_dir();
    let prefix = format!("cheriot-console-input-{}", std::process::id());
    let firmware = directory.join(format!("{prefix}.bin"));
    let input = directory.join(format!("{prefix}.input"));
    let instructions: [u32; 6] = [
        0x03d0_00db,
        0x1000_0137,
        0x2020_80db,
        0x0000_a183,
        0x0030_a023,
        0x0010_0073,
    ];
    fs::write(
        &firmware,
        instructions
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(&input, b"Z").unwrap();
    let result = run(&[
        "--profile",
        "profiles/cheriot1.toml",
        "--raw",
        firmware.to_str().unwrap(),
        "--address",
        "0x80000000",
        "--uart",
        &format!("device=uart0,in={}", input.display()),
        "--trap-policy",
        "stop",
        "--steps",
        "10",
        "--registers",
    ]);
    fs::remove_file(firmware).unwrap();
    fs::remove_file(input).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.starts_with('Z'), "{output:?}");
    assert!(output.contains("c03: address=0x0000005a"));
}

#[test]
fn cheriot_profile_exposes_registers_memory_and_breakpoints() {
    let result = run(&[
        "--profile",
        "profiles/cheriot1.toml",
        "--elf",
        HELLO,
        "--breakpoint",
        "0x80000000",
        "--registers",
        "--memory",
        "address=0x80000000,length=4",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("Breakpoint at 0x80000000"));
    assert!(output.contains("c00: address=0x00000000"));
    assert!(output.contains("Program counter: 0x80000000"));
    assert!(output.contains("PCC: address=0x00000000"));
    assert!(output.contains("tag=1"));
    assert!(output.contains("Memory at 0x80000000:"));
    assert!(output.contains("0x80000000:"));
}

#[test]
fn cheriot_profile_matches_existing_platform_state_after_every_step() {
    let firmware = include_bytes!("../../examples/hello_world/hello_world.elf");
    let mut configured = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build_cheriot1(firmware)
        .unwrap();
    let mut existing = Cheriot1System::from_elf(firmware).unwrap();
    let mut configured_state = Vec::new();
    let mut existing_state = Vec::new();
    let mut configured_output = Vec::new();
    let mut existing_output = Vec::new();

    for step in 1..=2_000_000 {
        let actual_exit = configured.step().unwrap();
        let expected_exit = existing.step().unwrap();
        assert_eq!(actual_exit, expected_exit, "exit at step {step}");

        configured_state.clear();
        existing_state.clear();
        configured.write_state(&mut configured_state).unwrap();
        existing.write_state(&mut existing_state).unwrap();
        assert_eq!(configured_state, existing_state, "state at step {step}");

        if step % 1000 == 0 || actual_exit.is_some() {
            configured_output.extend(configured.take_output());
            existing_output.extend(existing.take_output());
        }

        if let Some(code) = actual_exit {
            assert_eq!(code, 0);
            assert!(step > 1000);
            assert_eq!(configured_output, existing_output);
            return;
        }
    }

    panic!("the configured guest did not finish");
}

#[test]
fn cheriot_raw_firmware_executes_and_stops_at_breakpoint_instruction() {
    let path = std::env::temp_dir().join(format!("cheriot-profile-{}.bin", std::process::id()));
    let firmware = [0x93, 0x00, 0x50, 0x00, 0x73, 0x00, 0x10, 0x00];
    fs::write(&path, firmware).unwrap();
    let result = run(&[
        "--profile",
        "profiles/cheriot1.toml",
        "--raw",
        path.to_str().unwrap(),
        "--address",
        "0x80000000",
        "--trap-policy",
        "stop",
        "--steps",
        "10",
        "--registers",
    ]);
    fs::remove_file(path).unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("Guest EBREAK at 0x80000004"));
    assert!(output.contains("c01: address=0x00000005"));
    assert!(output.contains("tag=0"));
}

#[test]
fn cheriot_respects_configured_memory_and_device_addresses() {
    let text = PROFILE
        .replace("size = 0x04000000", "size = 0x03100000")
        .replace("base = 0x10000000", "base = 0x10004000");
    let configuration = MachineConfiguration::parse(&text).unwrap();
    let system = configuration
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0x00])
        .unwrap();

    assert_eq!(system.computer.bus.memory()[0].size(), 0x0310_0000);
    assert!(system
        .computer
        .bus
        .devices()
        .iter()
        .any(|device| device.name == "uart0" && device.base == 0x1000_4000));

    let nonexecutable = text.replace(
        "[[cheriot1-memory]]",
        "[[cheriot1-memory]]\nexecutable = false",
    );
    assert!(MachineConfiguration::parse(&nonexecutable)
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0x00])
        .is_err());

    let mut altered_profile = MachineConfiguration::parse(&text).unwrap();
    altered_profile.processor_profile.control_registers = false;
    assert!(altered_profile
        .build_cheriot1_raw(0x8000_0000, &[0x85, 0x00])
        .is_err());
}

#[test]
fn cheriot_step_limit_and_stop_trap_are_reported_as_incomplete() {
    let limited = run(&[
        "--profile",
        "profiles/cheriot1.toml",
        "--elf",
        HELLO,
        "--steps",
        "1000",
    ]);
    assert!(!limited.status.success());
    let diagnostics = String::from_utf8(limited.stderr).unwrap();
    assert!(diagnostics.contains("step limit reached after 1000 steps"));
    assert!(diagnostics.contains("guest has not completed"));
    assert!(!diagnostics.contains("Guest exit status: 0"));

    let stopped = run(&[
        "--profile",
        "profiles/cheriot1.toml",
        "--elf",
        HELLO,
        "--trap-policy",
        "stop",
        "--steps",
        "200000",
    ]);
    assert!(!stopped.status.success());
    let diagnostics = String::from_utf8(stopped.stderr).unwrap();
    assert!(
        diagnostics.contains("CHERIoT trap cause 0x80000007"),
        "{diagnostics}"
    );
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(arguments)
        .output()
        .unwrap()
}
