// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub(crate) type Result<T> = std::result::Result<T, AppError>;
pub(crate) fn require_utf8_path(path: &std::path::Path) -> Result<()> {
    path.to_str()
        .map(|_| ())
        .ok_or_else(|| AppError::Message(format!("path is not UTF-8: {}", path.display())))
}
