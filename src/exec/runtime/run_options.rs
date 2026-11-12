// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::execution::TrapPolicy;

use crate::exec::runtime::output::OutputPresentation;

pub(crate) struct RunOptions {
    pub insights: Option<std::rc::Rc<std::cell::RefCell<crate::exec::runtime::insights::Insights>>>,
    pub breakpoint: Vec<u32>,
    pub memory_dump: Option<(u32, usize)>,
    pub presentation: OutputPresentation,
    pub registers: Option<crate::exec::runtime::register_selection::RegisterSelection>,
    pub steps: Option<u64>,
    pub trace: bool,
    pub trap_policy: TrapPolicy,
    pub uart: crate::exec::runtime::uart::connections::UartConnections,
}
