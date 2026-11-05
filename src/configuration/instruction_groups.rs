// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstructionGroups {
    pub(super) compressed: Option<bool>,
    pub(super) control_registers: Option<bool>,
    pub(super) multiply_divide: Option<bool>,
}
