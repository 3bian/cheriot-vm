// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::{
    app_error::{Result, require_utf8_path},
    cli::Format as OutputFormat,
};
use std::path::Path;

pub(super) fn handle(
    config: Option<&Path>,
    project: Option<String>,
    format: OutputFormat,
) -> Result<u8> {
    use crate::build::workflow::request::{Action, Format, Request, Selection};
    if let Some(path) = config {
        require_utf8_path(path)?;
    }
    let mut request = Request::new(
        Action::Configuration,
        Selection {
            file: config.map(Path::to_path_buf),
            project,
            ..Selection::default()
        },
    );
    request.format = match format {
        OutputFormat::Text => Format::Text,
        OutputFormat::Toml => Format::Toml,
        OutputFormat::Json => Format::Json,
    };
    crate::build::selection::invoke(request)
}
