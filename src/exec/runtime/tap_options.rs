// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

use super::device_settings;

pub(crate) struct TapOptions {
    pub device: Option<String>,
    pub interface: String,
}

impl TapOptions {
    pub(super) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let settings = device_settings::parse(value, "--tap", &["device", "interface"])?;
        let interface = settings
            .get("interface")
            .ok_or_else(|| EmulatorError::Configuration("--tap requires interface=NAME".into()))?;
        Ok(Self {
            device: settings.get("device").map(|value| (*value).to_owned()),
            interface: (*interface).to_owned(),
        })
    }
}
