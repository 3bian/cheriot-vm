// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};

pub(crate) const KEY: &str = "profiles";

pub(crate) struct Profile {
    pub file: PathBuf,
    pub text: Option<String>,
}

fn embedded(config: &Path) -> Result<toml::Table, String> {
    let Ok(source) = std::fs::read_to_string(config) else {
        return Ok(toml::Table::new());
    };
    let document: toml::Table =
        toml::from_str(&source).map_err(|error| format!("{}: {error}", config.display()))?;
    match document.get(KEY) {
        None => Ok(toml::Table::new()),
        Some(toml::Value::Table(profiles)) => Ok(profiles.clone()),
        Some(_) => Err(format!(
            "{}: profiles must be a table of machine profiles",
            config.display()
        )),
    }
}

pub(crate) fn select(config: &Path, value: Option<&Path>) -> Result<Profile, String> {
    let profiles = embedded(config)?;
    let names = || {
        let names: Vec<&String> = profiles.keys().collect();
        crate::output::choices("Profiles", &names)
    };
    let name = match value {
        Some(value) => match value.to_str().filter(|name| profiles.contains_key(*name)) {
            Some(name) => name.to_owned(),
            None => {
                return Ok(Profile {
                    file: value.to_path_buf(),
                    text: None,
                });
            }
        },
        None => match profiles.len() {
            0 => {
                return Err(format!(
                    "no machine profile selected: pass --profile FILE, or define [profiles.NAME] in {}",
                    config.display()
                ));
            }
            1 => profiles.keys().next().cloned().unwrap_or_default(),
            _ => {
                return Err(format!(
                    "{} defines several profiles; select one with --profile NAME{}",
                    config.display(),
                    names()
                ));
            }
        },
    };
    let text = toml::to_string(&profiles[&name])
        .map_err(|error| format!("profile {name} in {}: {error}", config.display()))?;
    Ok(Profile {
        file: config.to_path_buf(),
        text: Some(text),
    })
}
