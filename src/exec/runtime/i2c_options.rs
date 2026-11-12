// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

use super::{device_settings, options::parse_unsigned};
use crate::exec::machine::devices::i2c::validate_host_addresses;

pub(crate) struct I2cOptions {
    pub addresses: Option<Vec<u8>>,
    pub device: Option<String>,
    pub path: String,
}

impl I2cOptions {
    pub(super) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let settings = device_settings::parse(value, "--i2c", &["addresses", "device", "path"])?;
        let path = settings
            .get("path")
            .ok_or_else(|| EmulatorError::Configuration("--i2c requires path=PATH".into()))?;
        let addresses = settings
            .get("addresses")
            .map(|value| {
                let addresses = value
                    .split('+')
                    .map(|value| {
                        u8::try_from(parse_unsigned(value)?).map_err(|_| {
                            EmulatorError::Configuration("I2C address exceeds seven bits".into())
                        })
                    })
                    .collect::<Result<Vec<_>, EmulatorError>>()?;
                validate_host_addresses(&addresses)?;
                Ok::<_, EmulatorError>(addresses)
            })
            .transpose()?;
        Ok(Self {
            addresses,
            device: settings.get("device").map(|value| (*value).to_owned()),
            path: (*path).to_owned(),
        })
    }
}
