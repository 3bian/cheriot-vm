// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn run_runs_existing_rv32_firmware_and_reports_step_limit() {
    let profile = "tests/fixtures/rv32imc.toml";
    let raw = "tests/fixtures/arithmetic.bin";
    let result = run(&[
        "exec",
        "--profile",
        profile,
        "--raw",
        raw,
        "--address",
        "0x80000000",
        "--registers",
        "names=x3+x4",
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let output = stdout(&result);
    assert!(output.contains("x03: 0x0000001e"), "{output}");
    assert!(output.contains("x04: 0x00000258"), "{output}");
    let limited = run(&[
        "exec",
        "--profile",
        profile,
        "--raw",
        raw,
        "--address",
        "0x80000000",
        "--steps",
        "1",
    ]);
    assert!(!limited.status.success());
    assert!(stderr(&limited).contains("step limit reached"));
}

#[test]
fn run_runs_cheriot_elf_to_guest_completion() {
    let result = run(&[
        "exec",
        "tests/fixtures/hello_world.elf",
        "--profile",
        "profiles/cheriot.toml",
        "--steps",
        "3000000",
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(stdout(&result).contains("Hello world"));
    assert!(stderr(&result).contains("Guest exit status: 0"));
}

#[test]
fn multiple_breakpoints_stop_before_the_first_matching_instruction() {
    let out = run(&[
        "exec",
        "--profile",
        "tests/fixtures/rv32imc.toml",
        "--raw",
        "tests/fixtures/arithmetic.bin",
        "--address",
        "0x80000000",
        "--breakpoint",
        "0x8000000c",
        "--breakpoint",
        "0x80000004",
        "--registers",
        "names=x1+x2",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let output = stdout(&out);
    assert!(output.contains("Breakpoint at 0x80000004"), "{output}");
    assert!(output.contains("x01: 0x0000000a"), "{output}");
    assert!(output.contains("x02: 0x00000000"), "{output}");
}

#[test]
fn invalid_addresses_and_duplicate_singleton_options_are_rejected() {
    let overflow = run(&[
        "exec",
        "--profile",
        "profile",
        "--raw",
        "firmware",
        "--address",
        "4294967296",
    ]);
    assert!(!overflow.status.success());
    assert!(stderr(&overflow).contains("raw address exceeds 32 bits"));
    let duplicate = run(&[
        "exec",
        "--profile",
        "a",
        "--profile",
        "b",
        "--raw",
        "firmware",
        "--address",
        "0",
    ]);
    assert!(!duplicate.status.success());
    assert!(stderr(&duplicate).contains("cannot be used multiple times"));
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_retain_the_application_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let path = OsString::from_vec(vec![0xff]);
    let build = cheriot()
        .args(["build", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    let execute = cheriot()
        .args(["exec", "--profile", "tests/fixtures/rv32imc.toml", "--raw"])
        .arg(&path)
        .args(["--address", "0"])
        .output()
        .unwrap();
    let debug = cheriot()
        .arg("debug")
        .arg(&path)
        .args(["--profile", "profile"])
        .output()
        .unwrap();
    for result in [build, execute, debug] {
        assert!(!result.status.success());
        assert!(stderr(&result).starts_with("cheriot: path is not UTF-8:"));
    }
}

#[test]
fn stats_report_instructions_coverage_and_stacks() {
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("stats.json");
    let out = run(&[
        "exec",
        "tests/fixtures/hello_world.elf",
        "--profile",
        "profiles/cheriot.toml",
        "--stats-output",
        report.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let stats: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    let total = stats["instructions"].as_u64().unwrap();
    assert!(total > 0);
    let attributed: u64 = stats["compartments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["instructions"].as_u64().unwrap())
        .sum();
    assert_eq!(attributed + stats["outside_code"].as_u64().unwrap(), total);
    for compartment in stats["compartments"].as_array().unwrap() {
        assert!(compartment["covered_bytes"].as_u64() <= compartment["size"].as_u64());
    }
    assert!(!stats["functions"].as_array().unwrap().is_empty());
    assert!(
        stats["stacks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "thread_stack_1")
    );

    let plain = run(&[
        "exec",
        "tests/fixtures/hello_world.elf",
        "--profile",
        "profiles/cheriot.toml",
    ]);
    let with_stats = run(&[
        "exec",
        "tests/fixtures/hello_world.elf",
        "--profile",
        "profiles/cheriot.toml",
        "--stats",
    ]);
    let text = stdout(&with_stats);
    assert!(text.starts_with(&stdout(&plain)), "{text}");
    for section in [
        "Execution\n",
        "Compartments and libraries\n",
        "Stack high-water marks\n",
    ] {
        assert!(text.contains(section), "{text}");
    }
    let raw = run(&[
        "exec",
        "--raw",
        "tests/fixtures/arithmetic.bin",
        "--address",
        "0x80000000",
        "--profile",
        "tests/fixtures/rv32imc.toml",
        "--stats",
    ]);
    assert!(!raw.status.success());
    assert!(stderr(&raw).contains("--stats needs an ELF"));
}

#[test]
fn a_target_file_runs_an_elf_outside_any_project() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("echo.toml");
    std::fs::write(
        &target,
        "[exec]\ncommand = [\"sh\", \"-c\", \"echo ran $0\", \"${firmware_name}\"]\n",
    )
    .unwrap();
    let elf = std::fs::canonicalize("tests/fixtures/hello_world.elf").unwrap();
    let out = cheriot()
        .current_dir(directory.path())
        .arg("exec")
        .arg(&elf)
        .args(["--target", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ran hello_world");
    let vm = cheriot()
        .current_dir(directory.path())
        .arg("exec")
        .arg(&elf)
        .args(["--target", "vm"])
        .output()
        .unwrap();
    assert!(vm.status.success(), "{}", stderr(&vm));
    assert!(stdout(&vm).contains("Hello world"));
}
