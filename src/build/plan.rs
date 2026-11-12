// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::selection;
use crate::build::workflow::request::{Action, Format};
use crate::{
    app_error::{Result, require_utf8_path},
    cli::{PlanArgs, PlanFormat},
};
use std::path::Path;
pub(super) fn handle(args: PlanArgs, config: Option<&Path>) -> Result<u8> {
    let mut request = selection::request(&args.selection, config, Action::Plan)?;
    if let Some(path) = args.output.as_deref() {
        require_utf8_path(path)?;
    }
    request.format = match args.format {
        PlanFormat::Text => Format::Text,
        PlanFormat::Json => Format::Json,
        PlanFormat::Toml => Format::Toml,
        PlanFormat::Shell => Format::Shell,
        PlanFormat::CompilationDatabase => Format::CompilationDatabase,
    };
    request.output = args.output;
    request.probe_toolchain = args.probe_toolchain;
    selection::invoke(request)
}
