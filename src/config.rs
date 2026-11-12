// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod check;
pub(crate) mod configuration;
pub(crate) mod diagnostic;
mod get;
mod inspect;
mod ls;
pub(crate) mod metadata;
pub(crate) mod presentation;
pub(crate) mod profiles;
pub(crate) mod project;
mod reset;
mod sbom;
mod sbom_export;
mod set;
pub(crate) mod transaction;

fn selected(global: Option<&Path>, project: Option<String>) -> Result<PathBuf> {
    let file = global
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("cheriot.toml"));
    let file = crate::config::configuration::select_project(&file, project.as_deref())
        .map_err(|e| AppError::Message(e.to_string()))?;
    let name = file
        .file_name()
        .ok_or_else(|| AppError::Message("project file has no file name".into()))?;
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(fs::canonicalize(parent)?.join(name))
}
fn read_project(file: &Path) -> Result<crate::config::configuration::ResolvedProject> {
    crate::config::project::load_project(file).map_err(|e| AppError::Message(e.to_string()))
}

pub(crate) struct Checked {
    pub names: Vec<String>,
    pub errors: Vec<String>,
    pub project: Option<crate::config::configuration::ResolvedProject>,
}

pub(crate) fn validate(
    config: Option<&Path>,
    project: Option<&str>,
    toolchain_dir: Option<&Path>,
) -> Checked {
    use crate::build::{selection, workflow, workflow::request::Action};
    let file = config.unwrap_or(Path::new("cheriot.toml"));
    let mut checked = Checked {
        names: Vec::new(),
        errors: Vec::new(),
        project: None,
    };
    if !file.exists() {
        let missing = crate::config::configuration::missing_project(file);
        checked.errors.push(missing.to_string());
        return checked;
    }
    match crate::config::configuration::read_workspace(file) {
        Ok(Some(workspace)) if project.is_none() => {
            for (name, path) in workspace.projects {
                match read_project(&path) {
                    Ok(_) => checked.names.push(name),
                    Err(e) => checked
                        .errors
                        .push(format!("workspace project {name}: {e}")),
                }
            }
        }
        Ok(_) => {
            let choice = crate::cli::Selection {
                project: project.map(str::to_owned),
                toolchain_dir: toolchain_dir.map(Path::to_path_buf),
                ..Default::default()
            };
            let resolved = selection::request(&choice, config, Action::Build).and_then(|request| {
                workflow::resolved_project(request).map_err(|e| AppError::Message(e.to_string()))
            });
            match resolved {
                Ok(resolved) => {
                    checked.names.push(resolved.name.clone());
                    checked.project = Some(resolved);
                }
                Err(e) => checked.errors.push(e.to_string()),
            }
        }
        Err(e) => checked.errors.push(e.to_string()),
    }
    checked
}

pub(crate) fn handle(c: crate::cli::ConfigArgs, config: Option<&Path>) -> Result<u8> {
    use crate::cli::ConfigCommand;
    if c.global {
        return match c.command {
            Some(command) => crate::global::handle(command),
            None => crate::help::print("config"),
        };
    }
    match c.command {
        Some(ConfigCommand::Check {
            project,
            profile,
            format,
        }) => check::handle(config, project, profile, format),
        Some(ConfigCommand::Inspect { project, format }) => {
            inspect::handle(config, project, format)
        }
        Some(ConfigCommand::Ls {
            key,
            project,
            format,
        }) => ls::handle(config, project, key, format),
        Some(ConfigCommand::Get {
            project,
            key,
            format,
        }) => get::handle(config, project, key, format),
        Some(ConfigCommand::Set {
            project,
            assignments,
        }) => set::handle(config, project, assignments),
        Some(ConfigCommand::Reset { project, keys }) => reset::handle(config, project, keys),
        Some(ConfigCommand::Sbom {
            project,
            format,
            output,
        }) => sbom::handle(config, project, format, output),
        None => crate::help::print("config"),
    }
}
