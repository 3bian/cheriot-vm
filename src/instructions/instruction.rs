// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{
    BranchCondition, CsrOperation, ImmediateOperation, LoadKind, RegisterOperation, StoreKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    AddUpperImmediateToProgramCounter {
        destination: u8,
        value: u32,
    },
    Branch {
        condition: BranchCondition,
        first: u8,
        second: u8,
        offset: i32,
    },
    Breakpoint,
    Csr {
        operation: CsrOperation,
        destination: u8,
        source: u8,
        address: u16,
        immediate: bool,
    },
    EnvironmentCall,
    Fence,
    FenceInstruction,
    Hint,
    Immediate {
        operation: ImmediateOperation,
        destination: u8,
        source: u8,
        immediate: i32,
    },
    JumpAndLink {
        destination: u8,
        offset: i32,
    },
    JumpAndLinkRegister {
        destination: u8,
        base: u8,
        offset: i32,
    },
    Load {
        kind: LoadKind,
        destination: u8,
        base: u8,
        offset: i32,
    },
    LoadUpperImmediate {
        destination: u8,
        value: u32,
    },
    MachineReturn,
    Register {
        operation: RegisterOperation,
        destination: u8,
        first: u8,
        second: u8,
    },
    Store {
        kind: StoreKind,
        source: u8,
        base: u8,
        offset: i32,
    },
    WaitForInterrupt,
}
