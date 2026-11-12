// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StopReason {
    Breakpoint { address: u32 },
    BudgetExhausted,
    GuestCompletion { code: u64 },
    GuestEbreak { address: u32 },
}
