// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::bus::Bus;
use crate::exec::machine::cheriot::{CheriotSystem, Computer as CheriotComputer};
use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::devices::ethernet::{Ethernet, NetworkBackend};
use crate::exec::machine::devices::gpio::Gpio;
use crate::exec::machine::devices::i2c::{Eeprom, I2c};
use crate::exec::machine::devices::revoker::Revoker;
use crate::exec::machine::devices::spi::{LoopbackPeripheral, Spi};
use crate::exec::machine::devices::storage::Storage;
use crate::exec::machine::devices::timer32::Timer32;
use crate::exec::machine::devices::timer64::Timer64;
use crate::exec::machine::devices::uart::{Uart, UartLayout};
use crate::exec::machine::error::EmulatorError;
use crate::exec::machine::firmware::{load_elf, load_raw};
use crate::exec::machine::memory::{MemoryKind, MemoryRegion, Tagging};
use crate::exec::machine::processor::ProcessorProfile;
use crate::exec::machine::rv32::Processor;
use crate::exec::machine::rv32::computer::Computer;
use crate::exec::machine::timing_policy::TimingPolicy;
use document::Document;
use instruction_groups::InstructionGroups;
use machine_settings::MachineSettings;
use processor_configuration::ProcessorConfiguration;

mod default_devices;
mod device_configuration;
mod document;
mod instruction_groups;
mod machine_settings;
mod memory_configuration;
mod parsed_device_configuration;
mod processor_configuration;

pub use default_devices::DefaultDevices;
pub use device_configuration::DeviceConfiguration;
pub use memory_configuration::MemoryConfiguration;

fn default_executable() -> bool {
    true
}

fn default_sectors() -> usize {
    16
}

fn no_device() -> String {
    "none".into()
}

#[derive(Debug)]
pub struct MachineConfiguration {
    pub completion: Option<CompletionPolicy>,
    pub default_devices: DefaultDevices,
    pub devices: Vec<DeviceConfiguration>,
    pub memory: Vec<MemoryConfiguration>,
    pub name: String,
    pub processor_profile: ProcessorProfile,
    pub processor_type: String,
    pub steps_per_tick: u32,
    pub timer_hz: u32,
    pub timing: TimingPolicy,
}

fn profile_for(kind: &str) -> Result<ProcessorProfile, EmulatorError> {
    let (embedded, multiply_divide, compressed) = match kind {
        "rv32e" => (true, false, false),
        "rv32em" => (true, true, false),
        "rv32ec" => (true, false, true),
        "rv32emc" => (true, true, true),
        "rv32i" => (false, false, false),
        "rv32im" => (false, true, false),
        "rv32ic" => (false, false, true),
        "rv32imc" => (false, true, true),
        "cheriot" => (true, true, true),
        _ => {
            return Err(EmulatorError::Configuration(format!(
                "unsupported processor type: {kind}"
            )));
        }
    };
    Ok(ProcessorProfile {
        embedded,
        multiply_divide,
        compressed,
        control_registers: true,
    })
}

impl MachineConfiguration {
    pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;

    pub fn build(self, entry: u32) -> Result<Computer, EmulatorError> {
        if self.is_cheriot() {
            return Err(EmulatorError::Configuration(
                "CHERIoT requires build_cheriot or build_cheriot_raw".into(),
            ));
        }

        crate::exec::machine::timing_policy::validate_timer_hz(self.timer_hz)?;
        crate::exec::machine::timing_policy::validate_step_period(
            self.timing,
            self.steps_per_tick,
        )?;

        let profile = self.processor_profile;
        let timing = self.timing;
        let steps_per_tick = self.steps_per_tick;
        let timer_hz = self.timer_hz;
        let completion = self.completion.unwrap_or(CompletionPolicy::Disabled);
        let defaults = self.default_devices;
        let mut bus = self.build_bus(entry)?;
        defaults.install(&mut bus)?;
        let host_interface = completion.build(&bus, None)?;
        let mut computer = Computer::new(Processor::new(profile, entry), bus);
        computer.set_timer_hz(timer_hz)?;
        computer.host_interface = host_interface;
        computer.timing = timing;
        computer.set_steps_per_tick(steps_per_tick)?;

        Ok(computer)
    }

    pub fn build_elf(mut self, firmware: &[u8]) -> Result<Computer, EmulatorError> {
        let completion = self.completion.unwrap_or(CompletionPolicy::Disabled);
        self.completion = Some(CompletionPolicy::Disabled);
        let mut computer = self.build(0)?;
        let interface = completion.build(&computer.bus, Some(firmware))?;
        let entry = load_elf(&mut computer.bus, firmware, computer.processor.profile)?;
        computer.host_interface = interface;
        computer.processor.program_counter = entry;
        Ok(computer)
    }

    pub fn build_raw(self, address: u32, firmware: &[u8]) -> Result<Computer, EmulatorError> {
        let mut computer = self.build(address)?;
        crate::exec::machine::firmware::validate_raw_entry(
            &computer.bus,
            address,
            firmware,
            computer.processor.profile.minimum_alignment(),
        )?;
        load_raw(&mut computer.bus, address, firmware)?;
        Ok(computer)
    }

    pub fn build_cheriot(self, firmware: &[u8]) -> Result<CheriotSystem, EmulatorError> {
        if !self.is_cheriot() {
            return Err(EmulatorError::Configuration(
                "build_cheriot requires the cheriot processor type".into(),
            ));
        }

        crate::exec::machine::timing_policy::validate_timer_hz(self.timer_hz)?;
        crate::exec::machine::timing_policy::validate_step_period(
            self.timing,
            self.steps_per_tick,
        )?;

        let timing = self.timing;
        let steps_per_tick = self.steps_per_tick;
        let timer_hz = self.timer_hz;
        let defaults = self.default_devices;
        let completion = self.completion.unwrap_or(CompletionPolicy::ElfSymbol);
        let bus = self.build_bus(0)?;
        let mut system = CheriotSystem::from_elf_with_policy(bus, firmware, defaults, completion)?;
        system.computer.set_timer_hz(timer_hz)?;
        system.computer.timing = timing;
        system.computer.set_steps_per_tick(steps_per_tick)?;

        Ok(system)
    }

    pub fn build_cheriot_raw(
        self,
        address: u32,
        firmware: &[u8],
    ) -> Result<CheriotSystem, EmulatorError> {
        if !self.is_cheriot() {
            return Err(EmulatorError::Configuration(
                "build_cheriot_raw requires the cheriot processor type".into(),
            ));
        }

        crate::exec::machine::timing_policy::validate_timer_hz(self.timer_hz)?;
        crate::exec::machine::timing_policy::validate_step_period(
            self.timing,
            self.steps_per_tick,
        )?;

        let timing = self.timing;
        let steps_per_tick = self.steps_per_tick;
        let timer_hz = self.timer_hz;
        let defaults = self.default_devices;
        let completion = self.completion.unwrap_or(CompletionPolicy::Disabled);
        let bus = self.build_bus(address)?;
        let mut system =
            CheriotSystem::from_raw_with_policy(bus, address, firmware, defaults, completion)?;
        system.computer.set_timer_hz(timer_hz)?;
        system.computer.timing = timing;
        system.computer.set_steps_per_tick(steps_per_tick)?;

        Ok(system)
    }

    pub fn validate_profile(self) -> Result<(), EmulatorError> {
        crate::exec::machine::timing_policy::validate_timer_hz(self.timer_hz)?;
        crate::exec::machine::timing_policy::validate_step_period(
            self.timing,
            self.steps_per_tick,
        )?;
        let defaults = self.default_devices;
        let completion = self.completion;
        let mut bus = self.build_bus(0)?;
        defaults.install(&mut bus)?;
        if let Some(policy @ CompletionPolicy::Address(_)) = completion {
            policy.build(&bus, None)?;
        }
        Ok(())
    }

    pub fn is_cheriot(&self) -> bool {
        self.processor_type == "cheriot"
    }

    pub fn parse(text: &str) -> Result<Self, EmulatorError> {
        if text.len() > Self::MAX_PROFILE_BYTES {
            return Err(EmulatorError::Configuration(
                "configuration exceeds 1 MiB".into(),
            ));
        }

        let mut document: Document = toml::from_str(text)
            .map_err(|error| EmulatorError::Configuration(error.to_string()))?;
        let devices = document.take_devices()?;
        let memory = document.take_memory();
        let mut profile = profile_for(&document.processor.kind)?;
        let groups = document.processor.instruction_groups;

        if document.processor.kind == "cheriot"
            && (groups.multiply_divide.is_some()
                || groups.compressed.is_some()
                || groups.control_registers.is_some())
        {
            return Err(EmulatorError::Configuration(
                "CHERIoT instruction groups are selected together and cannot be overridden".into(),
            ));
        }

        if (profile.multiply_divide && groups.multiply_divide == Some(false))
            || (profile.compressed && groups.compressed == Some(false))
        {
            return Err(EmulatorError::Configuration("instruction groups cannot disable extensions named in the processor type; select rv32i or rv32e for individual options".into()));
        }

        if let Some(value) = groups.multiply_divide {
            profile.multiply_divide = value;
        }
        if let Some(value) = groups.compressed {
            profile.compressed = value;
        }
        if let Some(value) = groups.control_registers {
            profile.control_registers = value;
        }

        crate::exec::machine::timing_policy::validate_timer_hz(document.machine.timer_hz)?;
        crate::exec::machine::timing_policy::validate_step_period(
            document.machine.timing,
            document.machine.steps_per_tick,
        )?;
        let default_devices = if document.processor.kind == "cheriot" {
            DefaultDevices::CheriotCompatibility
        } else {
            DefaultDevices::Configured
        };
        Ok(Self {
            completion: document.machine.completion,
            default_devices,
            name: document.machine.name,
            processor_type: document.processor.kind,
            processor_profile: profile,
            memory,
            devices,
            steps_per_tick: document.machine.steps_per_tick,
            timer_hz: document.machine.timer_hz,
            timing: document.machine.timing,
        })
    }

    fn build_bus(self, entry: u32) -> Result<Bus, EmulatorError> {
        let cheriot = self.is_cheriot();
        let required = profile_for(&self.processor_type)?;

        if self.name.trim().is_empty() || self.memory.is_empty() {
            return Err(EmulatorError::Configuration(
                "machine name and memory are required".into(),
            ));
        }

        if required.embedded != self.processor_profile.embedded
            || (required.multiply_divide && !self.processor_profile.multiply_divide)
            || (required.compressed && !self.processor_profile.compressed)
            || (cheriot && !self.processor_profile.control_registers)
        {
            return Err(EmulatorError::Configuration(
                "processor profile disagrees with processor type".into(),
            ));
        }

        let total_memory = self
            .memory
            .iter()
            .try_fold(0usize, |total, region| total.checked_add(region.size));
        let total_storage = self
            .devices
            .iter()
            .filter(|device| device.kind == "storage")
            .try_fold(0usize, |total, device| {
                total.checked_add(device.sectors.checked_mul(512)?)
            });
        let total_backing = total_memory.and_then(|memory| memory.checked_add(total_storage?));

        if self.memory.len() > 64
            || self.devices.len() > 64
            || total_backing.is_none_or(|total| total > 256 * 1024 * 1024)
        {
            return Err(EmulatorError::Configuration(
                "at most 64 memory/device regions and 256 MiB total RAM/ROM/storage backing are supported".into(),
            ));
        }

        if !entry.is_multiple_of(self.processor_profile.minimum_alignment()) {
            return Err(EmulatorError::Configuration(
                "entry must be instruction-aligned".into(),
            ));
        }

        if cheriot {
            self.validate_cheriot_memory()?;
        } else if self.devices.iter().any(|device| device.kind == "revoker") {
            return Err(EmulatorError::Configuration(
                "[[revoker]] needs a CHERIoT processor: it revokes capabilities, which other processors do not have".into(),
            ));
        }

        let controller_count = self
            .devices
            .iter()
            .filter(|device| device.kind == "interrupt-controller")
            .count();
        let mut sources = std::collections::BTreeSet::new();

        if controller_count > 1 {
            return Err(EmulatorError::Configuration(
                "only one interrupt controller is supported".into(),
            ));
        }

        for device in &self.devices {
            if let Some(source) = device.interrupt_source {
                if device.kind == "timer64" || device.layout == Some(UartLayout::Ns16550) {
                    return Err(EmulatorError::Configuration(
                        "16550 UART and 64-bit timer devices cannot use interrupt_source routing"
                            .into(),
                    ));
                }

                if controller_count != 1
                    || device.kind == "interrupt-controller"
                    || !(1..32).contains(&source)
                    || !sources.insert(source)
                {
                    return Err(EmulatorError::Configuration("interrupt_source must be unique, 1..31, and routed to one configured interrupt controller".into()));
                }
            }
        }

        let mut bus = Bus::new();

        for memory in self.memory {
            if bus.memory.iter().any(|region| region.name == memory.name) {
                return Err(EmulatorError::Configuration(format!(
                    "duplicate memory region name: {}",
                    memory.name
                )));
            }

            if memory.name.is_empty() {
                return Err(EmulatorError::Configuration(
                    "memory name must not be empty".into(),
                ));
            }

            let kind = match memory.kind.as_str() {
                "ram" => MemoryKind::Ram,
                "rom" => MemoryKind::Rom,
                _ => {
                    return Err(EmulatorError::Configuration(format!(
                        "unknown memory type: {}",
                        memory.kind
                    )));
                }
            };
            let mut region =
                MemoryRegion::new(memory.name, memory.base, memory.size, kind, memory.tagging)?;
            region.executable = memory.executable;
            bus.add_memory(region)?;
        }

        for device in self.devices {
            if device.host_connection.is_some() {
                return Err(EmulatorError::Configuration(
                    "host_connection has moved out of profiles; remove it and use --tap device=DEVICE,interface=INTERFACE for TAP, or no option for in-memory Ethernet".into(),
                ));
            }

            if device.base % 4 != 0 {
                return Err(EmulatorError::Configuration(format!(
                    "device {} base must be 4-byte aligned",
                    device.name
                )));
            }

            if device.name.is_empty() {
                return Err(EmulatorError::Configuration(
                    "device name must not be empty".into(),
                ));
            }

            let instance: Box<dyn crate::exec::machine::devices::Device> = match device
                .kind
                .as_str()
            {
                "timer64" => Box::new(Timer64::new()),
                "interrupt-controller" => Box::new(
                    crate::exec::machine::devices::interrupt_controller::InterruptController::new(),
                ),
                "uart" => Box::new(Uart::with_layout(device.layout.unwrap_or_default())),
                "gpio" => Box::new(Gpio::new()),
                "timer32" => Box::new(Timer32::new()),
                "spi" => {
                    let mut spi = Spi::new();

                    match device.attached_device.as_str() {
                        "none" => {}
                        "loopback" => spi.attach(0, Box::new(LoopbackPeripheral))?,
                        _ => {
                            return Err(EmulatorError::Configuration(
                                "SPI attached_device must be none or loopback".into(),
                            ));
                        }
                    }

                    Box::new(spi)
                }
                "i2c" => {
                    let mut i2c = I2c::new();
                    match device.attached_device.as_str() {
                        "none" => {}
                        "eeprom" => i2c.attach(0x50, Box::new(Eeprom::new()))?,
                        _ => {
                            return Err(EmulatorError::Configuration(
                                "I2C attached_device must be none or eeprom".into(),
                            ));
                        }
                    }
                    Box::new(i2c)
                }
                "ethernet" => Box::new(Ethernet::new(NetworkBackend::InMemory)),
                "revoker" => Box::new(Revoker::new(
                    CheriotComputer::REVOCABLE_MEMORY_BASE,
                    CheriotComputer::REVOCATION_BITMAP_BASE,
                )),
                "storage" => Box::new(Storage::new(device.sectors, device.read_only)?),
                _ => {
                    return Err(EmulatorError::Configuration(format!(
                        "unsupported device type: {}",
                        device.kind
                    )));
                }
            };
            DeviceConfiguration::validate_applicability(
                &device.kind,
                device.attached_device != "none",
                device.sectors != 16 || device.read_only,
                device.layout.is_some(),
            )?;
            let size = if device.kind == "timer64" {
                0xc0000
            } else {
                0x100
            };
            let device_name = device.name.clone();
            bus.add_device(device.name, device.base, size, instance)?;
            bus.set_interrupt_source(&device_name, device.interrupt_source)?;
        }

        Ok(bus)
    }

    fn validate_cheriot_memory(&self) -> Result<(), EmulatorError> {
        const MEMORY_BASE: u64 = CheriotComputer::REVOCABLE_MEMORY_BASE as u64;
        const REVOCATION_BASE: u64 = CheriotComputer::REVOCATION_BITMAP_BASE as u64;

        let mut highest_address = MEMORY_BASE;
        for region in self.memory.iter().filter(|region| region.kind == "ram") {
            if region.tagging != Tagging::Cheriot || u64::from(region.base) < MEMORY_BASE {
                return Err(EmulatorError::Configuration(
                    "CHERIoT RAM must be tagged using [[cheriot-memory]] and start at or above 0x80000000".into(),
                ));
            }
            let end = u64::from(region.base)
                .checked_add(region.size as u64)
                .ok_or_else(|| EmulatorError::Configuration("RAM address overflow".into()))?;
            highest_address = highest_address.max(end);
        }

        let bitmap_size = (highest_address - MEMORY_BASE).div_ceil(64);
        let bitmap_end = REVOCATION_BASE + bitmap_size;
        let bitmap_is_mapped = self.memory.iter().any(|region| {
            region.kind == "ram"
                && u64::from(region.base) <= REVOCATION_BASE
                && u64::from(region.base).saturating_add(region.size as u64) >= bitmap_end
        });
        if !bitmap_is_mapped {
            return Err(EmulatorError::Configuration(
                "CHERIoT RAM must cover the revocation bitmap at 0x83000000".into(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/configuration/tests.rs"]
mod tests;
