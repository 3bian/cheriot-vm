// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::timing_policy::{DEFAULT_STEPS_PER_TICK, DEFAULT_TIMER_HZ, TimingPolicy};

fn default_steps_per_tick() -> u32 {
    DEFAULT_STEPS_PER_TICK
}

fn default_timer_hz() -> u32 {
    DEFAULT_TIMER_HZ
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MachineSettings {
    #[serde(default)]
    pub(super) completion: Option<CompletionPolicy>,
    pub(super) name: String,
    #[serde(default = "default_steps_per_tick")]
    pub(super) steps_per_tick: u32,
    #[serde(default)]
    pub(super) timing: TimingPolicy,
    #[serde(default = "default_timer_hz")]
    pub(super) timer_hz: u32,
}
