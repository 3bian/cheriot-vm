// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs;

use crate::build::build_graph::{BuildAction, BuildPlan, ProbeResults};
use crate::build::compilation_database;
use crate::build::execution;
use crate::build::planning;
use crate::build::render;
use crate::config::configuration;
use crate::config::configuration::ResolvedProject;
use crate::config::diagnostic::{Diagnostic, Result};
use command_name::CommandName;
use options::Options;
use request::Format as OutputFormat;

mod command_name;
mod options;
mod progress;
pub mod request;
mod size;
mod typed_request;

pub fn execute_request(request: request::Request) -> Result<i32> {
    execute_options(Options::from_request(request)?)
}

fn execute_options(mut arguments: Options) -> Result<i32> {
    let as_toml = arguments.output_format == OutputFormat::Toml
        && arguments.command_name != CommandName::Configuration;
    if as_toml {
        arguments.output_format = OutputFormat::Json;
    }
    let (text, exit_code) = if arguments.command_name == CommandName::Build
        && arguments.output_format == OutputFormat::Json
    {
        build_as_json(&arguments)?
    } else {
        (execute_project_command(&arguments)?, 0)
    };

    let text = if as_toml && !text.is_empty() {
        crate::output::json_text_to_toml(&text, arguments.command_name.name())
            .map_err(Diagnostic::new)?
    } else {
        text
    };
    if let Some(path) = &arguments.output_file {
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|error| Diagnostic::io(parent, error))?;
        }
        fs::write(path, text).map_err(|error| Diagnostic::io(path, error))?;
    } else if !text.is_empty() {
        println!("{text}");
    }
    Ok(exit_code)
}

pub fn build_for_package(request: request::Request) -> Result<(ResolvedProject, BuildPlan)> {
    let arguments = Options::from_request(request)?;
    if arguments.command_name != CommandName::Build || arguments.dry_run {
        return Err(Diagnostic::new("package requires an actual build"));
    }
    let project = selected_project(&arguments)?;
    if !matches!(
        project.board.packaging,
        Some(configuration::PackagingKind::SonataLegacy | configuration::PackagingKind::SonataV1)
    ) {
        return Err(Diagnostic::new(
            "this board does not have a supported UF2 package layout",
        ));
    }
    let plan = planning::create_build_plan(&project)?;
    execute_with_progress(&arguments, &project, &plan, execution::ToolStdout::Stderr)?;
    Ok((project, plan))
}

pub fn resolved_project(request: request::Request) -> Result<ResolvedProject> {
    selected_project(&Options::from_request(request)?)
}

pub fn published_firmware(request: request::Request) -> Result<std::path::PathBuf> {
    let arguments = Options::from_request(request)?;
    if arguments.command_name != CommandName::Build {
        return Err(Diagnostic::new("firmware lookup requires a build request"));
    }
    let project = selected_project(&arguments)?;
    Ok(crate::build::build_graph::Publication::for_project(&project).public_elf())
}

pub fn build_firmware(request: request::Request) -> Result<std::path::PathBuf> {
    let arguments = Options::from_request(request)?;
    if arguments.command_name != CommandName::Build || arguments.dry_run {
        return Err(Diagnostic::new("firmware build requires an actual build"));
    }
    let project = selected_project(&arguments)?;
    let plan = planning::create_build_plan(&project)?;
    execute_with_progress(&arguments, &project, &plan, execution::ToolStdout::Stderr)?;
    plan.publication
        .as_ref()
        .map(|publication| publication.public_elf())
        .ok_or_else(|| Diagnostic::new("the plan has no firmware output"))
}

fn selected_project(arguments: &Options) -> Result<ResolvedProject> {
    let path = configuration::select_project(
        &arguments.configuration_file,
        arguments.project_name.as_deref(),
    )?;
    crate::config::project::load_project_with_settings(
        &path,
        arguments.firmware_name.as_deref(),
        &arguments.settings,
    )
}

fn execution_options(
    arguments: &Options,
    tool_stdout: execution::ToolStdout,
) -> execution::ExecutionOptions {
    execution::ExecutionOptions {
        dry_run: arguments.dry_run,
        jobs: arguments.jobs,
        tool_stdout,
    }
}

fn execute_with_progress(
    arguments: &Options,
    project: &ResolvedProject,
    plan: &BuildPlan,
    tool_stdout: execution::ToolStdout,
) -> Result<ProbeResults> {
    let mut progress = progress::BuildProgress::new(
        project,
        plan,
        arguments.verbose,
        arguments.quiet,
        arguments.dry_run,
    );
    let result = execution::execute_build_plan_with_options(
        plan,
        execution_options(arguments, tool_stdout),
        |event| progress.event(event),
    );
    progress.finish(result.is_ok());
    result
}

fn build_as_json(arguments: &Options) -> Result<(String, i32)> {
    let project = selected_project(arguments)?;
    let plan = planning::create_build_plan(&project)?;
    let mut events = Vec::new();
    let result = execution::execute_build_plan_with_options(
        &plan,
        execution_options(arguments, execution::ToolStdout::Stderr),
        |event| {
            events.push(event.clone());
        },
    );
    let status = if result.is_ok() { 0 } else { 1 };
    let report = serde_json::json!({
        "events": events,
        "error": result.err().map(|error| error.to_string()),
    });
    Ok((serde_json::to_string_pretty(&report)?, status))
}

fn execute_project_command(arguments: &Options) -> Result<String> {
    use crate::config::presentation::Rendering;
    let project = selected_project(arguments)?;
    match arguments.command_name {
        CommandName::Configuration => match arguments.output_format {
            OutputFormat::Text => crate::config::presentation::render(&project, Rendering::Text)
                .map_err(Diagnostic::new),
            OutputFormat::Toml => crate::config::presentation::render(&project, Rendering::Toml)
                .map_err(Diagnostic::new),
            OutputFormat::Json => crate::config::presentation::render(&project, Rendering::Json)
                .map_err(Diagnostic::new),
            _ => Err(Diagnostic::new("configuration format must be toml or json")),
        },
        CommandName::Size => size::execute(arguments, &project),
        _ => {
            let plan = if arguments.include_packaging {
                planning::create_packaging_plan(&project)?
            } else {
                planning::create_build_plan(&project)?
            };
            let probes = if arguments.probe_toolchain {
                execution::probe_toolchain(&plan)?
            } else {
                plan.unresolved_probes()
            };
            execute_plan_command(arguments, &project, &plan, &probes)
        }
    }
}

fn execute_plan_command(
    arguments: &Options,
    project: &ResolvedProject,
    plan: &BuildPlan,
    probes: &ProbeResults,
) -> Result<String> {
    match arguments.command_name {
        CommandName::Plan => match arguments.output_format {
            OutputFormat::Json => {
                let mut document = serde_json::to_value(plan)?;
                let object = document
                    .as_object_mut()
                    .ok_or_else(|| Diagnostic::new("invalid plan document"))?;
                object.insert("probe_results".into(), serde_json::to_value(probes)?);
                Ok(serde_json::to_string_pretty(&document)?)
            }
            OutputFormat::Shell => render::render_shell_script(plan),
            OutputFormat::CompilationDatabase => Ok(serde_json::to_string_pretty(
                &compilation_database::export_compilation_database(plan, probes)?,
            )?),
            OutputFormat::Text => Ok(describe_plan(plan, probes)),
            _ => Err(Diagnostic::new(
                "plan format must be text, json, shell or compilation-database",
            )),
        },
        CommandName::Build => {
            if arguments.output_format != OutputFormat::Human {
                return Err(Diagnostic::new("build format must be json"));
            }
            execute_with_progress(arguments, project, plan, execution::ToolStdout::Inherit)?;
            Ok(String::new())
        }
        CommandName::Clean => Ok(match execution::clean_build_plan(plan)? {
            0 => "Nothing to remove: no build files are present.".to_owned(),
            1 => "Removed 1 build file.".to_owned(),
            count => format!("Removed {count} build files."),
        }),
        _ => Err(Diagnostic::new("invalid command")),
    }
}

fn kind_label(kind: &crate::build::build_graph::BuildOperationKind) -> &'static str {
    use crate::build::build_graph::BuildOperationKind as Kind;
    match kind {
        Kind::Generate => "generate",
        Kind::Compile => "compile",
        Kind::Assemble => "assemble",
        Kind::PartialLink => "partial link",
        Kind::Link => "link",
        Kind::PostProcess => "post-process",
        Kind::Probe => "probe",
    }
}

fn working_directory(plan: &BuildPlan) -> Option<&std::path::Path> {
    let mut directories = plan
        .operations
        .iter()
        .filter_map(|operation| match &operation.action {
            BuildAction::Command { command, .. }
            | BuildAction::Probe { command }
            | BuildAction::GitDescription { command } => Some(command.cwd.as_path()),
            _ => None,
        });
    let first = directories.next()?;
    directories
        .all(|directory| directory == first)
        .then_some(first)
}

fn describe_plan(plan: &BuildPlan, probes: &ProbeResults) -> String {
    use crate::output;
    let mut kinds: std::collections::BTreeMap<&str, usize> = Default::default();
    for operation in &plan.operations {
        *kinds.entry(kind_label(&operation.kind)).or_default() += 1;
    }
    let directory = working_directory(plan);
    let mut report = String::new();
    output::heading(&mut report, "Plan");
    let mut summary = vec![("Profile", plan.profile.clone())];
    if let Some(directory) = directory {
        summary.push(("Directory", output::path(directory)));
    }
    summary.push(("Build directory", output::path(&plan.build_dir)));
    if let Some(publication) = &plan.publication {
        summary.push(("Firmware", output::path(&publication.public_elf())));
    }
    summary.push((
        "Operations",
        format!(
            "{}: {}",
            plan.operations.len(),
            kinds
                .iter()
                .map(|(kind, count)| format!("{count} {kind}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));
    output::fields(&mut report, &summary);
    let mut notes = plan.notes.clone();
    if directory.is_some() {
        notes.push(
            "Commands run in the directory above; paths inside it are written relative to it."
                .into(),
        );
    }
    output::heading(&mut report, "Notes");
    output::lines(&mut report, &notes);
    output::heading(&mut report, "Operations");
    for (index, operation) in plan.operations.iter().enumerate() {
        if index > 0 {
            report.push('\n');
        }
        report.push_str(&format!(
            "  {}  {}\n",
            output::bold(&operation.id),
            output::dim(kind_label(&operation.kind))
        ));
        for line in describe_operation(operation, probes, directory) {
            report.push_str(&format!("      {line}\n"));
        }
    }
    report.trim_end().to_owned()
}

fn describe_operation(
    operation: &crate::build::build_graph::BuildOperation,
    probes: &ProbeResults,
    directory: Option<&std::path::Path>,
) -> Vec<String> {
    let path =
        |path: &std::path::Path| match directory.and_then(|base| path.strip_prefix(base).ok()) {
            Some(relative) => relative.display().to_string(),
            None => path.display().to_string(),
        };
    let result = |probe: &str| match probes.get(probe).copied().flatten() {
        Some(true) => "succeeded",
        Some(false) => "failed",
        None => "not run yet",
    };
    let mut lines = Vec::new();
    match &operation.action {
        BuildAction::Publish => {
            for (input, output) in operation.inputs.iter().zip(operation.outputs.iter()) {
                lines.push(format!("publish {}", path(input)));
                lines.push(format!("  -> {}", path(output)));
            }
        }
        BuildAction::Copy { input } => {
            lines.push(format!("copy {}", path(input)));
            lines.push(format!("  -> {}", path(&operation.outputs[0])));
        }
        BuildAction::Generate { template, .. } => {
            lines.push(format!("render {template}"));
            lines.push(format!("  -> {}", path(&operation.outputs[0])));
        }
        BuildAction::MemoryImage { input, word_bytes } => {
            lines.push(format!(
                "convert {} to {word_bytes}-byte memory words",
                path(input)
            ));
            lines.push(format!("  -> {}", path(&operation.outputs[0])));
        }
        BuildAction::Command {
            command,
            conditional,
        } => {
            lines.extend(render::format_shell_command(command, directory));
            if let Some(condition) = conditional {
                lines.push(format!(
                    "if {} succeeds ({}), also pass:",
                    condition.probe,
                    result(&condition.probe)
                ));
                lines.push(format!(
                    "  {}",
                    condition
                        .arguments
                        .iter()
                        .map(|argument| render::quote_shell_argument(&argument.value))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
        BuildAction::Probe { command } => {
            lines.push(format!(
                "compiler feature probe ({}):",
                result(&operation.id)
            ));
            lines.extend(render::format_shell_command(command, directory));
        }
        BuildAction::GitDescription { command } => {
            lines.push("Git description, or `unknown` when this fails:".into());
            lines.extend(render::format_shell_command(command, directory));
        }
    }
    lines
}
