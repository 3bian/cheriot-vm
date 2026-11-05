// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::instructions::DecodedInstruction;

#[derive(Clone, Copy, Debug)]
pub enum StepOutcome {
    Executed {
        address: u32,
        decoded: DecodedInstruction,
    },
    Trapped {
        address: u32,
        cause: u32,
        interrupt: bool,
    },
}
