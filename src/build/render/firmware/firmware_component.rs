// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::PathBuf;

use crate::config::configuration::SdkComponentKind;

pub struct FirmwareComponent {
    pub name: String,
    pub kind: SdkComponentKind,
    pub object: PathBuf,
}
