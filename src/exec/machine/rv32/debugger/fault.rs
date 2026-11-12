// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fault {
    pub address: u32,
    pub cause: u32,
    pub resulting_pc: u32,
    pub value: u32,
}
