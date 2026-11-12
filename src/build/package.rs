// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) mod uf2;
use crate::{
    app_error::{AppError, Result},
    cli::PackageArgs,
    output,
};
use std::path::Path;

pub(super) fn handle(args: PackageArgs, config: Option<&Path>) -> Result<u8> {
    use crate::build::{selection, workflow, workflow::request::Action};
    let choice = args.selection.with_jobs(args.jobs);
    let (target, project) =
        crate::target::resolve_optional(&choice, config, args.target.as_deref())?;
    let (name, firmware, artifacts) = match target {
        Some(target) => {
            let name = target.name.clone();
            let mut prepared = crate::target::prepare(&choice, config, Some(&name), true, None)?;
            let package = prepared.package(&choice, config)?;
            (Some(name), prepared.context.firmware, vec![package])
        }
        None => {
            let invalid =
                |error: crate::config::diagnostic::Diagnostic| AppError::Message(error.to_string());
            let request = selection::request(&choice, config, Action::Build)?;
            let (project, plan) = workflow::build_for_package(request).map_err(invalid)?;
            let report = crate::build::execution::package_uf2(&project, &plan).map_err(invalid)?;
            let firmware =
                crate::build::build_graph::Publication::for_project(&project).public_elf();
            (None, firmware, report.artifacts)
        }
    };
    let document = serde_json::json!({
        "project": project.name, "board": project.board.name, "target": name,
        "firmware": firmware, "artifacts": artifacts,
    });
    if let Some(text) = output::structured(&document, &args.format, "package")? {
        println!("{text}");
        return Ok(0);
    }
    let mut text = String::new();
    output::heading(&mut text, "Package");
    let mut fields = vec![("Board", project.board.name.clone())];
    if let Some(name) = name {
        fields.push(("Target", name));
    }
    fields.push(("Firmware", output::path(&firmware)));
    for artifact in &artifacts {
        fields.push(("Package", output::path(artifact)));
    }
    output::fields(&mut text, &fields);
    print!("{text}");
    Ok(0)
}
