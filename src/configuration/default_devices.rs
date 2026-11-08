// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use crate::bus::Bus;
use crate::devices::ethernet::{Ethernet, NetworkBackend};
use crate::devices::gpio::Gpio;
use crate::EmulatorError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultDevices {
    CheriotCompatibility,
    Configured,
}

impl DefaultDevices {
    pub fn install(self, bus: &mut Bus) -> Result<(), EmulatorError> {
        if self == Self::Configured {
            return Ok(());
        }
        if !bus
            .devices()
            .iter()
            .any(|mapping| mapping.device.as_any().is::<Ethernet>())
        {
            bus.add_device(
                "ethernet0".into(),
                0x2000_0000,
                0x100,
                Box::new(Ethernet::new(NetworkBackend::InMemory)),
            )?;
        }
        if !bus
            .devices()
            .iter()
            .any(|mapping| mapping.device.as_any().is::<Gpio>())
        {
            bus.add_device("gpio0".into(), 0x1000_3000, 0x100, Box::new(Gpio::new()))?;
        }
        if bus.devices().len() > 64 {
            return Err(EmulatorError::Configuration(
                "at most 64 devices, including compatibility board defaults, are supported".into(),
            ));
        }
        Ok(())
    }
}
