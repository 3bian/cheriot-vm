// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::process::Command;

#[test]
fn debugger_options_reject_unsupported_combinations_before_resources_open() {
    for arguments in [
        vec!["--gdb", "port=65536"],
        vec!["--gdb", "ip=0.0.0.0"],
        vec!["--gdb", "ip=192.0.2.1"],
        vec!["--gdb", "ip=::"],
        vec!["--gdb", "port=333,port=334"],
        vec!["--gdb", "ip=invalid"],
        vec!["--gdb", "--trace"],
        vec!["--gdb", "--breakpoint", "0"],
        vec!["--gdb", "--trap-policy", "stop"],
        vec!["--gdb", "--tap", "interface=no-interface"],
        vec!["--gdb", "--gpio", "path=/no-chip,lines=1"],
        vec!["--gdb", "--spi", "path=/no-spi"],
        vec!["--gdb", "--i2c", "path=/no-i2c"],
        vec!["--gdb-raw-output"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .args(["--profile", "absent-profile", "--elf", "absent-firmware"])
            .args(arguments)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("--gdb"), "{error}");
        assert!(!error.contains("No such file"), "{error}");
        assert!(!error.contains("CHERIOT_GDB_READY"));
    }
}

#[cfg(not(all(feature = "gdb", target_os = "linux")))]
#[test]
fn debugger_build_feature_is_explicit() {
    let result = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
        .args([
            "--profile",
            "profiles/rv32imc.toml",
            "--raw",
            "examples/assembly/hello-uart.bin",
            "--gdb",
            "port=0",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--features gdb"));
}
