// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::host::gpio::settings::GpioSettings;
use cheriot_vm::EmulatorError;

use super::{device_settings, options::parse_unsigned};

pub(crate) struct GpioOptions {
    pub device: Option<String>,
    pub lines: Vec<u32>,
    pub path: String,
    pub settings: GpioSettings,
}

impl GpioOptions {
    pub(super) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let settings = device_settings::parse(
            value,
            "--gpio",
            &["active-low", "bias", "device", "drive", "lines", "path"],
        )?;
        let (Some(path), Some(lines)) = (settings.get("path"), settings.get("lines")) else {
            return Err(EmulatorError::Configuration(
                "--gpio requires path=PATH and lines=OFFSET+OFFSET".into(),
            ));
        };
        let lines = lines
            .split('+')
            .map(|value| {
                u32::try_from(parse_unsigned(value)?).map_err(|_| {
                    EmulatorError::Configuration("GPIO line offset exceeds 32 bits".into())
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            device: settings.get("device").map(|value| (*value).to_owned()),
            lines,
            path: (*path).to_owned(),
            settings: GpioSettings {
                active_low: settings
                    .get("active-low")
                    .map(|value| value.parse::<bool>())
                    .transpose()
                    .map_err(|_| {
                        EmulatorError::Configuration(
                            "GPIO active-low requires true or false".into(),
                        )
                    })?
                    .unwrap_or(false),
                bias: match settings.get("bias") {
                    None | Some(&"as-is") => None,
                    Some(value) => Some(value.parse()?),
                },
                drive: settings
                    .get("drive")
                    .map(|value| value.parse())
                    .transpose()?
                    .unwrap_or_default(),
            },
        })
    }
}
