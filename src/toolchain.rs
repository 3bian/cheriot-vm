// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::{app_error::Result, cli::DiagnoseArgs, output};
use serde::Serialize;
use std::path::{Path, PathBuf};

mod detect;
mod verify;

pub(crate) use detect::{HOW_TO_SELECT, default_directory};

#[derive(Serialize)]
struct Tool {
    name: String,
    path: Option<PathBuf>,
    version: Option<String>,
    number: Option<String>,
    error: Option<String>,
}
#[derive(Serialize)]
struct Report {
    directory: Option<PathBuf>,
    source: String,
    tools: Vec<Tool>,
    target_supported: Option<bool>,
    target_diagnostic: Option<String>,
}
pub(crate) const TOOLS: &[&str] = &[
    "clang",
    "clang++",
    "ld.lld",
    "llvm-strip",
    "llvm-objcopy",
    "llvm-objdump",
];

fn directory(
    arguments: &DiagnoseArgs,
    project: Option<&crate::config::configuration::ResolvedProject>,
) -> Result<(Option<PathBuf>, String)> {
    use crate::app_error::AppError;
    if let Some(project) = project {
        let toolchain = &project.toolchain;
        return Ok((
            toolchain.located().then(|| toolchain.directory.clone()),
            output::shorten_paths(&toolchain.source()),
        ));
    }
    if let Some(directory) = &arguments.toolchain_dir {
        return Ok((
            Some(directory.clone()),
            "command-line option --toolchain-dir".into(),
        ));
    }
    if let Some(directory) = crate::config::configuration::read_environment("CHERIOT_TOOLCHAIN_DIR")
        .map_err(|error| AppError::Message(error.to_string()))?
    {
        return Ok((
            Some(PathBuf::from(directory)),
            "environment variable CHERIOT_TOOLCHAIN_DIR".into(),
        ));
    }
    Ok(match default_directory().map_err(AppError::Message)? {
        Some((directory, source)) => (Some(directory), source.to_owned()),
        None => (None, "not found".to_owned()),
    })
}

pub fn diagnose(d: DiagnoseArgs, config: Option<&Path>) -> Result<u8> {
    let file = config.unwrap_or_else(|| Path::new("cheriot.toml"));
    let skipped = config.is_none() && d.project.is_none() && !file.exists();
    let checked = (!skipped)
        .then(|| crate::config::validate(config, d.project.as_deref(), d.toolchain_dir.as_deref()));
    let (projects, project_errors, project) = match checked {
        Some(checked) => (checked.names, checked.errors, checked.project),
        None => (Vec::new(), Vec::new(), None),
    };
    let (directory, source) = directory(&d, project.as_ref())?;
    let report = detect::inspect(directory.as_deref(), &source);
    let (global_file, global_status, global_error) = crate::global::report();
    let ok = project_errors.is_empty()
        && global_error.is_none()
        && report.tools.iter().all(|t| t.error.is_none())
        && report.target_supported == Some(true);
    let project_status = match (project_errors.is_empty(), skipped) {
        (_, true) => "skipped",
        (false, _) => "invalid",
        (true, _) => "valid",
    };
    let document = serde_json::json!({
        "project": {
            "file": file, "status": project_status, "projects": projects,
            "error": project_errors.first(), "errors": project_errors,
        },
        "global": {"file": global_file, "status": global_status, "error": global_error},
        "toolchain": report,
    });
    if let Some(text) = output::structured(&document, &d.format, "diagnosis")? {
        println!("{text}");
        return Ok(u8::from(!ok));
    }

    let mut text = String::new();
    output::heading(&mut text, "Project");
    let mut fields = vec![(
        "File",
        if skipped {
            format!("{} (not found)", output::path(file))
        } else {
            output::path(file)
        },
    )];
    if projects.len() > 1 {
        fields.push(("Projects", projects.len().to_string()));
    }
    fields.push(("Status", project_status.into()));
    output::fields(&mut text, &fields);

    output::heading(&mut text, "Global configuration");
    output::fields(
        &mut text,
        &[
            (
                "File",
                global_file
                    .as_deref()
                    .map_or_else(|| "-".into(), output::path),
            ),
            ("Status", global_status.into()),
        ],
    );

    output::heading(&mut text, "Toolchain");
    let mut fields = vec![
        (
            "Directory",
            report
                .directory
                .as_deref()
                .map_or_else(|| "-".into(), output::path),
        ),
        ("Source", report.source.clone()),
    ];
    if let Some(version) = report.tools.first().and_then(|tool| tool.version.clone()) {
        fields.push(("Compiler", version));
    }
    output::fields(&mut text, &fields);
    text.push('\n');
    let rows: Vec<[String; 3]> = report
        .tools
        .iter()
        .map(|t| {
            [
                t.name.clone(),
                if t.error.is_some() { "missing" } else { "ok" }.into(),
                t.number.clone().unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    output::table(&mut text, &["TOOL", "STATUS", "VERSION"], &rows);

    output::heading(&mut text, "CHERIoT target");
    output::fields(
        &mut text,
        &[(
            "Compile probe",
            match report.target_supported {
                _ if report.tools.first().is_none_or(|tool| tool.path.is_none()) => "skipped",
                Some(true) => "supported",
                Some(false) => "not supported",
                None => "unknown",
            }
            .into(),
        )],
    );
    let mut problems: Vec<String> = project_errors
        .iter()
        .map(|e| format!("project: {e}"))
        .collect();
    problems.extend(global_error.iter().map(|e| format!("global: {e}")));
    let missing: Vec<&str> = report
        .tools
        .iter()
        .filter(|tool| tool.path.is_none())
        .map(|tool| tool.name.as_str())
        .collect();
    if !missing.is_empty() {
        problems.push(match &report.directory {
            Some(directory) => format!(
                "toolchain: {} has no {}.{HOW_TO_SELECT}",
                output::path(directory),
                missing.join(", ")
            ),
            None => format!(
                "toolchain: no directory on PATH holds all of {}.{HOW_TO_SELECT}",
                TOOLS.join(", ")
            ),
        });
    }
    problems.extend(report.tools.iter().filter_map(|tool| {
        let error = tool.error.as_ref().filter(|_| tool.path.is_some())?;
        Some(format!("{}: {error}", tool.name))
    }));
    if missing.is_empty()
        && report.target_supported == Some(false)
        && let Some(directory) = &report.directory
    {
        problems.push(format!(
            "toolchain: the compiler in {} does not build for CHERIoT.{HOW_TO_SELECT}",
            output::path(directory)
        ));
    }
    let compiler_found = report.tools.first().is_some_and(|tool| tool.path.is_some());
    if let Some(diagnostic) = report.target_diagnostic.as_ref().filter(|_| compiler_found) {
        problems.extend(
            diagnostic
                .lines()
                .map(|line| format!("compile probe: {line}")),
        );
    }
    if !problems.is_empty() {
        output::heading(&mut text, "Problems");
        output::lines(
            &mut text,
            problems
                .iter()
                .map(|problem| output::shorten_paths(problem)),
        );
    }
    print!("{text}");
    Ok(u8::from(!ok))
}
