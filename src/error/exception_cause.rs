// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExceptionCause {
    Breakpoint,
    EnvironmentCall,
    IllegalInstruction,
    InstructionAccessFault,
    InstructionAddressMisaligned,
    LoadAccessFault,
    LoadAddressMisaligned,
    StoreAccessFault,
    StoreAddressMisaligned,
}

impl ExceptionCause {
    pub fn code(self) -> u32 {
        match self {
            Self::InstructionAddressMisaligned => 0,
            Self::InstructionAccessFault => 1,
            Self::IllegalInstruction => 2,
            Self::Breakpoint => 3,
            Self::LoadAddressMisaligned => 4,
            Self::LoadAccessFault => 5,
            Self::StoreAddressMisaligned => 6,
            Self::StoreAccessFault => 7,
            Self::EnvironmentCall => 11,
        }
    }
}
