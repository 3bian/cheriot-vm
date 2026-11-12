// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::{FlashArgs, Format};
use crate::output;
use std::path::{Path, PathBuf};

struct Report {
    fields: Vec<(&'static str, String)>,
    format: Format,
}

impl Report {
    fn add(&mut self, name: &'static str, value: impl Into<String>) {
        self.fields.push((name, value.into()));
    }

    fn print(mut self, status: &str) -> Result<()> {
        self.add("Status", status);
        let document: serde_json::Map<String, serde_json::Value> = self
            .fields
            .iter()
            .map(|(name, value)| (name.to_lowercase(), value.clone().into()))
            .collect();
        match output::structured(&document, &self.format, "flash")? {
            Some(text) => println!("{text}"),
            None => {
                let mut text = String::new();
                output::heading(&mut text, "Flash");
                let shown: Vec<(&str, String)> = self
                    .fields
                    .iter()
                    .map(|(name, value)| (*name, output::shorten_paths(value)))
                    .collect();
                output::fields(&mut text, &shown);
                print!("{text}");
            }
        }
        Ok(())
    }
}

pub(crate) fn handle(arguments: FlashArgs, config: Option<&Path>) -> Result<u8> {
    let arguments = FlashArgs {
        selection: arguments.selection.clone().with_jobs(arguments.jobs),
        ..arguments
    };
    let mut prepared = crate::target::prepare(
        &arguments.selection,
        config,
        arguments.target.as_deref(),
        true,
        None,
    )?;
    let flash = prepared.target.definition.flash.clone().ok_or_else(|| {
        AppError::Message(format!(
            "target {} does not define [flash]",
            prepared.target.name
        ))
    })?;
    prepared.context.destination = arguments.to.clone();
    let package = match &prepared.target.definition.package {
        Some(_) => Some(prepared.package(&arguments.selection, config)?),
        None => None,
    };
    let mut report = Report {
        fields: Vec::new(),
        format: arguments.format.clone(),
    };
    report.add("Target", prepared.target.name.clone());
    if let Some(description) = &prepared.target.definition.description {
        report.add("Description", description.clone());
    }
    report.add("Firmware", prepared.context.firmware.display().to_string());
    if let Some(package) = &package {
        report.add("Package", package.display().to_string());
    }

    if let Some(command) = &flash.command {
        let command = prepared.context.expand_command(command)?;
        report.add("Command", command.join(" "));
        if arguments.dry_run {
            report.print("skipped")?;
            return Ok(0);
        }
        let code = crate::target::run(&command, &prepared.context.project_dir)?;
        report.print(if code == 0 { "ok" } else { "failed" })?;
        return Ok(code);
    }

    let source = package.unwrap_or_else(|| prepared.context.firmware.clone());
    let directory = match (&arguments.to, &flash.volume, &flash.copy_to) {
        (Some(to), _, _) => PathBuf::from(to),
        (None, Some(volume), _) => match crate::target::mounted_volume(volume) {
            Some(directory) => directory,
            None if arguments.dry_run => {
                report.add("Destination", format!("volume {volume} (not mounted)"));
                report.print("skipped")?;
                return Ok(0);
            }
            None => {
                return Err(AppError::Message(format!(
                    "no mounted volume is labelled {volume}; is the board connected and mounted? Use --to DIR to name where it is"
                )));
            }
        },
        (None, None, Some(copy_to)) => PathBuf::from(prepared.context.expand(copy_to)?),
        (None, None, None) => {
            return Err(AppError::Message("[flash] has nowhere to copy to".into()));
        }
    };
    let destination = directory.join(source.file_name().unwrap_or_default());
    report.add("Destination", destination.display().to_string());
    if arguments.dry_run {
        report.print("skipped")?;
        return Ok(0);
    }
    if !directory.is_dir() {
        return Err(AppError::Message(format!(
            "{} is not a directory; is the board connected and mounted? Use --to DIR to name where it is",
            directory.display()
        )));
    }
    std::fs::copy(&source, &destination).map_err(|error| {
        AppError::Message(format!("cannot write {}: {error}", destination.display()))
    })?;
    report.print("ok")?;
    Ok(0)
}
