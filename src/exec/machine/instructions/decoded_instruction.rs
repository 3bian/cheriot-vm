// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Instruction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedInstruction {
    pub encoded: u32,
    pub instruction: Instruction,
    pub length: u32,
}
