// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::instructions::Instruction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    AddressFromGlobal,
    AddressFromPc,
    AndPermissions,
    ClearTag,
    GetAddress,
    GetBase,
    GetHigh,
    GetLength,
    GetPermissions,
    GetTag,
    GetTop,
    GetType,
    IncrementAddress,
    IncrementAddressImmediate,
    Integer(Instruction),
    Jump,
    JumpRegister,
    Load,
    Move,
    RepresentableAlignment,
    RepresentableLength,
    Seal,
    SetAddress,
    SetBounds,
    SetBoundsExact,
    SetBoundsImmediate,
    SetBoundsRoundDown,
    SetHigh,
    SpecialReadWrite,
    Store,
    Subtract,
    TestEqual,
    TestSubset,
    Unseal,
}
