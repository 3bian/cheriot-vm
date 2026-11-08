// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CapabilityFaultCause {
    Bounds = 1,
    Tag = 2,
    Seal = 3,
    ExecutePermission = 17,
    LoadPermission = 18,
    StorePermission = 19,
    StoreCapabilityPermission = 21,
    SystemRegisterPermission = 24,
}
