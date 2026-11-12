// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{presentation, read_project, sbom_export, selected};
use crate::app_error::{AppError, Result};
use crate::cli::{Format, SbomFormat};
use crate::output;
use std::path::{Path, PathBuf};

pub(super) fn handle(
    global: Option<&Path>,
    project: Option<String>,
    format: SbomFormat,
    output_file: Option<PathBuf>,
) -> Result<u8> {
    let project = read_project(&selected(global, project)?)?;
    let report = presentation::sbom(&project).map_err(AppError::Message)?;
    let document = match format {
        SbomFormat::Text => text(&report),
        SbomFormat::Json => output::structured(&report, &Format::Json, "sbom")?.unwrap_or_default(),
        SbomFormat::Toml => output::structured(&report, &Format::Toml, "sbom")?.unwrap_or_default(),
        SbomFormat::Cyclonedx => serde_json::to_string_pretty(&sbom_export::cyclonedx(&report))?,
        SbomFormat::Spdx => serde_json::to_string_pretty(&sbom_export::spdx(&report))?,
    };
    match output_file {
        Some(path) => std::fs::write(path, format!("{document}\n"))?,
        None => println!("{document}"),
    }
    Ok(0)
}

fn text(report: &presentation::Sbom) -> String {
    let mut text = String::new();
    output::heading(&mut text, "Project");
    output::fields(
        &mut text,
        &[
            ("Name", report.project.clone()),
            ("Components", report.components.len().to_string()),
            ("Unidentified files", report.unidentified.len().to_string()),
            (
                "Headers",
                if report.headers_from_build {
                    "included from the build's dependency files"
                } else {
                    "not included (build the project first)"
                }
                .into(),
            ),
        ],
    );
    let rows: Vec<[String; 5]> = report
        .components
        .iter()
        .map(|c| {
            [
                c.identity.name.clone(),
                match (&c.identity.version, c.version_source) {
                    (Some(version), Some("git")) => format!("{version} (git)"),
                    (Some(version), _) => version.clone(),
                    (None, _) => "-".into(),
                },
                if c.third_party {
                    "third-party"
                } else {
                    "first-party"
                }
                .into(),
                c.files.len().to_string(),
                c.identity.license.clone().unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    output::heading(&mut text, "Components");
    output::table(
        &mut text,
        &["COMPONENT", "VERSION", "ORIGIN", "FILES", "LICENCE"],
        &rows,
    );
    if !report.unidentified.is_empty() {
        output::heading(&mut text, "Unidentified files");
        output::lines(
            &mut text,
            report
                .unidentified
                .iter()
                .map(|file| output::path(std::path::Path::new(file))),
        );
    }
    text.trim_end().to_owned()
}
