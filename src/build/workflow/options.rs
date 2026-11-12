// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};

use super::{command_name::CommandName, request::Format as OutputFormat};
use crate::config::configuration::ProjectSettings;
use crate::config::diagnostic::{Diagnostic, Origin, Result};

pub(super) struct Options {
    pub(super) command_name: CommandName,
    pub(super) configuration_file: PathBuf,
    pub(super) compartment_name: Option<String>,
    pub(super) settings: ProjectSettings,
    pub(super) dry_run: bool,
    pub(super) firmware_name: Option<String>,
    pub(super) include_packaging: bool,
    pub(super) jobs: usize,
    pub(super) output_file: Option<PathBuf>,
    pub(super) output_format: OutputFormat,
    pub(super) probe_toolchain: bool,
    pub(super) project_name: Option<String>,
    pub(super) verbose: bool,
    pub(super) quiet: bool,
}

pub(super) fn environment_setting(name: &str) -> Result<Option<String>> {
    crate::config::configuration::read_environment(name)
}

pub(super) fn selected_value(
    option: Option<String>,
    variable: &str,
    option_name: &str,
) -> Result<Option<(String, Origin)>> {
    let (value, origin) = match option {
        Some(value) => (value, Origin::command_line(option_name)),
        None => match environment_setting(variable)? {
            Some(value) => (value, Origin::environment(variable)),
            None => return Ok(None),
        },
    };
    if value.is_empty() {
        return Err(Diagnostic::new(format!(
            "{} must not be empty",
            origin.rule
        )));
    }
    Ok(Some((value, origin)))
}

pub(super) fn absolute_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(crate::config::configuration::resolution::normalize_path(
            path,
        ))
    } else {
        let current_directory = std::env::current_dir()
            .map_err(|error| Diagnostic::new(format!("cannot read current directory: {error}")))?;
        Ok(crate::config::configuration::resolution::normalize_path(
            &current_directory.join(path),
        ))
    }
}
