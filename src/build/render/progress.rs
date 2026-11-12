// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::build::build_graph::{BuildAction, BuildCommand, BuildOperationKind, BuildPlan};
use crate::config::configuration::{BuildMode, ResolvedProject};

pub fn introduction(project: &ResolvedProject, verbose: bool, dry_run: bool) -> Vec<String> {
    let mode = match project.mode {
        BuildMode::Debug => "debug",
        BuildMode::Release => "release",
    };
    let mut lines = vec![format!(
        "{} {} (board: {}, mode: {})",
        if dry_run { "Checking" } else { "Building" },
        project.firmware.name,
        project.board.name,
        mode
    )];
    if verbose {
        let toolchain = &project.toolchain;
        lines.push(format!("Directory: {}", project.root.display()));
        lines.push(if toolchain.located() {
            format!(
                "Toolchain: {} ({})",
                toolchain.directory.display(),
                toolchain.source()
            )
        } else {
            "Toolchain: not found".to_owned()
        });
    }
    lines
}

pub fn stage(kind: &BuildOperationKind, action: &BuildAction) -> Option<&'static str> {
    if matches!(action, BuildAction::GitDescription { .. }) {
        return None;
    }
    Some(match kind {
        BuildOperationKind::Generate => "Generating files",
        BuildOperationKind::Compile => "Compiling sources",
        BuildOperationKind::Assemble => "Assembling sources",
        BuildOperationKind::PartialLink => "Linking components",
        BuildOperationKind::Link => "Linking firmware",
        BuildOperationKind::PostProcess => "Writing build outputs",
        BuildOperationKind::Probe => "Checking compiler features",
    })
}

pub fn verbose_command(command: &BuildCommand, base: &std::path::Path) -> Vec<String> {
    let mut display = command.clone();
    display.environment.clear();
    super::format_shell_command(&display, Some(base))
}

pub fn conclusion(
    plan: &BuildPlan,
    succeeded: bool,
    dry_run: bool,
    count: usize,
    skipped: usize,
    elapsed_seconds: f64,
) -> Vec<String> {
    let mut lines = vec![if !succeeded {
        format!("Build failed after {elapsed_seconds:.1}s.")
    } else if dry_run {
        format!("{count} actions would run; {skipped} are up to date.")
    } else if count == 0 {
        format!("Firmware is up to date ({skipped} actions skipped).")
    } else {
        format!(
            "Build succeeded in {elapsed_seconds:.1}s ({count} actions run, {skipped} up to date)."
        )
    }];
    if succeeded && let Some(publication) = &plan.publication {
        lines.push(format!("Firmware: {}", publication.public_elf().display()));
        if publication.report_enabled {
            lines.push(format!("Report: {}", publication.public_report().display()));
        }
    }
    lines
}
