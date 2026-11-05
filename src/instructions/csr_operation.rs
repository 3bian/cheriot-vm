// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CsrOperation {
    ReadClear,
    ReadSet,
    ReadWrite,
}

impl CsrOperation {
    pub fn writes(self, source: u8) -> bool {
        self == Self::ReadWrite || source != 0
    }
}
