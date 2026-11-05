// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use super::DefaultDevices;
use crate::completion::CompletionPolicy;
use crate::timing_policy::TimingPolicy;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineSettings {
    #[serde(default)]
    pub(super) completion: Option<CompletionPolicy>,
    #[serde(default)]
    pub(super) default_devices: Option<DefaultDevices>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) timing: Option<TimingPolicy>,
}
