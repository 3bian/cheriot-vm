// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::{BuildArgs, Format};

pub(super) fn handle(request: BuildArgs) -> Result<u8> {
    use std::io::Read;
    if request.selection.board.is_some()
        || request.selection.firmware.is_some()
        || request.selection.mode.is_some()
        || request.selection.build_dir.is_some()
        || request.selection.output_dir.is_some()
        || request.selection.project.is_some()
        || request.selection.toolchain_dir.is_some()
        || !request.selection.overlay.is_empty()
    {
        return Err(AppError::Message(
            "--execute-plan cannot be combined with options that select a project".into(),
        ));
    }
    let file = request
        .execute_plan
        .ok_or_else(|| AppError::Message("plan file is required".into()))?;
    let reader: Box<dyn Read> = if file.as_os_str() == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(&file)?)
    };
    let mut bytes = Vec::new();
    reader.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(AppError::Message("serialized plan exceeds 64 MiB".into()));
    }
    let plan: crate::build::build_graph::BuildPlan = serde_json::from_slice(&bytes)?;
    let json = matches!(request.format, Format::Json | Format::Toml);
    if json && (request.quiet || request.verbose) {
        return Err(AppError::Message(
            "--format json cannot be combined with --quiet or --verbose".into(),
        ));
    }
    let options = crate::build::execution::ExecutionOptions {
        dry_run: request.dry_run,
        jobs: request.selection.jobs.unwrap_or(1) as usize,
        tool_stdout: if json {
            crate::build::execution::ToolStdout::Stderr
        } else {
            crate::build::execution::ToolStdout::Inherit
        },
    };
    let mut events = Vec::new();
    let outcome =
        crate::build::execution::execute_build_plan_with_options(&plan, options, |event| {
            if json {
                events.push(event.clone());
            } else if !request.quiet
                && let crate::build::execution::ExecutionEvent::Started { operation, .. } = event
            {
                eprintln!("Running {operation}");
            }
        });
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"events":events,"error":outcome.as_ref().err().map(ToString::to_string)})
            )?
        );
    }
    outcome.map_err(|e| AppError::Message(e.to_string()))?;
    Ok(0)
}
