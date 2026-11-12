// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![forbid(unsafe_code)]
mod app_error;
mod build;
mod cli;
mod config;
mod debug;
mod diagnose;
mod dispatch;
mod exec;
mod file_lock;
mod flash;
mod global;
mod help;
mod init;
mod output;
mod resources;
mod target;
mod test;
mod toolchain;

use clap::{CommandFactory, FromArgMatches};
use std::process::ExitCode;

fn main() -> ExitCode {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if message.contains("failed printing to stdout") && message.contains("Broken pipe") {
            std::process::exit(141);
        }
        default_hook(info);
    }));
    let command = help::prepare(cli::Cli::command(), "cheriot");
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(error) => return usage_error(error),
    };
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return usage_error(error),
    };
    match dispatch::dispatch(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            output::error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

fn usage_error(error: clap::Error) -> ExitCode {
    use clap::error::ErrorKind;
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        error.exit();
    }
    output::set_colour(None);
    let rendered = error.render().to_string();
    let body: Vec<&str> = rendered
        .lines()
        .filter(|line| !line.starts_with("Usage: ") && !line.starts_with("For more information"))
        .map(|line| line.strip_prefix("error: ").unwrap_or(line))
        .collect();
    let mut message = body.join("\n").trim_end().to_owned();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut command = vec!["cheriot".to_owned()];
    let mut current = cli::Cli::command();
    for argument in &arguments {
        match current.find_subcommand(argument).cloned() {
            Some(next) => {
                command.push(argument.clone());
                current = next;
            }
            None if argument.starts_with('-') => continue,
            None => break,
        }
    }
    if let Some(moved) = arguments
        .windows(2)
        .find(|pair| {
            pair[0] == "build"
                && ["clean", "inspect", "package", "plan", "size"].contains(&pair[1].as_str())
        })
        .map(|pair| pair[1].clone())
    {
        message.push_str(&format!(
            "\n\n`cheriot build {moved}` is now `cheriot {moved}`."
        ));
    }
    message.push_str(&format!(
        "\n\nRun `{} --help` for the options.",
        command.join(" ")
    ));
    output::error(&message);
    ExitCode::from(2)
}
