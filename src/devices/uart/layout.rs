// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub enum UartLayout {
    #[serde(rename = "16550")]
    Ns16550,
    #[default]
    #[serde(rename = "simple")]
    Simple,
}
