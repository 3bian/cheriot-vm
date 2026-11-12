// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Operation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedInstruction {
    pub destination: u8,
    pub encoded: u32,
    pub first: u8,
    pub immediate: i32,
    pub length: u32,
    pub operation: Operation,
    pub second: u8,
}
