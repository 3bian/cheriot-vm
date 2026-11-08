// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::num::NonZeroUsize;

use gdbstub::arch::RegId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterId(usize);

impl RegisterId {
    pub fn byte_range(self) -> std::ops::Range<usize> {
        match self.0 {
            0..=32 => self.0 * 4..self.0 * 4 + 4,
            33..=53 => 132 + (self.0 - 33) * 8..132 + (self.0 - 33) * 8 + 8,
            _ => 300 + self.0 - 54..301 + self.0 - 54,
        }
    }
}

impl RegId for RegisterId {
    fn from_raw_id(id: usize) -> Option<(Self, Option<NonZeroUsize>)> {
        let size = match id {
            0..=32 => 4,
            33..=53 => 8,
            54..=74 => 1,
            _ => return None,
        };
        Some((Self(id), NonZeroUsize::new(size)))
    }

    fn to_raw_id(&self) -> Option<usize> {
        Some(self.0)
    }
}
