// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::selection;
use crate::build::workflow::request::{Action, Format};
use crate::{
    app_error::{Result, require_utf8_path},
    cli::{SizeArgs, SizeFormat},
};
use std::path::Path;
pub(super) fn handle(args: SizeArgs, config: Option<&Path>) -> Result<u8> {
    let choice = args.selection.with_jobs(args.jobs);
    let mut request = selection::request(&choice, config, Action::Size)?;
    if let Some(path) = args.output.as_deref() {
        require_utf8_path(path)?;
    }
    request.format = match args.format {
        SizeFormat::Text => Format::Text,
        SizeFormat::Json => Format::Json,
        SizeFormat::Toml => Format::Toml,
        SizeFormat::Bytes => Format::Bytes,
    };
    request.compartment = args.compartment;
    request.output = args.output;
    selection::invoke(request)
}
