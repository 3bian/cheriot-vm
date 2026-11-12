// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{
    command_name::CommandName,
    options::{Options, absolute_path, selected_value},
    request::{Action, Format, Request},
};
use crate::config::configuration::ProjectSettings;
use crate::config::diagnostic::{Diagnostic, Result};
use std::path::{Path, PathBuf};
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Diagnostic::new("paths must be UTF-8"))
}
fn option_text(path: Option<PathBuf>) -> Result<Option<String>> {
    path.as_deref().map(text).transpose()
}
impl Options {
    pub(super) fn from_request(request: Request) -> Result<Self> {
        let Request {
            action,
            selection,
            format,
            output,
            compartment,
            dry_run,
            verbose,
            quiet,
            probe_toolchain,
            include_packaging,
        } = request;
        let command_name = match action {
            Action::Build => CommandName::Build,
            Action::Plan => CommandName::Plan,
            Action::Clean => CommandName::Clean,
            Action::Size => CommandName::Size,
            Action::Configuration => CommandName::Configuration,
        };
        let output_format = format;
        let valid_format = match command_name {
            CommandName::Configuration => {
                matches!(format, Format::Text | Format::Toml | Format::Json)
            }
            CommandName::Plan => matches!(
                format,
                Format::Text
                    | Format::Json
                    | Format::Toml
                    | Format::Shell
                    | Format::CompilationDatabase
            ),
            CommandName::Build => {
                matches!(format, Format::Human | Format::Json | Format::Toml)
            }
            CommandName::Size => matches!(
                format,
                Format::Text | Format::Json | Format::Toml | Format::Bytes
            ),
            _ => format == Format::Human,
        };
        if !valid_format {
            return Err(Diagnostic::new(format!(
                "invalid output format for {}",
                command_name.name()
            )));
        }
        for (option, value) in [
            ("--config", selection.file.as_deref()),
            ("--build-dir", selection.build_dir.as_deref()),
            ("--output-dir", selection.output_dir.as_deref()),
            ("--toolchain-dir", selection.toolchain_dir.as_deref()),
            ("--output", output.as_deref()),
        ] {
            if value.is_some_and(|path| path.as_os_str().is_empty()) {
                return Err(Diagnostic::new(format!("{option} must not be empty")));
            }
        }
        for (option, value) in [
            ("--board", selection.board.as_deref()),
            ("--project", selection.project.as_deref()),
            ("--firmware", selection.firmware.as_deref()),
            ("--compartment", compartment.as_deref()),
        ] {
            if value == Some("") {
                return Err(Diagnostic::new(format!("{option} must not be empty")));
            }
        }
        if selection
            .overlays
            .iter()
            .any(|path| path.as_os_str().is_empty())
        {
            return Err(Diagnostic::new("--overlay must not be empty"));
        }
        if verbose && quiet {
            return Err(Diagnostic::new("--verbose and --quiet cannot be combined"));
        }
        if command_name == CommandName::Build && output_format == Format::Json && (verbose || quiet)
        {
            return Err(Diagnostic::new(
                "--verbose and --quiet cannot be combined with --format json",
            ));
        }
        if output.is_some() && command_name == CommandName::Build && output_format != Format::Json {
            return Err(Diagnostic::new(
                "--output requires --format json for this command",
            ));
        }
        let configuration_file = selection
            .file
            .unwrap_or_else(|| PathBuf::from("cheriot.toml"));
        let board = selected_value(selection.board, "CHERIOT_BOARD", "--board")?
            .map(|(value, origin)| {
                let value = if value.contains('/') || value.contains('\\') {
                    absolute_path(&value)?.display().to_string()
                } else {
                    value
                };
                Ok::<_, Diagnostic>((value, origin))
            })
            .transpose()?;
        let mode = selected_value(selection.mode, "CHERIOT_MODE", "--mode")?;
        if mode
            .as_ref()
            .is_some_and(|(value, _)| !["debug", "release"].contains(&value.as_str()))
        {
            return Err(Diagnostic::new("build mode must be debug or release"));
        }
        let toolchain_directory = selected_value(
            option_text(selection.toolchain_dir)?,
            "CHERIOT_TOOLCHAIN_DIR",
            "--toolchain-dir",
        )?
        .map(|(v, o)| Ok::<_, Diagnostic>((absolute_path(&v)?, o)))
        .transpose()?;
        let build_directory = selected_value(
            option_text(selection.build_dir)?,
            "CHERIOT_BUILD_DIR",
            "--build-dir",
        )?
        .map(|(v, o)| Ok::<_, Diagnostic>((absolute_path(&v)?, o)))
        .transpose()?;
        let output_directory = selected_value(
            option_text(selection.output_dir)?,
            "CHERIOT_OUTPUT_DIR",
            "--output-dir",
        )?
        .map(|(v, o)| Ok::<_, Diagnostic>((absolute_path(&v)?, o)))
        .transpose()?;
        // The option, the environment, then the user's global setting.
        let jobs = if accepts_jobs(command_name) {
            let chosen = selected_value(
                selection.jobs.map(|v| v.to_string()),
                "CHERIOT_JOBS",
                "--jobs",
            )?
            .map(|(value, _)| value);
            let chosen = match chosen {
                Some(value) => Some(value),
                None => crate::global::value("jobs").map_err(Diagnostic::new)?,
            };
            chosen
                .map(|value| {
                    value
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (1..=256).contains(n))
                        .ok_or_else(|| {
                            Diagnostic::new("jobs must be a whole number between 1 and 256")
                        })
                })
                .transpose()?
                .unwrap_or(1)
        } else {
            1
        };
        let overlays = selection
            .overlays
            .into_iter()
            .map(|p| absolute_path(&text(&p)?))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            command_name,
            configuration_file,
            compartment_name: compartment,
            settings: ProjectSettings {
                overlays,
                board,
                mode,
                toolchain_directory,
                build_directory,
                output_directory,
            },
            dry_run,
            firmware_name: selection.firmware,
            include_packaging,
            jobs,
            output_file: output,
            output_format,
            probe_toolchain,
            project_name: selection.project,
            verbose,
            quiet,
        })
    }
}

fn accepts_jobs(command: CommandName) -> bool {
    matches!(command, CommandName::Build | CommandName::Size)
}
