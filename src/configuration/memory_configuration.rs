// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use super::default_executable;
use crate::memory::Tagging;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfiguration {
    pub base: u32,
    #[serde(default = "default_executable")]
    pub executable: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub size: usize,
    #[serde(skip)]
    pub tagging: Tagging,
}
