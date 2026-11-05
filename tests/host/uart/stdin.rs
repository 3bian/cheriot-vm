// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn encode_branch(first: u32, second: u32, offset: i32, not_equal: bool) -> u32 {
    let bits = offset as u32;
    ((bits >> 12 & 1) << 31)
        | ((bits >> 5 & 63) << 25)
        | (second << 20)
        | (first << 15)
        | (u32::from(not_equal) << 12)
        | ((bits >> 1 & 15) << 8)
        | ((bits >> 11 & 1) << 7)
        | 0x63
}

fn echo_program(profile: &str, length: usize) -> Vec<u8> {
    let mut words = if profile == "cheriot1" {
        vec![0x03d0_00db, 0x1000_0137, 0x2020_80db]
    } else {
        vec![0x1000_00b7]
    };
    words.extend([
        0x0000_0193,
        (((length as u32 + 0x800) >> 12) << 12) | 0x2b7,
        (((length as u32) & 0xfff) << 20) | 0x28293,
    ]);
    let status = if profile == "cheriot1" { 0x14 } else { 4 };
    words.extend([
        (status << 20) | 0xa103,
        0x0011_7113,
        encode_branch(2, 0, -8, false),
        0x0000_a103,
        0x0020_a023,
        0x0011_8193,
        encode_branch(3, 5, -24, true),
        0x0010_0073,
    ]);
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}

fn run_guest(
    profile: &str,
    payload: Option<Vec<u8>>,
    keep_open: bool,
    steps: &str,
) -> (bool, Vec<u8>, String) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "vm-stdin-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let firmware = directory.join("echo.bin");
    let length = payload.as_ref().map_or(1, Vec::len);
    fs::write(&firmware, echo_program(profile, length)).unwrap();
    let stdout = directory.join("stdout");
    let stderr = directory.join("stderr");
    let mut child = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .args([
            "--profile",
            &format!("profiles/{profile}.toml"),
            "--raw",
            firmware.to_str().unwrap(),
            "--address",
            "0x80000000",
            "--uart",
            if profile == "cheriot1" {
                "device=uart0,in=/dev/stdin"
            } else {
                "in=/dev/stdin"
            },
            "--trap-policy",
            "stop",
            "--output",
            "raw",
            "--steps",
            steps,
        ])
        .stdin(Stdio::piped())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take();
    let writer = payload.map(|bytes| {
        let mut input = input.take().unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            input.write_all(&bytes).unwrap();
        })
    });
    if !keep_open {
        drop(input.take());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("stdin blocked guest execution");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if let Some(writer) = writer {
        writer.join().unwrap();
    }
    let output = fs::read(stdout).unwrap();
    let errors = fs::read_to_string(stderr).unwrap();
    fs::remove_dir_all(directory).unwrap();
    (status.success(), output, errors)
}

#[test]
fn delayed_stdin_streams_more_than_a_fifo_without_losing_bytes() {
    for profile in ["rv32e", "rv32imc", "cheriot1"] {
        let payload: Vec<_> = (0..8193).map(|n| (n % 256) as u8).collect();
        let (success, output, errors) = run_guest(profile, Some(payload.clone()), false, "0");
        assert!(success, "{profile}: {errors}");
        assert_eq!(output, payload, "{profile}");
    }
}

#[test]
fn open_idle_stdin_and_eof_do_not_block_step_budgets() {
    for keep_open in [false, true] {
        let (success, output, errors) = run_guest("rv32imc", None, keep_open, "10000");
        assert!(!success);
        assert!(output.is_empty());
        assert!(
            errors.contains("step limit reached after 10000 steps"),
            "{errors}"
        );
    }
}

#[test]
fn regular_file_input_streams_more_than_a_fifo() {
    let directory = std::env::temp_dir().join(format!("vm-input-file-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("input");
    let firmware = directory.join("echo.bin");
    let payload: Vec<_> = (0..8193).map(|index| (index % 256) as u8).collect();
    fs::write(&input, &payload).unwrap();
    for profile in ["cheriot1", "rv32e", "rv32imc"] {
        fs::write(&firmware, echo_program(profile, payload.len())).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .args([
                "--profile",
                &format!("profiles/{profile}.toml"),
                "--raw",
                firmware.to_str().unwrap(),
                "--address",
                "0x80000000",
                "--uart",
                &format!("in={}", input.display()),
                "--output",
                "raw",
                "--trap-policy",
                "stop",
                "--steps",
                "200000",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{profile}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, payload, "{profile}");
    }
    fs::remove_dir_all(directory).unwrap();
}
