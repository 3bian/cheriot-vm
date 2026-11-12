// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::Result;
use std::path::{Path, PathBuf};

pub(super) fn handle(
    global: Option<&Path>,
    project: Option<String>,
    profile: Option<PathBuf>,
    format: crate::cli::Format,
) -> Result<u8> {
    let file = global.unwrap_or(Path::new("cheriot.toml")).to_path_buf();
    let checked = super::validate(global, project.as_deref(), None);
    let projects = checked.names;
    let mut errors = checked.errors;
    let mut profile_name = None;
    if let Some(profile) = profile {
        let validation = (|| -> std::result::Result<String, String> {
            let selected = crate::config::profiles::select(&file, Some(&profile))?;
            let data = match selected.text {
                Some(text) => text.into_bytes(),
                None => crate::exec::machine::input::read_file(
                    &profile,
                    crate::exec::machine::MachineConfiguration::MAX_PROFILE_BYTES,
                    "profile",
                )
                .map_err(|e| e.to_string())?,
            };
            let text = std::str::from_utf8(&data).map_err(|e| e.to_string())?;
            let config = crate::exec::machine::MachineConfiguration::parse(text)
                .map_err(|e| e.to_string())?;
            let name = config.name.clone();
            config.validate_profile().map_err(|e| e.to_string())?;
            Ok(name)
        })();
        match validation {
            Ok(name) => profile_name = Some(name),
            Err(e) => errors.push(format!("profile {}: {e}", profile.display())),
        }
    }
    let valid = errors.is_empty();
    let report = serde_json::json!({"valid":valid,"projects":projects,"profile":profile_name,"errors":errors});
    match crate::output::structured(&report, &format, "check")? {
        Some(text) => println!("{text}"),
        None => {
            if valid {
                match projects.as_slice() {
                    [project] => println!("Configuration valid: {project}."),
                    projects => {
                        println!("Configuration valid: {} projects.\n", projects.len());
                        print!("{}", crate::output::columns(projects));
                    }
                }
                if let Some(name) = profile_name {
                    println!("Machine profile valid: {name}.");
                }
            } else {
                for error in errors {
                    crate::output::error(&error);
                }
            }
        }
    }
    Ok(u8::from(!valid))
}
