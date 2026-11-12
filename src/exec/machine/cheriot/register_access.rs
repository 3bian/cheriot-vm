// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Capability;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterAccess {
    Read { index: u8, value: Capability },
    Write { index: u8, value: Capability },
}
