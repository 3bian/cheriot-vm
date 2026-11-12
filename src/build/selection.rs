// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::build::workflow::request::{self, Action, Request};
use crate::{
    app_error::{AppError, Result, require_utf8_path},
    cli::{Mode, Selection},
};
use std::path::Path;
pub(crate) fn request(s: &Selection, config: Option<&Path>, action: Action) -> Result<Request> {
    for path in [config, s.build_dir.as_deref(), s.output_dir.as_deref()]
        .into_iter()
        .flatten()
        .chain(s.overlay.iter().map(Path::new))
        .chain(s.toolchain_dir.as_deref())
    {
        require_utf8_path(path)?;
    }
    let selection = request::Selection {
        file: config.map(Path::to_path_buf),
        project: s.project.clone(),
        firmware: s.firmware.clone(),
        board: s.board.clone(),
        mode: s.mode.as_ref().map(|m| {
            match m {
                Mode::Debug => "debug",
                Mode::Release => "release",
            }
            .into()
        }),
        toolchain_dir: s.toolchain_dir.clone(),
        build_dir: s.build_dir.clone(),
        output_dir: s.output_dir.clone(),
        overlays: s.overlay.clone(),
        jobs: s.jobs,
    };
    Ok(Request::new(action, selection))
}
pub(crate) fn invoke(request: Request) -> Result<u8> {
    let code = crate::build::workflow::execute_request(request)
        .map_err(|e| AppError::Message(e.to_string()))?;
    u8::try_from(code)
        .map_err(|_| AppError::Message(format!("invalid build-tool exit code {code}")))
}
