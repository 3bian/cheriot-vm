// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::{app_error::Result, cli::DebugArgs};

mod dap;
#[cfg(feature = "gdb")]
pub(crate) mod gdb;
mod inspection;
mod start;

pub(crate) fn handle(arguments: DebugArgs, config: Option<&std::path::Path>) -> Result<u8> {
    start::handle(arguments, config)
}
