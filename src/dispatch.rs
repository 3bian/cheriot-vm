// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::{
    app_error::{AppError, Result},
    cli::{Cli, Command},
};
use std::path::PathBuf;

pub(crate) fn dispatch(cli: Cli) -> Result<u8> {
    crate::output::set_colour(cli.color.as_ref());
    let config = match cli.config {
        Some(file) => Some(file),
        None => crate::config::configuration::read_environment("CHERIOT_CONFIG")
            .map_err(|error| AppError::Message(error.to_string()))?
            .map(PathBuf::from),
    };
    let config = config.as_deref();
    match cli.command {
        Command::Build(args) => crate::build::handle(args, config),
        Command::Clean(args) => crate::build::clean(args, config),
        Command::Config(args) => crate::config::handle(args, config),
        Command::Debug(args) => crate::debug::handle(args, config),
        Command::Diagnose(args) => crate::diagnose::handle(args, config),
        Command::Exec(args) => crate::exec::handle(args, config),
        Command::Flash(args) => crate::flash::handle(args, config),
        Command::Init(args) => crate::init::handle(args),
        Command::Inspect(args) => crate::build::inspect(args),
        Command::Package(args) => crate::build::package(args, config),
        Command::Plan(args) => crate::build::plan(args, config),
        Command::Size(args) => crate::build::size(args, config),
        Command::Test(args) => crate::test::handle(args, config),
    }
}
