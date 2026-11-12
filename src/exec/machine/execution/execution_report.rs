// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::StopReason;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionReport {
    pub attempted_steps: u64,
    pub fetched_instructions: u64,
    pub retired_instructions: u64,
    pub stop: StopReason,
    pub virtual_time: u64,
}
