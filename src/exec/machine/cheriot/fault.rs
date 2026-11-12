// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::CapabilityFaultCause;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fault {
    pub cause: u32,
    pub value: u32,
}

impl Fault {
    pub fn capability(code: u32, register: u8) -> Self {
        Self {
            cause: 28,
            value: (u32::from(register) << 5) | code,
        }
    }

    pub fn capability_cause(cause: CapabilityFaultCause, register: u8) -> Self {
        Self::capability(cause as u32, register)
    }

    pub fn illegal() -> Self {
        Self { cause: 2, value: 0 }
    }
}
