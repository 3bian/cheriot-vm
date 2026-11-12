// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use super::InstructionGroups;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessorConfiguration {
    #[serde(default)]
    pub(super) instruction_groups: InstructionGroups,
    #[serde(rename = "type")]
    pub(super) kind: String,
}
