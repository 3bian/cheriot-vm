// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::ConfigCommand;
use crate::output;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) struct Key {
    pub name: &'static str,
    pub value: &'static str,
    pub about: &'static str,
}

pub(crate) const KEYS: &[Key] = &[
    Key {
        name: "color",
        value: "auto|always|never",
        about: "Colour text output; the --color option overrides it",
    },
    Key {
        name: "jobs",
        value: "N",
        about: "Number of parallel build jobs, 1 to 256",
    },
    Key {
        name: "targets_dir",
        value: "DIR",
        about: "Directory of target files searched after the project's and the SDK's",
    },
    Key {
        name: "template",
        value: "DIR|URL",
        about: "Template folder or Git repository that `cheriot init` copies",
    },
    Key {
        name: "toolchain_dir",
        value: "DIR",
        about: "Directory containing the CHERIoT LLVM tools",
    },
];

pub(crate) fn file() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    if let Some(file) = set("CHERIOT_GLOBAL_CONFIG") {
        return Some(PathBuf::from(file));
    }
    let directory = if cfg!(windows) {
        set("APPDATA").map(PathBuf::from)
    } else {
        set("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|directory| directory.is_absolute())
            .or_else(|| set("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    Some(directory?.join("cheriot").join("config.toml"))
}

fn key(name: &str) -> Result<&'static Key> {
    let name = name.trim_matches('/');
    KEYS.iter().find(|key| key.name == name).ok_or_else(|| {
        let names: Vec<&str> = KEYS.iter().map(|key| key.name).collect();
        AppError::Message(format!(
            "unknown global setting {name:?}{}",
            output::choices("Global settings", &names)
        ))
    })
}

fn checked(key: &Key, value: &str) -> std::result::Result<toml_edit::Value, String> {
    let value = value.trim().trim_matches('"');
    if value.is_empty() {
        return Err(format!("{} must not be empty", key.name));
    }
    match key.name {
        "color" if !["auto", "always", "never"].contains(&value) => {
            Err("color must be auto, always or never".into())
        }
        "jobs" => value
            .parse::<i64>()
            .ok()
            .filter(|jobs| (1..=256).contains(jobs))
            .map(toml_edit::Value::from)
            .ok_or_else(|| "jobs must be a whole number between 1 and 256".into()),
        "targets_dir" | "toolchain_dir" => Ok(absolute(value)?.into()),
        "template" if !value.contains("://") && !value.starts_with("git@") => {
            Ok(absolute(value)?.into())
        }
        _ => Ok(value.into()),
    }
}

fn absolute(value: &str) -> std::result::Result<String, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let value = match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(value),
    };
    let directory = std::env::current_dir().map_err(|error| error.to_string())?;
    directory
        .join(value)
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "paths must be UTF-8".into())
}

fn read(file: &Path) -> std::result::Result<toml_edit::DocumentMut, String> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("{}: {error}", file.display())),
    };
    let document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error| format!("{}: {error}", file.display()))?;
    for (name, item) in document.iter() {
        let key = KEYS
            .iter()
            .find(|key| key.name == name)
            .ok_or_else(|| format!("{}: unknown global setting {name:?}", file.display()))?;
        let text = match item.as_value() {
            Some(toml_edit::Value::String(text)) => text.value().clone(),
            Some(toml_edit::Value::Integer(number)) => number.value().to_string(),
            _ => return Err(format!("{}: {name} must be a plain value", file.display())),
        };
        checked(key, &text).map_err(|error| format!("{}: {error}", file.display()))?;
    }
    Ok(document)
}

fn text_of(item: &toml_edit::Item) -> Option<String> {
    match item.as_value()? {
        toml_edit::Value::String(text) => Some(text.value().clone()),
        other => Some(other.to_string().trim().to_owned()),
    }
}

fn load() -> std::result::Result<BTreeMap<String, String>, String> {
    let Some(file) = file() else {
        return Ok(BTreeMap::new());
    };
    Ok(read(&file)?
        .iter()
        .filter_map(|(name, item)| Some((name.to_owned(), text_of(item)?)))
        .collect())
}

pub(crate) fn value(name: &str) -> std::result::Result<Option<String>, String> {
    static SETTINGS: OnceLock<std::result::Result<BTreeMap<String, String>, String>> =
        OnceLock::new();
    SETTINGS
        .get_or_init(load)
        .as_ref()
        .map(|settings| settings.get(name).cloned())
        .map_err(Clone::clone)
}

fn required_file() -> Result<PathBuf> {
    file().ok_or_else(|| {
        AppError::Message(
            "no user configuration directory: set the HOME or the CHERIOT_GLOBAL_CONFIG environment variable"
                .into(),
        )
    })
}

fn write(file: &Path, document: &toml_edit::DocumentMut) -> Result<()> {
    let parent = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, document.to_string().as_bytes())?;
    temporary
        .persist(file)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

fn edit(change: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<()>) -> Result<u8> {
    let file = required_file()?;
    let mut document = read(&file).map_err(AppError::Message)?;
    change(&mut document)?;
    write(&file, &document)?;
    println!("Updated {}", output::path(&file));
    Ok(0)
}

fn status(file: &Path) -> (&'static str, Option<String>) {
    match read(file) {
        Err(error) => ("invalid", Some(error)),
        Ok(_) if file.is_file() => ("valid", None),
        Ok(_) => ("not present", None),
    }
}

pub(crate) fn report() -> (Option<PathBuf>, &'static str, Option<String>) {
    match file() {
        Some(file) => {
            let (status, error) = status(&file);
            (Some(file), status, error)
        }
        None => (None, "not present", None),
    }
}

pub(crate) fn handle(command: ConfigCommand) -> Result<u8> {
    let project_only = |project: &Option<String>| match project {
        Some(_) => Err(AppError::Message(
            "--project does not apply with --global".into(),
        )),
        None => Ok(()),
    };
    match command {
        ConfigCommand::Ls {
            key: path,
            project,
            format,
        } => {
            project_only(&project)?;
            if path.is_some_and(|path| !path.trim_matches('/').is_empty()) {
                return Err(AppError::Message(
                    "global settings are all at the top level".into(),
                ));
            }
            let names: Vec<&str> = KEYS.iter().map(|key| key.name).collect();
            match output::structured(&names, &format, "keys")? {
                Some(text) => println!("{text}"),
                None => names.iter().for_each(|name| println!("{name}")),
            }
            Ok(0)
        }
        ConfigCommand::Get {
            key: name,
            project,
            format,
        } => {
            project_only(&project)?;
            let key = key(&name)?;
            let file = required_file()?;
            let settings = read(&file).map_err(AppError::Message)?;
            let value = settings.get(key.name).and_then(text_of).ok_or_else(|| {
                AppError::Message(format!(
                    "{} is not set in {}",
                    key.name,
                    output::path(&file)
                ))
            })?;
            match output::structured(&value, &format, key.name)? {
                Some(text) => println!("{text}"),
                None => println!("{value}"),
            }
            Ok(0)
        }
        ConfigCommand::Set {
            assignments,
            project,
        } => {
            project_only(&project)?;
            edit(|document| {
                for assignment in &assignments {
                    let (name, value) = assignment.split_once('=').ok_or_else(|| {
                        AppError::Message(format!("expected PATH=VALUE: {assignment:?}"))
                    })?;
                    let key = key(name)?;
                    document[key.name] =
                        toml_edit::value(checked(key, value).map_err(AppError::Message)?);
                }
                Ok(())
            })
        }
        ConfigCommand::Reset { keys, project } => {
            project_only(&project)?;
            edit(|document| {
                for name in &keys {
                    document.remove(key(name)?.name);
                }
                Ok(())
            })
        }
        ConfigCommand::Check {
            project,
            profile,
            format,
        } => {
            project_only(&project)?;
            if profile.is_some() {
                return Err(AppError::Message(
                    "--profile does not apply with --global".into(),
                ));
            }
            let file = required_file()?;
            let (state, error) = status(&file);
            let report = serde_json::json!({
                "valid": error.is_none(), "file": file, "status": state, "errors": error.iter().collect::<Vec<_>>(),
            });
            match output::structured(&report, &format, "check")? {
                Some(text) => println!("{text}"),
                None => match &error {
                    Some(error) => output::error(error),
                    None if state == "valid" => {
                        println!("Global configuration valid: {}.", output::path(&file));
                    }
                    None => println!(
                        "No global configuration: {} is not present.",
                        output::path(&file)
                    ),
                },
            }
            Ok(u8::from(error.is_some()))
        }
        ConfigCommand::Inspect { project, format } => {
            project_only(&project)?;
            let file = required_file()?;
            let settings = read(&file).map_err(AppError::Message)?;
            let values: BTreeMap<&str, Option<String>> = KEYS
                .iter()
                .map(|key| (key.name, settings.get(key.name).and_then(text_of)))
                .collect();
            let document = serde_json::json!({"file": file, "settings": values});
            if let Some(text) = output::structured(&document, &format, "global")? {
                println!("{text}");
                return Ok(0);
            }
            let mut text = String::new();
            output::heading(&mut text, "Global configuration");
            output::fields(
                &mut text,
                &[
                    ("File", output::path(&file)),
                    ("Status", status(&file).0.into()),
                ],
            );
            output::heading(&mut text, "Settings");
            let rows: Vec<[String; 2]> = KEYS
                .iter()
                .map(|key| {
                    [
                        key.name.to_owned(),
                        values[key.name].clone().unwrap_or_else(|| "-".into()),
                    ]
                })
                .collect();
            output::table(&mut text, &["SETTING", "VALUE"], &rows);
            output::heading(&mut text, "Accepted values");
            let accepted: Vec<(&str, String)> = KEYS
                .iter()
                .map(|key| (key.name, format!("{}. {}.", key.value, key.about)))
                .collect();
            output::fields(&mut text, &accepted);
            print!("{text}");
            Ok(0)
        }
        ConfigCommand::Sbom { .. } => Err(AppError::Message(
            "--global does not apply to config sbom, which describes a project".into(),
        )),
    }
}
