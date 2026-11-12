// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::config::diagnostic::Diagnostic;

pub(crate) struct ToolCheck {
    pub(crate) executable: Option<String>,
    pub(crate) error: Diagnostic,
}
