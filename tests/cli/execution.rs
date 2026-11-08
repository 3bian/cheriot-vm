// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs;
use std::process::{Command, Output};

#[test]
fn input_budgets_are_enforced_before_machine_construction() {
    let directory = std::env::temp_dir().join(format!("cheriot-input-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let oversized = directory.join("oversized");
    let path = oversized.to_str().unwrap();

    for (limit, description, arguments) in [
        (
            cheriot_vm::MachineConfiguration::MAX_PROFILE_BYTES,
            "profile",
            vec![
                "--profile",
                path,
                "--raw",
                "examples/assembly/hello-uart.bin",
            ],
        ),
        (
            cheriot_vm::input::MAX_FIRMWARE_BYTES,
            "firmware",
            vec!["--profile", "profiles/rv32imc.toml", "--elf", path],
        ),
    ] {
        fs::File::create(&oversized)
            .unwrap()
            .set_len(limit as u64 + 1)
            .unwrap();
        let result = run(&arguments);
        assert!(!result.status.success());
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(
            message.contains(&format!("{description} exceeds {limit} bytes")),
            "{message}"
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn breakpoint_stops_before_side_effects_and_vector_policy_runs_handler() {
    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        "examples/assembly/hello-uart.bin",
        "--breakpoint",
        "0x80000000",
    ]);
    assert!(text.contains("Breakpoint at 0x80000000"));
    assert!(text.contains("retired=0"));
    assert!(!text.contains("UART ["));
    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        "examples/assembly/trap-return.bin",
        "--trap-policy",
        "vector",
        "--breakpoint",
        "0x80000018",
        "--steps",
        "50",
        "--registers",
    ]);
    assert!(text.contains("exception trap cause 11 at 0x80000010"));
    assert!(text.contains("x10: 0x00000001 (1)"));
    assert!(text.contains("x11: 0x0000002a (42)"));
}

#[test]
fn invalid_arguments_and_firmware_fail_cleanly() {
    for args in [
        vec!["--profile"],
        vec![
            "--profile",
            "profiles/rv32imc.toml",
            "--raw",
            "examples/assembly/compressed.bin",
            "--elf",
            "other.elf",
        ],
        vec![
            "--profile",
            "profiles/cheriot.toml",
            "--raw",
            "examples/assembly/compressed.bin",
            "--address",
            "0",
        ],
        vec![
            "--profile",
            "profiles/rv32imc.toml",
            "--raw",
            "examples/assembly/compressed.bin",
            "--address",
            "1",
        ],
        vec![
            "--profile",
            "profiles/rv32imc.toml",
            "--raw",
            "examples/assembly/compressed.bin",
            "--memory",
            "address=0,length=257",
        ],
        vec!["--steps", "18446744073709551616"],
        vec!["--steps", "-1"],
    ] {
        let result = run(&args);
        assert!(!result.status.success(), "{args:?}");
        let message = String::from_utf8(result.stderr).unwrap();
        assert!(message.contains("error:"), "{message}");
        assert!(!message.contains("panicked"));
    }
}

#[test]
fn integer_step_limits_are_optional_and_report_incomplete_execution() {
    let directory = std::env::temp_dir().join(format!("cheriot-vm-limits-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let firmware = directory.join("long-program.bin");
    let mut bytes = Vec::new();

    for _ in 0..1500 {
        bytes.extend(0x00000013_u32.to_le_bytes());
    }

    bytes.extend(0x00100073_u32.to_le_bytes());
    fs::write(&firmware, bytes).unwrap();

    for profile in ["profiles/rv32e.toml", "profiles/rv32imc.toml"] {
        let base = ["--profile", profile, "--raw", firmware.to_str().unwrap()];

        for extra in [
            vec![],
            vec!["--steps", "0"],
            vec!["--steps", "10000001"],
            vec!["--steps", "18446744073709551615"],
            vec!["--steps", "1501"],
        ] {
            let mut arguments = base.to_vec();
            arguments.extend(extra);
            let text = output(&arguments);
            assert!(text.contains("Guest EBREAK at 0x80001770"), "{text}");
            assert!(text.contains("retired=1500"), "{text}");
        }

        let mut arguments = base.to_vec();
        arguments.extend(["--steps", "1500"]);
        let result = run(&arguments);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr)
            .contains("step limit reached after 1500 steps at program counter 0x80001770"));
        assert!(String::from_utf8_lossy(&result.stdout).contains("retired=1500"));

        arguments.extend(["--breakpoint", "0x80001770"]);
        assert!(output(&arguments).contains("Breakpoint at 0x80001770"));
    }

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn integer_uart_output_is_drained_during_unlimited_execution() {
    let directory = std::env::temp_dir().join(format!("cheriot-vm-output-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let firmware = directory.join("uart-loop.bin");
    let instructions: [u32; 8] = [
        0x100000b7, 0x000101b7, 0x00118193, 0x04100113, 0x0020a023, 0xfff18193, 0xfe019ce3,
        0x00100073,
    ];
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&instructions);
    fs::write(&firmware, bytes).unwrap();

    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        firmware.to_str().unwrap(),
    ]);
    let output_bytes: usize = text
        .lines()
        .filter_map(|line| line.strip_prefix("UART [uart0]: "))
        .map(|line| {
            assert!(line.bytes().all(|byte| byte == b'A'));
            line.len()
        })
        .sum();
    assert_eq!(output_bytes, 65537);
    assert!(text.contains("Guest EBREAK at 0x8000001c"));

    fs::remove_dir_all(directory).unwrap();
}

fn output(args: &[&str]) -> String {
    let result = run(args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .args(if args.contains(&"--raw") && !args.contains(&"--address") {
            vec!["--address", "0x80000000"]
        } else {
            vec![]
        })
        .output()
        .unwrap()
}

#[test]
fn supplied_firmware_trace_registers_and_memory() {
    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        "examples/assembly/hello-uart.bin",
        "--steps",
        "10",
        "--trace",
        "--registers",
        "--memory",
        "address=0x80000000,length=4",
    ]);
    assert!(text.contains("UART [uart0]: A"));
    assert!(text.contains("Guest EBREAK at 0x8000000c"));
    assert!(text.contains("x02: 0x00000041 (65)"));
    assert!(text.contains("0x80000000: b7 00 00 10"));
    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        "examples/assembly/arithmetic.bin",
        "--steps",
        "10",
        "--registers",
    ]);
    assert!(text.contains("x03: 0x0000001e (30)"));
    assert!(text.contains("x04: 0x00000258 (600)"));
    let text = output(&[
        "--profile",
        "profiles/rv32imc.toml",
        "--raw",
        "examples/assembly/compressed.bin",
        "--steps",
        "10",
        "--trace",
        "--registers",
    ]);
    assert!(text.contains("x01: 0x00000002 (2)"));
    assert!(text.contains("0x80000002 [0085/2]"));
}

#[test]
fn named_register_lists_show_only_available_selected_registers() {
    for (profile, selection, present, absent, rejected) in [
        (
            "rv32e",
            "x0+x15+pc",
            vec!["x00:", "x15:"],
            vec!["x01:", "x16:"],
            "x16",
        ),
        (
            "rv32imc",
            "x1+x31",
            vec!["x01:", "x31:"],
            vec!["x00:", "x02:"],
            "c1",
        ),
        (
            "cheriot",
            "c0+c15+pcc",
            vec!["c00:", "c15:", "PCC:"],
            vec!["c01:", "MTCC:"],
            "x1",
        ),
    ] {
        let base = [
            "--profile",
            &format!("profiles/{profile}.toml"),
            "--raw",
            "examples/assembly/hello-uart.bin",
            "--breakpoint",
            "0x80000000",
            "--registers",
        ];
        let mut args = base.to_vec();
        let names = format!("names={selection}");
        args.push(&names);
        let text = output(&args);
        for expected in present {
            assert!(text.contains(expected), "{text}");
        }
        for excluded in absent {
            assert!(!text.contains(excluded), "{text}");
        }
        let mut args = base.to_vec();
        let names = format!("names={rejected}");
        args.push(&names);
        let result = run(&args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("unavailable for this processor"));
    }
}
