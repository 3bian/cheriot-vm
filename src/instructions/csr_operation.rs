// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CsrOperation {
    ReadClear,
    ReadSet,
    ReadWrite,
}

impl CsrOperation {
    pub(crate) fn apply(self, previous: u32, source_value: u32) -> u32 {
        match self {
            Self::ReadWrite => source_value,
            Self::ReadSet => previous | source_value,
            Self::ReadClear => previous & !source_value,
        }
    }

    pub fn writes(self, source: u8) -> bool {
        self == Self::ReadWrite || source != 0
    }
}
