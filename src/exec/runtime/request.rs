// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{
    device_settings,
    gpio_options::GpioOptions,
    host_options::HostOptions,
    i2c_options::I2cOptions,
    options::{Options, parse_u32, parse_unsigned},
    output::OutputPresentation,
    register_selection::RegisterSelection,
    spi_options::SpiOptions,
    tap_options::TapOptions,
    uart::options::UartOptions,
};
use crate::exec::machine::EmulatorError;
use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::execution::TrapPolicy;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug)]
pub enum Firmware {
    Elf(PathBuf),
    Raw { path: PathBuf, address: u32 },
}
#[derive(Debug, Default)]
pub struct PeripheralSettings {
    pub gpio: Vec<String>,
    pub i2c: Vec<String>,
    pub spi: Vec<String>,
    pub tap: Vec<String>,
    pub uart: Vec<String>,
}
#[derive(Debug)]
pub struct MachineRequest {
    pub firmware: Firmware,
    pub profile: PathBuf,
    pub profile_text: Option<String>,
    pub stats: Option<Option<PathBuf>>,
    pub breakpoints: Vec<u32>,
    pub checkpoint: Option<PathBuf>,
    pub completion: Option<CompletionPolicy>,
    pub peripherals: PeripheralSettings,
    pub memory: Option<String>,
    pub output: Option<Presentation>,
    pub registers: Option<RegisterDump>,
    pub steps: Option<u64>,
    pub trace: bool,
    pub trap_policy: Option<TrapPolicy>,
    pub debugger: Option<SocketAddr>,
    pub debugger_client: Option<PathBuf>,
    pub debugger_raw_output: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum Presentation {
    Raw,
    Labelled,
    Escaped,
}
#[derive(Debug)]
pub enum RegisterDump {
    Selection(String),
}
impl MachineRequest {
    pub fn new(firmware: Firmware, profile: PathBuf) -> Self {
        Self {
            firmware,
            profile,
            profile_text: None,
            stats: None,
            breakpoints: vec![],
            checkpoint: None,
            completion: None,
            peripherals: PeripheralSettings::default(),
            memory: None,
            output: None,
            registers: None,
            steps: None,
            trace: false,
            trap_policy: None,
            debugger: None,
            debugger_client: None,
            debugger_raw_output: false,
        }
    }
    pub(super) fn validate(self) -> Result<Options, EmulatorError> {
        let path = |p: PathBuf| {
            p.into_os_string()
                .into_string()
                .map_err(|_| EmulatorError::Configuration("paths must be UTF-8".into()))
        };
        let (firmware_path, firmware_is_elf, raw_address) = match self.firmware {
            Firmware::Elf(p) => (path(p)?, true, 0),
            Firmware::Raw { path: p, address } => (path(p)?, false, address),
        };
        let mut host = HostOptions::default();
        for v in self.peripherals.gpio {
            host.gpio.push(GpioOptions::parse(&v)?);
        }
        for v in self.peripherals.i2c {
            host.i2c.push(I2cOptions::parse(&v)?);
        }
        for v in self.peripherals.spi {
            host.spi.push(SpiOptions::parse(&v)?);
        }
        for v in self.peripherals.tap {
            host.tap.push(TapOptions::parse(&v)?);
        }
        for v in self.peripherals.uart {
            host.uart.push(UartOptions::parse(&v)?);
        }
        if host.count() > 64 {
            return Err(EmulatorError::Configuration(
                "at most 64 host bindings are supported".into(),
            ));
        }
        let memory_dump = self
            .memory
            .map(|v| {
                let settings = device_settings::parse(&v, "--memory", &["address", "length"])?;
                let address = settings.get("address").ok_or_else(|| {
                    EmulatorError::Configuration("--memory requires address=ADDRESS".into())
                })?;
                let start = parse_u32(address, "memory address exceeds 32 bits")?;
                let count = settings
                    .get("length")
                    .map_or(Ok(16), |v| parse_unsigned(v))?;
                if !(1..=256).contains(&count) {
                    return Err(EmulatorError::Configuration(
                        "memory dump length must be 1..256".into(),
                    ));
                }
                if u64::from(start) + count > 1_u64 << 32 {
                    return Err(EmulatorError::Configuration(
                        "memory dump address overflow".into(),
                    ));
                }
                Ok((start, count as usize))
            })
            .transpose()?;
        let registers = self
            .registers
            .map(|v| {
                let RegisterDump::Selection(v) = v;
                let name = v.strip_prefix("names=").unwrap_or(&v);
                RegisterSelection::parse(name)
            })
            .transpose()?;
        let output = self.output.map(|p| match p {
            Presentation::Raw => OutputPresentation::Raw,
            Presentation::Labelled => OutputPresentation::Labelled,
            Presentation::Escaped => OutputPresentation::Escaped,
        });
        let checkpoint = self.checkpoint.map(path).transpose()?;
        if let Some(address) = self.debugger {
            if !address.ip().is_loopback() {
                return Err(EmulatorError::Configuration(
                    "--host must be a loopback IP address".into(),
                ));
            }
            if !cfg!(all(feature = "gdb", unix)) {
                return Err(EmulatorError::Configuration(
                    "debug is not part of this build: it needs the gdb feature and a Unix system"
                        .into(),
                ));
            }
            if output.is_some()
                || checkpoint.is_some()
                || self.trace
                || registers.is_some()
                || memory_dump.is_some()
                || !self.breakpoints.is_empty()
                || self.trap_policy.is_some()
                || host.has_hardware()
            {
                return Err(EmulatorError::Configuration("debug does not take --console-format, --checkpoint, --trace, --registers, --memory, --breakpoint, --trap-policy or live TAP/GPIO/SPI/I2C; use debugger commands instead".into()));
            }
        }
        Ok(Options {
            breakpoint: self.breakpoints,
            checkpoint,
            completion: self.completion,
            firmware_is_elf,
            firmware_path,
            gdb: self.debugger,
            gdb_client: self.debugger_client,
            gdb_raw_output: self.debugger_raw_output,
            host,
            memory_dump,
            output,
            profile_path: path(self.profile)?,
            profile_text: self.profile_text,
            stats: self.stats,
            raw_address,
            registers,
            steps: self.steps,
            trace: self.trace,
            trap_policy: self.trap_policy,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub enum AddressKind {
    Raw,
    Breakpoint,
    Completion,
}

pub fn address(value: &str, kind: AddressKind) -> Result<u32, EmulatorError> {
    let message = match kind {
        AddressKind::Raw => "raw address exceeds 32 bits",
        AddressKind::Breakpoint => "breakpoint address exceeds 32 bits",
        AddressKind::Completion => "completion address exceeds 32 bits",
    };
    parse_u32(value, message)
}

#[cfg(test)]
#[path = "../machine/tests/cli/request.rs"]
mod tests;
