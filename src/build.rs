// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::build::workflow::request::{Action, Format};
use crate::{
    app_error::Result,
    cli::{
        BuildArgs, CleanArgs, Format as OutputFormat, InspectArgs, PackageArgs, PlanArgs, SizeArgs,
    },
};
use std::path::Path;
pub(crate) mod build_graph;
pub(crate) mod calculations;
pub(crate) mod compilation_database;
mod execute_plan;
pub(crate) mod execution;
mod inspect;
pub(crate) mod package;
mod plan;
pub(crate) mod planning;
pub(crate) mod render;
pub(crate) mod selection;
mod size;
#[cfg(test)]
mod tests;
pub(crate) mod workflow;
pub(crate) fn handle(mut args: BuildArgs, config: Option<&Path>) -> Result<u8> {
    args.selection.jobs = args.jobs;
    if args.execute_plan.is_some() {
        return execute_plan::handle(args);
    }
    let mut request = selection::request(&args.selection, config, Action::Build)?;
    request.dry_run = args.dry_run;
    request.verbose = args.verbose;
    request.quiet = args.quiet;
    request.format = match args.format {
        OutputFormat::Json => Format::Json,
        OutputFormat::Toml => Format::Toml,
        OutputFormat::Text => request.format,
    };
    selection::invoke(request)
}

pub(crate) fn clean(args: CleanArgs, config: Option<&Path>) -> Result<u8> {
    selection::invoke(selection::request(&args.selection, config, Action::Clean)?)
}

pub(crate) fn inspect(args: InspectArgs) -> Result<u8> {
    match &args.file {
        Some(file) => inspect::handle(
            file,
            args.format,
            args.output.as_deref(),
            args.sections,
            args.symbols,
        ),
        None => crate::help::print("inspect"),
    }
}

pub(crate) fn package(args: PackageArgs, config: Option<&Path>) -> Result<u8> {
    package::handle(args, config)
}

pub(crate) fn plan(args: PlanArgs, config: Option<&Path>) -> Result<u8> {
    plan::handle(args, config)
}

pub(crate) fn size(args: SizeArgs, config: Option<&Path>) -> Result<u8> {
    size::handle(args, config)
}
