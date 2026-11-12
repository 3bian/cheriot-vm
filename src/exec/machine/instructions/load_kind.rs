// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadKind {
    SignedByte,
    SignedHalfword,
    UnsignedByte,
    UnsignedHalfword,
    Word,
}

impl LoadKind {
    pub fn extend(self, bits: u32) -> u32 {
        match self {
            Self::SignedByte => bits as i8 as i32 as u32,
            Self::SignedHalfword => bits as i16 as i32 as u32,
            Self::UnsignedByte => bits & 0xff,
            Self::UnsignedHalfword => bits & 0xffff,
            Self::Word => bits,
        }
    }

    pub const fn width(self) -> usize {
        match self {
            Self::SignedByte | Self::UnsignedByte => 1,
            Self::SignedHalfword | Self::UnsignedHalfword => 2,
            Self::Word => 4,
        }
    }
}
