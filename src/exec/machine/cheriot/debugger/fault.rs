// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Capability;

#[derive(Clone, Copy, Debug)]
pub struct Fault {
    pub address: u32,
    pub cause: u32,
    pub previous_pcc: Capability,
    pub resulting_pc: u32,
    pub value: u32,
}
