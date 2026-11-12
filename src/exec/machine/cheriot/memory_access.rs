// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryAccess {
    Read {
        address: u32,
        width: usize,
        value: u64,
        tag: Option<bool>,
    },
    RevocationRead {
        address: u32,
        value: u8,
    },
    Write {
        address: u32,
        width: usize,
        value: u64,
        tag: Option<bool>,
    },
}
