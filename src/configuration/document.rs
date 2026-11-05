// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;
use toml::Spanned;

use super::parsed_device_configuration::ParsedDeviceConfiguration;
use crate::EmulatorError;

use super::{DeviceConfiguration, MachineSettings, MemoryConfiguration, ProcessorConfiguration};
use crate::memory::Tagging;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    #[serde(default, rename = "cheriot1-memory")]
    cheriot1_memory: Vec<Spanned<MemoryConfiguration>>,
    #[serde(default)]
    ethernet: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default)]
    gpio: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default)]
    i2c: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default, rename = "interrupt-controller")]
    interrupt_controller: Vec<Spanned<ParsedDeviceConfiguration>>,
    pub(super) machine: MachineSettings,
    #[serde(default)]
    memory: Vec<Spanned<MemoryConfiguration>>,
    pub(super) processor: ProcessorConfiguration,
    #[serde(default)]
    spi: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default)]
    storage: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default, rename = "tagged-memory")]
    tagged_memory: Vec<Spanned<MemoryConfiguration>>,
    #[serde(default)]
    timer32: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default)]
    timer64: Vec<Spanned<ParsedDeviceConfiguration>>,
    #[serde(default)]
    uart: Vec<Spanned<ParsedDeviceConfiguration>>,
}

impl Document {
    pub fn take_devices(&mut self) -> Result<Vec<DeviceConfiguration>, EmulatorError> {
        let mut devices = Vec::new();

        for (kind, settings) in [
            ("ethernet", &mut self.ethernet),
            ("gpio", &mut self.gpio),
            ("i2c", &mut self.i2c),
            ("interrupt-controller", &mut self.interrupt_controller),
            ("spi", &mut self.spi),
            ("storage", &mut self.storage),
            ("timer32", &mut self.timer32),
            ("timer64", &mut self.timer64),
            ("uart", &mut self.uart),
        ] {
            for device in settings.drain(..) {
                let position = device.span().start;
                let configuration = device.into_inner().into_configuration(kind)?;
                devices.push((position, configuration));
            }
        }

        devices.sort_by_key(|(position, _)| *position);
        Ok(devices.into_iter().map(|(_, device)| device).collect())
    }

    pub fn take_memory(&mut self) -> Vec<MemoryConfiguration> {
        let mut memory = Vec::new();

        for (tagging, regions) in [
            (Tagging::None, &mut self.memory),
            (Tagging::Cheriot1, &mut self.cheriot1_memory),
            (Tagging::Cheriot1, &mut self.tagged_memory),
        ] {
            for region in regions.drain(..) {
                let position = region.span().start;
                let mut configuration = region.into_inner();
                configuration.tagging = tagging;
                memory.push((position, configuration));
            }
        }

        memory.sort_by_key(|(position, _)| *position);
        memory.into_iter().map(|(_, region)| region).collect()
    }
}
