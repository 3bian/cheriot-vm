// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StopReason {
    Breakpoint {
        address: u32,
    },
    BudgetExhausted,
    DebuggerInterrupt,
    GuestCompletion {
        code: u64,
    },
    GuestEbreak {
        address: u32,
    },
    HostFailure {
        message: String,
    },
    Trap {
        address: u32,
        cause: u32,
        value: u32,
    },
}
