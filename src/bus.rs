// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::OnceCell;

use crate::cheriot1::Capability;
use crate::devices::{Device, InterruptLine};
use crate::error::EmulatorError;
use crate::memory::MemoryRegion;

mod device_indices;
mod device_mapping;

use device_indices::DeviceIndices;

pub use device_mapping::DeviceMapping;

pub struct Bus {
    device_indices: OnceCell<DeviceIndices>,
    devices: Vec<DeviceMapping>,
    pub(crate) memory: Vec<MemoryRegion>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            device_indices: OnceCell::new(),
            memory: Vec::new(),
            devices: Vec::new(),
        }
    }

    pub fn add_device(
        &mut self,
        name: String,
        base: u32,
        size: u32,
        device: Box<dyn Device>,
    ) -> Result<(), EmulatorError> {
        if self.devices.len() >= 64 {
            return Err(EmulatorError::Configuration(
                "at most 64 device regions are supported".into(),
            ));
        }
        self.check_region(base, size as u64, &name)?;

        if self.devices.iter().any(|mapping| mapping.name == name) {
            return Err(EmulatorError::Configuration(format!(
                "duplicate device name {name}"
            )));
        }

        self.device_indices.take();
        self.devices.push(DeviceMapping {
            name,
            base,
            size,
            device,
            interrupt_source: None,
        });

        Ok(())
    }

    pub fn add_memory(&mut self, region: MemoryRegion) -> Result<(), EmulatorError> {
        if region
            .tagging()
            .granule_bytes()
            .is_some_and(|granule| !(region.base as usize).is_multiple_of(granule))
        {
            return Err(EmulatorError::Configuration(
                "tagged memory base must preserve capability granule alignment".into(),
            ));
        }
        let total = self.memory.iter().map(MemoryRegion::size).sum::<usize>();
        if self.memory.len() >= 64
            || total
                .checked_add(region.size())
                .is_none_or(|size| size > 256 * 1024 * 1024)
        {
            return Err(EmulatorError::Configuration(
                "at most 64 memory regions and 256 MiB memory backing are supported".into(),
            ));
        }
        if self.memory.iter().any(|memory| memory.name == region.name) {
            return Err(EmulatorError::Configuration(format!(
                "duplicate memory name {}",
                region.name
            )));
        }
        self.check_region(region.base, region.size() as u64, &region.name)?;
        self.memory.push(region);

        Ok(())
    }

    pub fn advance_time_without_dispatch(&mut self, ticks: u64) {
        for mapping in &mut self.devices {
            mapping.device.advance_time_without_dispatch(ticks);
        }
    }

    pub fn check_host_errors(&self) -> Result<(), EmulatorError> {
        for &index in &self.indices().host_errors {
            let mapping = &self.devices[index];
            if let Some(error) = mapping.device.host_error() {
                return Err(EmulatorError::HostIo(std::io::Error::other(format!(
                    "device {}: {error}",
                    mapping.name
                ))));
            }
        }

        Ok(())
    }

    pub fn device<T: 'static>(&self, name: &str) -> Option<&T> {
        self.devices
            .iter()
            .find(|mapping| mapping.name == name)
            .and_then(|mapping| mapping.device.as_any().downcast_ref::<T>())
    }

    pub fn device_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        self.device_indices.take();
        self.devices
            .iter_mut()
            .find(|mapping| mapping.name == name)
            .and_then(|mapping| mapping.device.as_any_mut().downcast_mut::<T>())
    }

    pub fn devices(&self) -> &[DeviceMapping] {
        &self.devices
    }

    pub fn drain_output(&mut self, output: &mut Vec<u8>) {
        for mapping in &mut self.devices {
            mapping.device.drain_output(output);
        }
    }

    pub fn drain_output_each(
        &mut self,
        scratch: &mut Vec<u8>,
        mut consume: impl FnMut(&str, &str, &[u8]) -> Result<(), EmulatorError>,
    ) -> Result<(), EmulatorError> {
        for mapping in &mut self.devices {
            scratch.clear();
            let Some(kind) = mapping.device.drain_output(scratch) else {
                continue;
            };
            if !scratch.is_empty() {
                consume(kind, &mapping.name, scratch)?;
            }
        }
        Ok(())
    }

    pub fn fetch(&self, address: u32, width: usize) -> Result<u32, EmulatorError> {
        if !matches!(width, 1 | 2 | 4) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "unsupported access width".into(),
            });
        }

        self.memory
            .iter()
            .find(|region| region.executable && region.contains(address, width))
            .ok_or_else(|| EmulatorError::Bus {
                address,
                explanation: "instruction fetch outside executable memory".into(),
            })?
            .read(address, width)
    }

    pub fn inspect(&self, address: u32, width: usize) -> Result<u32, EmulatorError> {
        self.memory
            .iter()
            .find(|region| region.contains(address, width))
            .ok_or_else(|| EmulatorError::Bus {
                address,
                explanation: "inspection outside memory".into(),
            })?
            .read(address, width)
    }

    pub fn install_firmware(&mut self, address: u32, bytes: &[u8]) -> Result<(), EmulatorError> {
        self.memory
            .iter_mut()
            .find(|region| region.contains(address, bytes.len()))
            .ok_or_else(|| {
                EmulatorError::Firmware(format!(
                    "firmware segment 0x{address:08x} ({} bytes) is not fully mapped",
                    bytes.len()
                ))
            })?
            .install_firmware(address, bytes)
    }

    pub fn interrupt_pending(&self, line: InterruptLine) -> bool {
        let indices = self.indices();
        let devices = match line {
            InterruptLine::MachineExternal => &indices.external_interrupts,
            InterruptLine::MachineSoftware => &indices.software_interrupts,
            InterruptLine::MachineTimer => &indices.timer_interrupts,
        };

        devices
            .iter()
            .any(|&index| self.devices[index].device.interrupt_pending(line))
    }

    pub fn memory(&self) -> &[MemoryRegion] {
        &self.memory
    }

    pub fn read(&mut self, address: u32, width: usize) -> Result<u32, EmulatorError> {
        if !matches!(width, 1 | 2 | 4) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "unsupported access width".into(),
            });
        }

        if let Some(region) = self
            .memory
            .iter()
            .find(|region| region.contains(address, width))
        {
            return region.read(address, width);
        }

        if let Some(mapping) = self
            .devices
            .iter_mut()
            .find(|mapping| mapping.contains(address, width))
        {
            return mapping
                .device
                .read(address - mapping.base, width)
                .map_err(|error| device_error(address, error));
        }

        Err(EmulatorError::Bus {
            address,
            explanation: "unmapped read".into(),
        })
    }

    pub fn inspect_capability(&self, address: u32) -> Result<Capability, EmulatorError> {
        self.read_capability(address)
    }

    pub(crate) fn read_capability(&self, address: u32) -> Result<Capability, EmulatorError> {
        self.memory
            .iter()
            .find(|region| region.contains(address, 8))
            .ok_or_else(|| EmulatorError::Bus {
                address,
                explanation: "capability load outside memory".into(),
            })?
            .read_capability(address)
    }

    pub fn refresh_interrupts(&mut self) {
        use crate::devices::interrupt_controller::InterruptController;
        let indices = self
            .device_indices
            .get_or_init(|| DeviceIndices::new(&self.devices));
        let mut levels = 0;

        for &(index, source) in &indices.routed_interrupts {
            if self.devices[index].device.interrupt().is_some() {
                levels |= 1 << source;
            }
        }

        for &index in &indices.interrupt_controllers {
            if let Some(controller) = self.devices[index]
                .device
                .as_any_mut()
                .downcast_mut::<InterruptController>()
            {
                controller.set_levels(levels);
            }
        }
    }

    pub fn remove_device(&mut self, name: &str) -> Option<DeviceMapping> {
        let index = self
            .devices
            .iter()
            .position(|mapping| mapping.name == name)?;
        self.device_indices.take();
        Some(self.devices.remove(index))
    }

    pub fn remove_memory(&mut self, name: &str) -> Option<MemoryRegion> {
        let index = self.memory.iter().position(|region| region.name == name)?;
        Some(self.memory.remove(index))
    }

    pub fn replace_device(
        &mut self,
        name: &str,
        device: Box<dyn Device>,
    ) -> Result<Box<dyn Device>, EmulatorError> {
        let mapping = self
            .devices
            .iter_mut()
            .find(|mapping| mapping.name == name)
            .ok_or_else(|| EmulatorError::Configuration(format!("no device named {name}")))?;
        self.device_indices.take();
        Ok(std::mem::replace(&mut mapping.device, device))
    }

    pub fn set_interrupt_source(
        &mut self,
        name: &str,
        source: Option<u8>,
    ) -> Result<(), EmulatorError> {
        if source.is_some_and(|source| source == 0 || source >= 32) {
            return Err(EmulatorError::Configuration(
                "interrupt source must be from 1 to 31".into(),
            ));
        }
        let mapping = self
            .devices
            .iter_mut()
            .find(|mapping| mapping.name == name)
            .ok_or_else(|| EmulatorError::Configuration(format!("no device named {name}")))?;
        self.device_indices.take();
        mapping.interrupt_source = source;
        Ok(())
    }

    pub fn set_memory_executable(
        &mut self,
        name: &str,
        executable: bool,
    ) -> Result<(), EmulatorError> {
        let region = self
            .memory
            .iter_mut()
            .find(|region| region.name == name)
            .ok_or_else(|| EmulatorError::Configuration(format!("no memory named {name}")))?;
        region.executable = executable;
        Ok(())
    }

    pub fn tick_devices(&mut self, ticks: u64) {
        for mapping in &mut self.devices {
            mapping.device.tick(ticks);
        }
    }

    pub fn ticks_until_interrupt(&self) -> Option<u64> {
        self.devices
            .iter()
            .filter_map(|mapping| mapping.device.ticks_until_interrupt())
            .min()
    }

    pub fn write(&mut self, address: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        if !matches!(width, 1 | 2 | 4) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "unsupported access width".into(),
            });
        }

        if let Some(region) = self
            .memory
            .iter_mut()
            .find(|region| region.contains(address, width))
        {
            return region.write(address, width, value);
        }

        if let Some(mapping) = self
            .devices
            .iter_mut()
            .find(|mapping| mapping.contains(address, width))
        {
            return mapping
                .device
                .write(address - mapping.base, width, value)
                .map_err(|error| device_error(address, error));
        }

        Err(EmulatorError::Bus {
            address,
            explanation: "unmapped write".into(),
        })
    }

    pub fn write_capability(
        &mut self,
        address: u32,
        capability: Capability,
    ) -> Result<(), EmulatorError> {
        self.memory
            .iter_mut()
            .find(|region| region.contains(address, 8))
            .ok_or_else(|| EmulatorError::Bus {
                address,
                explanation: "capability store outside memory".into(),
            })?
            .write_capability(address, capability)
    }

    fn check_region(&self, base: u32, size: u64, name: &str) -> Result<(), EmulatorError> {
        if name.trim().is_empty()
            || name.len() > 4096
            || size == 0
            || u64::from(base)
                .checked_add(size)
                .is_none_or(|end| end > u32::MAX as u64 + 1)
        {
            return Err(EmulatorError::Configuration(format!(
                "invalid region {name}"
            )));
        }

        let end = base as u64 + size;
        let collides = |other_base: u32, other_size: u64| {
            (base as u64) < other_base as u64 + other_size && (other_base as u64) < end
        };
        if self
            .memory
            .iter()
            .any(|region| collides(region.base, region.size() as u64))
            || self
                .devices
                .iter()
                .any(|mapping| collides(mapping.base, mapping.size as u64))
        {
            return Err(EmulatorError::Configuration(format!(
                "address region {name} overlaps an existing region"
            )));
        }

        Ok(())
    }

    fn indices(&self) -> &DeviceIndices {
        self.device_indices
            .get_or_init(|| DeviceIndices::new(&self.devices))
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

fn device_error(address: u32, error: EmulatorError) -> EmulatorError {
    match error {
        EmulatorError::Bus { explanation, .. } => EmulatorError::Bus {
            address,
            explanation,
        },
        other => other,
    }
}

#[cfg(test)]
#[path = "../tests/bus/tests.rs"]
mod tests;
