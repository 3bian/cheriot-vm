// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::build::build_graph::{BuildOperationKind, BuildPlan, CommandResolution};
use crate::config::diagnostic::{Diagnostic, Result};

mod compilation_entry;

pub use compilation_entry::CompilationEntry;

pub fn export_compilation_database(
    plan: &BuildPlan,
    probes: &crate::build::build_graph::ProbeResults,
) -> Result<Vec<CompilationEntry>> {
    let mut result = Vec::new();
    for operation in &plan.operations {
        if !matches!(
            operation.kind,
            BuildOperationKind::Compile | BuildOperationKind::Assemble
        ) {
            continue;
        }
        if let Some(resolution) = operation.command_resolution(probes)? {
            let variants = match resolution {
                CommandResolution::Concrete(command) => vec![(command, None)],
                CommandResolution::Alternatives {
                    probe,
                    when_false,
                    when_true,
                } => vec![
                    (when_false, Some(format!("{probe}=false (unresolved)"))),
                    (when_true, Some(format!("{probe}=true (unresolved)"))),
                ],
            };
            for (command, condition) in variants {
                let mut arguments = vec![command.executable];
                arguments.extend(command.argv.into_iter().map(|argument| argument.value));
                result.push(CompilationEntry {
                    directory: command.cwd.display().to_string(),
                    file: operation
                        .inputs
                        .first()
                        .ok_or_else(|| Diagnostic::new("compile input missing"))?
                        .display()
                        .to_string(),
                    output: operation
                        .outputs
                        .first()
                        .map(|path| path.display().to_string()),
                    arguments,
                    command: None,
                    cheriot_condition: condition,
                    cheriot_target: operation
                        .id
                        .strip_prefix("compile.")
                        .and_then(|id| id.rsplit_once('.').map(|(target, _)| target.to_owned())),
                });
            }
        }
    }
    Ok(result)
}
