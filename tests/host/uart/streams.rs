// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(target_os = "linux")]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "../../firmware/uart_htif.rs"]
mod uart_htif;

struct Guest {
    directory: PathBuf,
}

impl Guest {
    fn new(profile: &str, second_serial: bool, echo: bool) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "vm-serial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let mut configuration = fs::read_to_string(format!("profiles/{profile}.toml")).unwrap();
        if second_serial {
            configuration.push_str("\n[[uart]]\nname = 'serial1'\nbase = 0x10004000\n");
            if profile == "cheriot" {
                configuration.push_str("layout = '16550'\n");
            }
        }
        fs::write(directory.join("profile.toml"), configuration).unwrap();
        let mut words = Vec::new();
        for base in [0x10000000_u32, 0x10004000]
            .into_iter()
            .take(if second_serial { 2 } else { 1 })
        {
            if profile == "cheriot" {
                words.extend([0x03d000db, base | 0x137, 0x202080db]);
            } else {
                words.push(base | 0xb7);
            }
            words.push(if echo { 0x0000a183 } else { 0x04100193 });
            words.push(0x0030a023);
        }
        words.push(0x00100073);
        fs::write(
            directory.join("guest.bin"),
            crate::firmware_words::firmware_from_words(&words),
        )
        .unwrap();
        Self { directory }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"));
        command.args([
            "--profile",
            self.directory.join("profile.toml").to_str().unwrap(),
            "--raw",
            self.directory.join("guest.bin").to_str().unwrap(),
            "--address",
            "0x80000000",
            "--trap-policy",
            "stop",
            "--steps",
            "100",
        ]);
        command
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_htif_routing(device: &str) {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let guest = Guest::new(profile, false, false);
        let configuration = guest.directory.join("profile.toml");
        fs::write(
            &configuration,
            fs::read_to_string(&configuration)
                .unwrap()
                .replace("name = \"uart0\"", &format!("name = \"{device}\"")),
        )
        .unwrap();
        fs::write(
            guest.directory.join("guest.bin"),
            uart_htif::program(profile == "cheriot", b'H', false),
        )
        .unwrap();
        let path = guest.directory.join("uart.out");
        let output = guest
            .command()
            .args([
                "--completion",
                "0x80000100",
                "--uart",
                &format!("device={device},out={}", path.display()),
                "--output",
                "raw",
            ])
            .output()
            .unwrap();
        success(&output);
        assert_eq!(
            fs::read(path).unwrap(),
            b"U",
            "{profile}/{device}: stdout={:?}",
            output.stdout
        );
        assert_eq!(output.stdout, b"H", "{profile}/{device}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("EBREAK"));
    }
}

#[test]
fn automatic_selection_and_explicit_output_paths_preserve_guest_bytes_and_diagnostics() {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let guest = Guest::new(profile, false, false);
        let path = guest.directory.join("serial.out");
        let result = guest
            .command()
            .args(["--uart", &format!("out={}", path.display())])
            .output()
            .unwrap();
        success(&result);
        assert_eq!(fs::read(&path).unwrap(), b"A");
        assert!(String::from_utf8_lossy(&result.stdout).contains("Guest EBREAK"));
        assert!(!result.stdout.starts_with(b"A"));
        let expected = guest.command().args(["--output", "raw"]).output().unwrap();
        let actual = guest
            .command()
            .args(["--uart", "out=/dev/stdout", "--output", "raw"])
            .output()
            .unwrap();
        success(&expected);
        success(&actual);
        assert_eq!(actual.stdout, b"A");
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

#[test]
fn distinct_uart_name_routes_htif_to_stdout() {
    assert_htif_routing("uart0");
}

#[test]
fn input_is_disconnected_unless_requested_and_default_output_is_stdout() {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let guest = Guest::new(profile, false, true);
        let input = guest.directory.join("unconnected-input");
        fs::write(&input, b"Z").unwrap();
        let output = guest
            .command()
            .args(["--output", "raw"])
            .stdin(fs::File::open(input).unwrap())
            .output()
            .unwrap();
        success(&output);
        assert_eq!(output.stdout, [0]);
    }
}

#[test]
fn invalid_and_duplicate_bindings_do_not_truncate_output_files() {
    for second in [false, true] {
        let guest = Guest::new("rv32imc", second, false);
        let path = guest.directory.join("preserved");
        fs::write(&path, b"preserve me").unwrap();
        let output = guest
            .command()
            .args([
                "--uart",
                &format!("out={}", path.display()),
                "--uart",
                "device=gpio0,out=/dev/null",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(fs::read(&path).unwrap(), b"preserve me");
        if second {
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains("uart0, serial1"), "{error}");
        }
        let duplicate = guest
            .command()
            .args([
                "--uart",
                &format!("device=uart0,out={}", path.display()),
                "--uart",
                "device=uart0,in=/dev/null",
            ])
            .output()
            .unwrap();
        assert!(!duplicate.status.success());
        assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate UART binding"));
        assert_eq!(fs::read(&path).unwrap(), b"preserve me");
    }
}

#[test]
fn aliased_uart_output_files_are_rejected_before_truncation() {
    use std::os::unix::fs::symlink;

    let guest = Guest::new("rv32imc", true, false);
    let path = guest.directory.join("preserved");
    let symlink_path = guest.directory.join("symlink");
    let hardlink_path = guest.directory.join("hardlink");
    fs::write(&path, b"preserve me").unwrap();
    symlink(&path, &symlink_path).unwrap();
    fs::hard_link(&path, &hardlink_path).unwrap();

    for alias in [
        format!("{}/./preserved", guest.directory.display()),
        symlink_path.display().to_string(),
        hardlink_path.display().to_string(),
    ] {
        let output = guest
            .command()
            .args([
                "--uart",
                &format!("device=uart0,out={}", path.display()),
                "--uart",
                &format!("device=serial1,out={alias}"),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success(), "alias {alias}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("only be bound once"), "{alias}: {error}");
        assert_eq!(fs::read(&path).unwrap(), b"preserve me", "{alias}");
    }
}

#[test]
fn unavailable_later_uart_output_does_not_truncate_an_earlier_file() {
    let guest = Guest::new("rv32imc", true, false);
    let path = guest.directory.join("preserved");
    fs::write(&path, b"preserve me").unwrap();
    let output = guest
        .command()
        .args([
            "--uart",
            &format!("device=uart0,out={}", path.display()),
            "--uart",
            &format!(
                "device=serial1,out={}",
                guest.directory.join("missing").join("output").display()
            ),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(path).unwrap(), b"preserve me");
}

#[test]
fn shared_stdout_aliases_remain_available_to_multiple_uarts() {
    let guest = Guest::new("rv32imc", true, false);
    let output = guest
        .command()
        .args([
            "--uart",
            "device=uart0,out=/dev/stdout",
            "--uart",
            "device=serial1,out=/dev/fd/1",
            "--output",
            "raw",
        ])
        .output()
        .unwrap();
    success(&output);
    assert_eq!(output.stdout, b"AA");
}

#[test]
fn dedicated_uart_output_cannot_truncate_the_redirected_stdout_file() {
    let guest = Guest::new("rv32imc", true, false);
    let path = guest.directory.join("stdout");
    fs::write(&path, b"preserve me").unwrap();
    let stdout = fs::OpenOptions::new().append(true).open(&path).unwrap();
    let output = guest
        .command()
        .args([
            "--uart",
            "device=uart0,out=/dev/stdout",
            "--uart",
            &format!("device=serial1,out={}", path.display()),
            "--output",
            "raw",
        ])
        .stdout(stdout)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("only be bound once"));
    assert_eq!(fs::read(path).unwrap(), b"preserve me");
}

#[test]
fn multiple_serial_devices_route_each_input_and_output_by_name_in_any_order() {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let guest = Guest::new(profile, true, true);
        let first_in = guest.directory.join("first.in");
        let first_out = guest.directory.join("first.out");
        let second_in = guest.directory.join("second.in");
        let second_out = guest.directory.join("second.out");
        fs::write(&first_in, b"X").unwrap();
        fs::write(&second_in, b"Y").unwrap();
        let output = guest
            .command()
            .args([
                "--uart",
                &format!(
                    "out={},device=serial1,in={}",
                    second_out.display(),
                    second_in.display()
                ),
                "--uart",
                &format!(
                    "in={},out={},device={}",
                    first_in.display(),
                    first_out.display(),
                    "uart0"
                ),
                "--output",
                "raw",
            ])
            .output()
            .unwrap();
        success(&output);
        assert_eq!(fs::read(first_out).unwrap(), b"X");
        assert_eq!(fs::read(second_out).unwrap(), b"Y");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Guest EBREAK"));
    }
}

#[test]
fn partial_routing_keeps_unbound_channels_on_stdout() {
    let guest = Guest::new("rv32imc", true, false);
    let path = guest.directory.join("first.out");
    let output = guest
        .command()
        .args([
            "--uart",
            &format!("device=uart0,out={}", path.display()),
            "--output",
            "raw",
        ])
        .output()
        .unwrap();
    success(&output);
    assert_eq!(fs::read(path).unwrap(), b"A");
    assert_eq!(output.stdout, b"A");
}

#[test]
fn uart_named_tohost_still_routes_htif_to_stdout() {
    assert_htif_routing("tohost");
}

#[test]
fn failing_uart_output_is_a_host_failure_and_stdout_binding_does_not_truncate() {
    let guest = Guest::new("rv32imc", false, false);
    let output = guest
        .command()
        .args(["--uart", "out=/dev/full"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No space left"));
    let path = guest.directory.join("stdout");
    fs::write(&path, b"prefix:").unwrap();
    let file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    let output = guest
        .command()
        .args(["--uart", "out=/dev/stdout", "--output", "raw"])
        .stdout(file)
        .output()
        .unwrap();
    success(&output);
    assert_eq!(fs::read(path).unwrap(), b"prefix:A");
}
