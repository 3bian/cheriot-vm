// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::num::NonZeroUsize;

use gdbstub::arch::RegId;

pub const CSR_ADDRESSES: [u16; 26] = [
    0x300, 0x301, 0x304, 0x305, 0x310, 0x320, 0x340, 0x341, 0x342, 0x343, 0x344, 0xb00, 0xb02,
    0xb80, 0xb82, 0xc00, 0xc01, 0xc02, 0xc80, 0xc81, 0xc82, 0xf11, 0xf12, 0xf13, 0xf14, 0xf15,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterId(usize);

impl RegisterId {
    pub fn byte_range(self) -> std::ops::Range<usize> {
        self.0 * 4..self.0 * 4 + 4
    }
}

impl RegId for RegisterId {
    fn from_raw_id(id: usize) -> Option<(Self, Option<NonZeroUsize>)> {
        let index = if id <= 32 {
            id
        } else {
            CSR_ADDRESSES
                .iter()
                .position(|address| usize::from(*address) + 65 == id)?
                + 33
        };
        Some((Self(index), NonZeroUsize::new(4)))
    }

    fn to_raw_id(&self) -> Option<usize> {
        Some(if self.0 <= 32 {
            self.0
        } else {
            usize::from(CSR_ADDRESSES[self.0 - 33]) + 65
        })
    }
}
