// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::host::uart::settings::UartSettings;
use cheriot_vm::EmulatorError;

use crate::cli::{device_settings, options::parse_unsigned};

pub(crate) struct UartOptions {
    pub device: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub settings: Option<UartSettings>,
}

impl UartOptions {
    pub(crate) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let settings = device_settings::parse(
            value,
            "--uart",
            &[
                "baud",
                "data-bits",
                "device",
                "flow",
                "in",
                "out",
                "parity",
                "stop-bits",
            ],
        )?;
        if !settings.contains_key("in") && !settings.contains_key("out") {
            return Err(EmulatorError::Configuration(
                "--uart requires in=PATH or out=PATH".into(),
            ));
        }
        let framing = if settings
            .keys()
            .any(|key| matches!(*key, "baud" | "data-bits" | "flow" | "parity" | "stop-bits"))
        {
            let baud = settings
                .get("baud")
                .map(|value| {
                    parse_unsigned(value).and_then(|value| {
                        u32::try_from(value).map_err(|_| {
                            EmulatorError::Configuration("UART baud rate exceeds 32 bits".into())
                        })
                    })
                })
                .transpose()?;
            let data_bits = settings.get("data-bits").map_or(Ok(8), |value| {
                value
                    .parse::<u8>()
                    .map_err(|_| EmulatorError::Configuration("invalid UART data-bits".into()))
            })?;
            let stop_bits = settings.get("stop-bits").map_or(Ok(1), |value| {
                value
                    .parse::<u8>()
                    .map_err(|_| EmulatorError::Configuration("invalid UART stop-bits".into()))
            })?;
            let framing = UartSettings {
                baud,
                data_bits,
                flow: settings
                    .get("flow")
                    .map(|value| value.parse())
                    .transpose()?
                    .unwrap_or_default(),
                parity: settings
                    .get("parity")
                    .map(|value| value.parse())
                    .transpose()?
                    .unwrap_or_default(),
                stop_bits,
            };
            framing.validate()?;
            Some(framing)
        } else {
            None
        };
        Ok(Self {
            device: settings.get("device").map(|value| (*value).to_owned()),
            input: settings.get("in").map(|value| (*value).to_owned()),
            output: settings.get("out").map(|value| (*value).to_owned()),
            settings: framing,
        })
    }
}
