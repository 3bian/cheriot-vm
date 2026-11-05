// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    Breakpoint(u32),
    Completed(u64),
    Ebreak(u32),
    Fault(u32),
    Interrupted,
    OutputLimit,
    RunLimit,
    Step,
    Startup,
}
