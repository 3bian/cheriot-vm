// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Tool;
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub(super) fn probe(tools: &[Tool]) -> (Option<bool>, Option<String>) {
    let Some(compiler) = tools.first().and_then(|tool| tool.path.as_ref()) else {
        return (Some(false), Some("CHERIoT clang was not found".into()));
    };
    let result = (|| -> std::io::Result<std::process::Output> {
        let mut child = Command::new(compiler)
            .args([
                "-target",
                "riscv32-unknown-cheriotrtos",
                "-mcpu=cheriot-ibex",
                "-mabi=cheriot",
                "-mxcheri-rvc",
                "-x",
                "c",
                "-c",
                "-o",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(b"int cheriot_probe(void) { return 0; }\n")?;
        }
        child.wait_with_output()
    })();
    match result {
        Ok(output) => (
            Some(output.status.success()),
            if output.status.success() {
                None
            } else {
                Some(String::from_utf8_lossy(&output.stderr).trim().to_string())
            },
        ),
        Err(error) => (Some(false), Some(error.to_string())),
    }
}
