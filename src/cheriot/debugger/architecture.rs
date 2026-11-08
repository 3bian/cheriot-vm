// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use gdbstub::arch::Arch;

use super::{RegisterId, Registers};

pub enum Architecture {}

impl Arch for Architecture {
    type Usize = u32;
    type Registers = Registers;
    type BreakpointKind = usize;
    type RegId = RegisterId;

    fn target_description_xml() -> Option<&'static str> {
        Some(include_str!("target.xml"))
    }
}
