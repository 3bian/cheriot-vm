// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::InitArgs;
use crate::output;
use std::path::{Path, PathBuf};

fn is_repository(template: &str) -> bool {
    template.contains("://") || template.starts_with("git@") || template.ends_with(".git")
}

fn locate(explicit: Option<&str>) -> Result<String> {
    if let Some(template) = explicit {
        return Ok(template.to_owned());
    }
    if let Ok(template) = std::env::var("CHERIOT_TEMPLATE")
        && !template.is_empty()
    {
        return Ok(template);
    }
    if let Some(template) = crate::global::value("template").map_err(AppError::Message)? {
        return Ok(template);
    }
    crate::resources::directories("template")
        .into_iter()
        .find(|candidate| candidate.is_dir())
        .map(|found| found.display().to_string())
        .ok_or_else(|| {
            AppError::Message(
                "no template folder found. Name one with --template DIR|URL, the CHERIOT_TEMPLATE environment variable, or `cheriot config --global set template=DIR|URL`"
                    .into(),
            )
        })
}

fn files(root: &Path, directory: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name() == ".git" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            files(root, &path, found)?;
        } else {
            found.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    Ok(())
}

pub(crate) fn handle(arguments: InitArgs) -> Result<u8> {
    let destination = arguments
        .directory
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&destination)?;
    let destination = std::fs::canonicalize(&destination)?;
    let name = arguments
        .name
        .clone()
        .or_else(|| {
            destination
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "project".into());
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Message(format!(
            "project name {name:?} may only contain letters, digits, '-' and '_'; pass --name"
        )));
    }

    let template = locate(arguments.template.as_deref())?;
    let mut checkout = None;
    let source = if is_repository(&template) {
        let directory = tempfile::tempdir()?;
        let status = std::process::Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", &template])
            .arg(directory.path())
            .status()
            .map_err(|error| AppError::Message(format!("cannot run git: {error}")))?;
        if !status.success() {
            return Err(AppError::Message(format!(
                "cannot clone template {template}"
            )));
        }
        let path = directory.path().to_path_buf();
        checkout = Some(directory);
        path
    } else {
        PathBuf::from(&template)
    };
    if !source.is_dir() {
        return Err(AppError::Message(format!(
            "template {} is not a directory",
            source.display()
        )));
    }

    let mut relative = Vec::new();
    files(&source, &source, &mut relative)?;
    relative.sort();
    if relative.is_empty() {
        return Err(AppError::Message(format!("template {template} is empty")));
    }
    let existing: Vec<String> = relative
        .iter()
        .filter(|file| destination.join(file).exists())
        .map(|file| file.display().to_string())
        .collect();
    if !existing.is_empty() && !arguments.force {
        return Err(AppError::Message(format!(
            "{} already has: {}. Use --force to overwrite",
            destination.display(),
            existing.join(", ")
        )));
    }
    for file in &relative {
        let target = destination.join(file);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = std::fs::read(source.join(file))?;
        match String::from_utf8(bytes) {
            Ok(text) => std::fs::write(
                &target,
                text.replace("{{name}}", &name)
                    .replace("{{sdk}}", &arguments.sdk)
                    .replace("{{board}}", &arguments.board),
            )?,
            Err(error) => std::fs::write(&target, error.into_bytes())?,
        }
    }
    drop(checkout);

    let mut text = String::new();
    output::heading(&mut text, "Project");
    output::fields(
        &mut text,
        &[
            ("Name", name),
            ("Directory", output::path(&destination)),
            ("Template", output::shorten_paths(&template)),
            ("Files", relative.len().to_string()),
        ],
    );
    output::heading(&mut text, "Next steps");
    output::lines(
        &mut text,
        [
            format!("Make the CHERIoT RTOS SDK available at {}", arguments.sdk),
            "cheriot config check".to_owned(),
            "cheriot build".to_owned(),
            "cheriot test".to_owned(),
        ],
    );
    print!("{text}");
    Ok(0)
}
