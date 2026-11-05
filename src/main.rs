// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![forbid(unsafe_code)]

use std::io::Write;
use std::process::ExitCode;

mod cli;

fn main() -> ExitCode {
    match cli::execute_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            #[cfg(all(feature = "gdb", target_os = "linux"))]
            if std::env::args().any(|argument| argument == "--gdb") {
                if let Ok(mut diagnostics) =
                    cheriot_vm::host::debugger::output::DebugOutput::new(std::io::stderr())
                {
                    let _ = diagnostics.enqueue(format!("error: {error}\n").as_bytes(), true);
                    let _ = diagnostics.pump();
                }
                return ExitCode::FAILURE;
            }
            let _ = writeln!(std::io::stderr(), "error: {error}");
            ExitCode::FAILURE
        }
    }
}
