// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorProfile {
    pub compressed: bool,
    pub control_registers: bool,
    pub embedded: bool,
    pub multiply_divide: bool,
}

impl ProcessorProfile {
    pub fn minimum_alignment(self) -> u32 {
        if self.compressed {
            2
        } else {
            4
        }
    }

    pub fn register_count(self) -> usize {
        if self.embedded {
            16
        } else {
            32
        }
    }
}
