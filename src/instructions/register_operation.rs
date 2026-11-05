// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterOperation {
    Add,
    And,
    DivSigned,
    DivUnsigned,
    Mul,
    MulHighSigned,
    MulHighSignedUnsigned,
    MulHighUnsigned,
    Or,
    RemainderSigned,
    RemainderUnsigned,
    SetLessThan,
    SetLessThanUnsigned,
    ShiftLeft,
    ShiftRightArithmetic,
    ShiftRightLogical,
    Sub,
    Xor,
}
