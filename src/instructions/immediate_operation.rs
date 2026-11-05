// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImmediateOperation {
    Add,
    And,
    Or,
    SetLessThan,
    SetLessThanUnsigned,
    ShiftLeft,
    ShiftRightArithmetic,
    ShiftRightLogical,
    Xor,
}
