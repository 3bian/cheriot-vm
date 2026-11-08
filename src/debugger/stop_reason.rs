// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::BreakpointType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Breakpoint(u32, BreakpointType),
    Completed(u64),
    Ebreak(u32),
    Fault(u32),
    Interrupted,
    OutputLimit,
    RunLimit,
    Step,
    Startup,
}

impl StopReason {
    pub(crate) fn from_fault(address: u32, cause: u32) -> Option<Self> {
        if cause & 0x8000_0000 != 0 {
            None
        } else if cause == 3 {
            Some(Self::Ebreak(address))
        } else {
            Some(Self::Fault(cause))
        }
    }
}
