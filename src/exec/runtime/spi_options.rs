// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

use super::{device_settings, options::parse_unsigned};

pub(crate) struct SpiOptions {
    pub device: Option<String>,
    pub path: String,
    pub select: u32,
    pub speed: u32,
}

impl SpiOptions {
    pub(super) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let settings =
            device_settings::parse(value, "--spi", &["device", "path", "select", "speed"])?;
        let path = settings
            .get("path")
            .ok_or_else(|| EmulatorError::Configuration("--spi requires path=PATH".into()))?;
        let select = settings
            .get("select")
            .map_or(Ok(0), |value| parse_unsigned(value))?;
        let speed = settings
            .get("speed")
            .map_or(Ok(500_000), |value| parse_unsigned(value))?;
        if select >= 4 || !(1..=50_000_000).contains(&speed) {
            return Err(EmulatorError::Configuration(
                "--spi requires select=0..3 and speed=1..50000000 Hz".into(),
            ));
        }
        Ok(Self {
            device: settings.get("device").map(|value| (*value).to_owned()),
            path: (*path).to_owned(),
            select: select as u32,
            speed: speed as u32,
        })
    }
}
