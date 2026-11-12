// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::{
    io::Write,
    path::Path,
    process::{Child, Command, Output, Stdio},
    thread,
    time::Duration,
};
#[path = "support/test_fixture.rs"]
mod test_fixture;

fn cheriot() -> Command {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_cheriot"));
    command
        .env(
            "CHERIOT_GLOBAL_CONFIG",
            std::env::temp_dir().join("cheriot-tests-no-global-config.toml"),
        )
        .env("CHERIOT_TARGETS_DIR", source.join("targets"))
        .env("CHERIOT_TEMPLATE", source.join("template"));
    command
}
fn run(args: &[&str]) -> Output {
    cheriot().args(args).output().unwrap()
}
fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).replace("\n  ", " ")
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[path = "build/cli.rs"]
mod build;
#[path = "config/cli.rs"]
mod config;
#[path = "debug/cli.rs"]
mod debug;
#[path = "exec/cli.rs"]
mod exec_command;
#[path = "cli/help.rs"]
mod help;
#[path = "toolchain/cli.rs"]
mod toolchain;
