// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepOutcome {
    Retired {
        address: u32,
        encoded: u32,
    },
    Trapped {
        address: u32,
        cause: u32,
        value: u32,
    },
}
