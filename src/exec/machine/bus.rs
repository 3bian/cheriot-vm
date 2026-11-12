// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::TypeId;
use std::cell::OnceCell;

use crate::exec::machine::cheriot::Capability;
use crate::exec::machine::devices::timer64::Timer64;
use crate::exec::machine::devices::{Device, InterruptLine};
use crate::exec::machine::error::EmulatorError;
use crate::exec::machine::memory::MemoryRegion;

mod device_indices;
mod device_mapping;
mod memory_port;

use device_indices::DeviceIndices;

pub use device_mapping::DeviceMapping;
pub use memory_port::MemoryPort;

pub struct Bus {
    device_indices: OnceCell<DeviceIndices>,
    devices: Vec<DeviceMapping>,
    pub(crate) memory: Vec<MemoryRegion>,
    pending_machine_time: Option<u64>,
    possibly_modified_timer: Option<usize>,
    devices_dirty: bool,
    interrupt_bits: u32,
    host_errors_clean: std::cell::Cell<bool>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            device_indices: OnceCell::new(),
            memory: Vec::new(),
            devices: Vec::new(),
            pending_machine_time: None,
            possibly_modified_timer: None,
            devices_dirty: true,
            interrupt_bits: 0,
            host_errors_clean: std::cell::Cell::new(false),
        }
    }

    pub fn add_device(
        &mut self,
        name: String,
        base: u32,
        size: u32,
        mut device: Box<dyn Device>,
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

        self.touch();
        self.collect_timer_write();
        let written_time = device
            .as_any_mut()
            .downcast_mut::<Timer64>()
            .and_then(Timer64::take_written_time);
        self.device_indices.take();
        self.devices.push(DeviceMapping {
            name,
            base,
            size,
            device,
            interrupt_source: None,
        });
        if let Some(time) = written_time {
            self.set_machine_time(time);
        }

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

    fn touch(&mut self) {
        self.devices_dirty = true;
        self.host_errors_clean.set(false);
    }

    #[inline]
    pub fn check_host_errors(&self) -> Result<(), EmulatorError> {
        if self.host_errors_clean.get() {
            return Ok(());
        }
        self.scan_host_errors()
    }

    fn scan_host_errors(&self) -> Result<(), EmulatorError> {
        for &index in &self.indices().host_errors {
            let mapping = &self.devices[index];
            if let Some(error) = mapping.device.host_error() {
                return Err(EmulatorError::HostIo(std::io::Error::other(format!(
                    "device {}: {error}",
                    mapping.name
                ))));
            }
        }
        self.host_errors_clean.set(true);
        Ok(())
    }

    pub fn device<T: 'static>(&self, name: &str) -> Option<&T> {
        self.devices
            .iter()
            .find(|mapping| mapping.name == name)
            .and_then(|mapping| mapping.device.as_any().downcast_ref::<T>())
    }

    pub fn device_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        let index = self
            .devices
            .iter()
            .position(|mapping| mapping.name == name)?;
        self.touch();
        self.collect_timer_write();
        if TypeId::of::<T>() == TypeId::of::<Timer64>()
            && self.devices[index].device.as_any().is::<Timer64>()
        {
            self.possibly_modified_timer = Some(index);
        }
        self.device_indices.take();
        self.devices[index].device.as_any_mut().downcast_mut::<T>()
    }

    pub fn devices(&self) -> &[DeviceMapping] {
        &self.devices
    }

    #[cfg(test)]
    pub fn drain_output(&mut self, output: &mut Vec<u8>) {
        self.touch();
        for mapping in &mut self.devices {
            mapping.device.drain_output(output);
        }
    }

    pub fn drain_output_each(
        &mut self,
        scratch: &mut Vec<u8>,
        mut consume: impl FnMut(&str, &str, &[u8]) -> Result<(), EmulatorError>,
    ) -> Result<(), EmulatorError> {
        self.touch();
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

    #[inline]
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
        self.install_firmware_segment(address, bytes, bytes.len())
    }

    pub(crate) fn install_firmware_segment(
        &mut self,
        address: u32,
        bytes: &[u8],
        memory_size: usize,
    ) -> Result<(), EmulatorError> {
        self.memory
            .iter_mut()
            .find(|region| region.contains(address, memory_size))
            .ok_or_else(|| {
                EmulatorError::Firmware(format!(
                    "firmware segment 0x{address:08x} ({} bytes) is not fully mapped",
                    memory_size
                ))
            })?
            .install_firmware_segment(address, bytes, memory_size)
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

    #[inline]
    pub(crate) fn machine_interrupt_bits(&self) -> u32 {
        if self.devices_dirty {
            self.live_interrupt_bits()
        } else {
            self.interrupt_bits
        }
    }

    fn live_interrupt_bits(&self) -> u32 {
        (u32::from(self.interrupt_pending(InterruptLine::MachineExternal)) << 11)
            | (u32::from(self.interrupt_pending(InterruptLine::MachineSoftware)) << 3)
            | (u32::from(self.interrupt_pending(InterruptLine::MachineTimer)) << 7)
    }

    pub fn memory(&self) -> &[MemoryRegion] {
        &self.memory
    }

    pub fn poll_host_inputs(&mut self) {
        self.touch();
        for mapping in &mut self.devices {
            mapping.device.poll_host_inputs();
        }
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
            self.devices_dirty = true;
            self.host_errors_clean.set(false);
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

    #[inline]
    pub fn refresh_interrupts(&mut self) {
        if self.devices_dirty {
            self.recompute_interrupts();
        }
    }

    fn recompute_interrupts(&mut self) {
        use crate::exec::machine::devices::interrupt_controller::InterruptController;
        let indices = self
            .device_indices
            .get_or_init(|| DeviceIndices::new(&self.devices));
        let mut levels = 0;

        for &(index, source) in &indices.routed_interrupts {
            let device = &mut self.devices[index].device;
            if device.interrupt().is_some() {
                levels |= 1 << source;
                if indices.edge_sources & (1 << source) != 0 {
                    device.interrupt_taken();
                }
            }
        }

        for &index in &indices.interrupt_controllers {
            if let Some(controller) = self.devices[index]
                .device
                .as_any_mut()
                .downcast_mut::<InterruptController>()
            {
                controller.set_levels(levels, indices.edge_sources);
            }
        }
        self.interrupt_bits = self.live_interrupt_bits();
        self.devices_dirty = false;
    }

    #[cfg(test)]
    pub fn remove_memory(&mut self, name: &str) -> Option<MemoryRegion> {
        let index = self.memory.iter().position(|region| region.name == name)?;
        Some(self.memory.remove(index))
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
        self.devices_dirty = true;
        Ok(())
    }

    #[cfg(test)]
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
        self.touch();
        self.collect_timer_write();
        for mapping in &mut self.devices {
            mapping.device.tick(ticks);
        }
        let indices = self
            .device_indices
            .get_or_init(|| DeviceIndices::new(&self.devices));
        for &index in &indices.bus_masters {
            self.devices[index]
                .device
                .drive_memory(ticks, &mut MemoryPort::new(&mut self.memory));
        }
        if let Some(time) = &mut self.pending_machine_time {
            *time = time.wrapping_add(ticks);
        }
    }

    pub fn write(&mut self, address: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        self.collect_timer_write();
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
            self.devices_dirty = true;
            self.host_errors_clean.set(false);
            let offset = address - mapping.base;
            mapping
                .device
                .write(offset, width, value)
                .map_err(|error| device_error(address, error))?;
            let written_time = if matches!(offset, 0xbff8 | 0xbffc) {
                mapping
                    .device
                    .as_any_mut()
                    .downcast_mut::<Timer64>()
                    .and_then(Timer64::take_written_time)
            } else {
                None
            };
            if let Some(time) = written_time {
                self.set_machine_time(time);
            }
            return Ok(());
        }

        Err(EmulatorError::Bus {
            address,
            explanation: "unmapped write".into(),
        })
    }

    #[inline]
    pub(crate) fn take_machine_time(&mut self) -> Option<u64> {
        if self.possibly_modified_timer.is_none() && self.pending_machine_time.is_none() {
            return None;
        }
        self.collect_timer_write();
        self.pending_machine_time.take()
    }

    #[inline]
    fn collect_timer_write(&mut self) {
        if let Some(index) = self.possibly_modified_timer.take() {
            let written_time = self.devices[index]
                .device
                .as_any_mut()
                .downcast_mut::<Timer64>()
                .and_then(Timer64::take_written_time);
            if let Some(time) = written_time {
                self.set_machine_time(time);
            }
        }
    }

    fn set_machine_time(&mut self, time: u64) {
        self.touch();
        for mapping in &mut self.devices {
            if let Some(timer) = mapping.device.as_any_mut().downcast_mut::<Timer64>() {
                timer.set_time(time);
            }
        }
        self.pending_machine_time = Some(time);
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
#[path = "tests/bus/tests.rs"]
mod tests;
