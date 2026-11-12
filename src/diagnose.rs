// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::{app_error::Result, cli::DiagnoseArgs};
use std::path::Path;
pub(crate) fn handle(args: DiagnoseArgs, config: Option<&Path>) -> Result<u8> {
    crate::toolchain::diagnose(args, config)
}
