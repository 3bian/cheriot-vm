// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use super::{default_sectors, no_device, DeviceConfiguration};
use crate::devices::uart::UartLayout;
use crate::EmulatorError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParsedDeviceConfiguration {
    attached_device: Option<String>,
    base: u32,
    host_connection: Option<String>,
    interrupt_source: Option<u8>,
    layout: Option<UartLayout>,
    name: String,
    read_only: Option<bool>,
    sectors: Option<usize>,
}

impl ParsedDeviceConfiguration {
    pub fn into_configuration(self, kind: &str) -> Result<DeviceConfiguration, EmulatorError> {
        DeviceConfiguration::validate_applicability(
            kind,
            self.attached_device.is_some(),
            self.sectors.is_some() || self.read_only.is_some(),
            self.layout.is_some(),
        )?;
        Ok(DeviceConfiguration {
            attached_device: self.attached_device.unwrap_or_else(no_device),
            base: self.base,
            host_connection: self.host_connection,
            interrupt_source: self.interrupt_source,
            kind: kind.into(),
            layout: self.layout,
            name: self.name,
            read_only: self.read_only.unwrap_or(false),
            sectors: self.sectors.unwrap_or_else(default_sectors),
        })
    }
}
