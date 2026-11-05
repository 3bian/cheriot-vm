// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::execution::TrapPolicy;

use crate::cli::output::OutputPresentation;

pub(crate) struct RunOptions {
    pub breakpoint: Option<u32>,
    pub memory_dump: Option<(u32, usize)>,
    pub presentation: OutputPresentation,
    pub registers: Option<crate::cli::register_selection::RegisterSelection>,
    pub steps: Option<u64>,
    pub trace: bool,
    pub trap_policy: TrapPolicy,
    pub uart: crate::cli::uart::connections::UartConnections,
}
