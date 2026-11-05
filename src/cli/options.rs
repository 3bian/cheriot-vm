// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::completion::CompletionPolicy;
use cheriot_vm::execution::TrapPolicy;

use crate::cli::output::OutputPresentation;
use crate::cli::register_selection::RegisterSelection;

use super::{
    device_settings, gpio_options::GpioOptions, host_options::HostOptions, i2c_options::I2cOptions,
    spi_options::SpiOptions, tap_options::TapOptions, uart::options::UartOptions,
};
use cheriot_vm::EmulatorError;

pub(crate) struct Options {
    pub breakpoint: Option<u32>,
    pub checkpoint: Option<String>,
    pub completion: Option<CompletionPolicy>,
    pub firmware_is_elf: bool,
    pub firmware_path: String,
    pub gdb: Option<std::net::SocketAddr>,
    pub gdb_raw_output: bool,
    pub host: HostOptions,
    pub memory_dump: Option<(u32, usize)>,
    pub output: Option<OutputPresentation>,
    pub profile_path: String,
    pub raw_address: u32,
    pub registers: Option<RegisterSelection>,
    pub steps: Option<u64>,
    pub trace: bool,
    pub trap_policy: Option<TrapPolicy>,
}

impl Options {
    pub fn parse(arguments: &[String]) -> Result<Self, EmulatorError> {
        let mut profile_path = None;
        let mut completion = None;
        let mut output = None;
        let mut checkpoint = None;
        let mut firmware_path = None;
        let mut firmware_is_elf = false;
        let mut host = HostOptions::default();
        let mut raw_address: u32 = 0;
        let mut steps = None;
        let mut trace = false;
        let mut trap_policy = None;
        let mut registers = None;
        let mut breakpoint = None;
        let mut memory_dump = None;
        let mut index = 0;
        let mut gdb = None;
        let mut gdb_raw_output = false;

        let mut supplied = std::collections::HashSet::new();
        while index < arguments.len() {
            let flag = arguments[index].as_str();
            if !matches!(
                flag,
                "--trace" | "--uart" | "--tap" | "--gpio" | "--spi" | "--i2c"
            ) && !supplied.insert(flag)
            {
                return Err(EmulatorError::Configuration(format!(
                    "{flag} may only be supplied once"
                )));
            }
            match arguments[index].as_str() {
                "--profile" => {
                    profile_path = Some(required_argument(arguments, &mut index, "--profile")?)
                }
                "--raw" => {
                    if firmware_path.is_some() {
                        return Err(EmulatorError::Configuration(
                            "choose only one of --raw or --elf".into(),
                        ));
                    }

                    firmware_path = Some(required_argument(arguments, &mut index, "--raw")?);
                }
                "--elf" => {
                    if firmware_path.is_some() {
                        return Err(EmulatorError::Configuration(
                            "choose only one of --raw or --elf".into(),
                        ));
                    }

                    firmware_path = Some(required_argument(arguments, &mut index, "--elf")?);
                    firmware_is_elf = true;
                }
                "--address" => {
                    let text = required_argument(arguments, &mut index, "--address")?;
                    raw_address = u32::try_from(parse_unsigned(&text)?).map_err(|_| {
                        EmulatorError::Configuration("raw address exceeds 32 bits".into())
                    })?;
                }
                "--steps" => {
                    let text = required_argument(arguments, &mut index, "--steps")?;
                    steps = Some(parse_unsigned(&text)?);
                }
                "--breakpoint" => {
                    let text = required_argument(arguments, &mut index, "--breakpoint")?;
                    breakpoint = Some(u32::try_from(parse_unsigned(&text)?).map_err(|_| {
                        EmulatorError::Configuration("breakpoint address exceeds 32 bits".into())
                    })?);
                }
                "--memory" => {
                    let value = required_argument(arguments, &mut index, "--memory")?;
                    let settings =
                        device_settings::parse(&value, "--memory", &["address", "length"])?;
                    let address = settings.get("address").ok_or_else(|| {
                        EmulatorError::Configuration("--memory requires address=ADDRESS".into())
                    })?;
                    let start = u32::try_from(parse_unsigned(address)?).map_err(|_| {
                        EmulatorError::Configuration("memory address exceeds 32 bits".into())
                    })?;
                    let count = settings
                        .get("length")
                        .map_or(Ok(16), |value| parse_unsigned(value))?;
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
                    memory_dump = Some((start, count as usize));
                }
                "--uart" => {
                    let value = required_argument(arguments, &mut index, "--uart")?;
                    host.uart.push(UartOptions::parse(&value)?);
                }
                "--trap-policy" => {
                    trap_policy = Some(
                        match required_argument(arguments, &mut index, "--trap-policy")?.as_str() {
                            "stop" => TrapPolicy::Stop,
                            "vector" => TrapPolicy::Vector,
                            _ => {
                                return Err(EmulatorError::Configuration(
                                    "--trap-policy must be stop or vector".into(),
                                ))
                            }
                        },
                    );
                }
                "--tap" => {
                    let value = required_argument(arguments, &mut index, "--tap")?;
                    host.tap.push(TapOptions::parse(&value)?);
                }
                "--gpio" => {
                    let value = required_argument(arguments, &mut index, "--gpio")?;
                    host.gpio.push(GpioOptions::parse(&value)?);
                }
                "--i2c" => {
                    let value = required_argument(arguments, &mut index, "--i2c")?;
                    host.i2c.push(I2cOptions::parse(&value)?);
                }
                "--spi" => {
                    let value = required_argument(arguments, &mut index, "--spi")?;
                    host.spi.push(SpiOptions::parse(&value)?);
                }
                "--completion" => {
                    let text = required_argument(arguments, &mut index, "--completion")?;
                    let policy = match text.as_str() {
                        "disabled" => CompletionPolicy::Disabled,
                        "elf-symbol" => CompletionPolicy::ElfSymbol,
                        _ => CompletionPolicy::Address(
                            u32::try_from(parse_unsigned(&text)?).map_err(|_| {
                                EmulatorError::Configuration(
                                    "completion address exceeds 32 bits".into(),
                                )
                            })?,
                        ),
                    };
                    completion = Some(policy);
                }
                "--output" => {
                    let presentation =
                        match required_argument(arguments, &mut index, "--output")?.as_str() {
                            "escaped" => OutputPresentation::Escaped,
                            "labelled" => OutputPresentation::Labelled,
                            "raw" => OutputPresentation::Raw,
                            _ => {
                                return Err(EmulatorError::Configuration(
                                    "--output must be raw, labelled or escaped".into(),
                                ))
                            }
                        };
                    output = Some(presentation);
                }
                "--checkpoint" => {
                    checkpoint = Some(required_argument(arguments, &mut index, "--checkpoint")?);
                }
                "--gdb" => {
                    let mut ip = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
                    let mut port = 333;
                    if arguments
                        .get(index + 1)
                        .is_some_and(|value| !value.starts_with('-'))
                    {
                        let value = required_argument(arguments, &mut index, "--gdb")?;
                        let settings =
                            device_settings::parse(&value, "--gdb", &["ip", "port", "output"])?;
                        if let Some(value) = settings.get("ip") {
                            ip = value.parse().map_err(|_| {
                                EmulatorError::Configuration(
                                    "--gdb ip requires a loopback IP address".into(),
                                )
                            })?;
                        }
                        if let Some(value) = settings.get("output") {
                            gdb_raw_output = match *value {
                                "raw" => true,
                                "escaped" => false,
                                _ => {
                                    return Err(EmulatorError::Configuration(
                                        "--gdb output must be escaped or raw".into(),
                                    ))
                                }
                            };
                        }
                        if let Some(value) = settings.get("port") {
                            port = value.parse::<u16>().map_err(|_| {
                                EmulatorError::Configuration(
                                    "--gdb port requires a port from 0 to 65535".into(),
                                )
                            })?;
                        }
                    }
                    if !ip.is_loopback() {
                        return Err(EmulatorError::Configuration(
                            "--gdb ip requires a loopback IP address".into(),
                        ));
                    }
                    gdb = Some(std::net::SocketAddr::new(ip, port));
                }
                "--trace" => trace = true,
                "--registers" => {
                    let selection = if arguments
                        .get(index + 1)
                        .is_some_and(|value| !value.starts_with('-'))
                    {
                        let value = required_argument(arguments, &mut index, "--registers")?;
                        let settings = device_settings::parse(&value, "--registers", &["names"])?;
                        let name = settings.get("names").ok_or_else(|| {
                            EmulatorError::Configuration(
                                "--registers requires names=REGISTER+REGISTER".into(),
                            )
                        })?;
                        RegisterSelection::parse(name)?
                    } else {
                        RegisterSelection::All
                    };
                    registers = Some(selection);
                }
                other => {
                    return Err(EmulatorError::Configuration(format!(
                        "unknown argument: {other}"
                    )))
                }
            }

            if host.count() > 64 {
                return Err(EmulatorError::Configuration(
                    "at most 64 host bindings are supported".into(),
                ));
            }
            index += 1;
        }

        let profile_path = profile_path
            .ok_or_else(|| EmulatorError::Configuration("--profile is required".into()))?;
        let firmware_path = firmware_path
            .ok_or_else(|| EmulatorError::Configuration("--raw or --elf is required".into()))?;

        if gdb.is_some() {
            if !cfg!(all(feature = "gdb", target_os = "linux")) {
                return Err(EmulatorError::Configuration(
                    "--gdb requires a Linux build with --features gdb".into(),
                ));
            }
            if output.is_some()
                || checkpoint.is_some()
                || trace
                || registers.is_some()
                || memory_dump.is_some()
                || breakpoint.is_some()
                || trap_policy.is_some()
                || host.has_hardware()
            {
                return Err(EmulatorError::Configuration("--gdb conflicts with --output, --checkpoint, --trace, --registers, --memory, --breakpoint, --trap-policy and live TAP/GPIO/SPI/I2C; use debugger commands".into()));
            }
        }
        Ok(Self {
            checkpoint,
            completion,
            output,
            breakpoint,
            firmware_is_elf,
            firmware_path,
            gdb,
            gdb_raw_output,
            host,
            memory_dump,
            profile_path,
            raw_address,
            registers,
            steps,
            trace,
            trap_policy,
        })
    }
}

pub(super) fn parse_unsigned(value: &str) -> Result<u64, EmulatorError> {
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
            .map_err(|_| EmulatorError::Configuration(format!("invalid number: {value}")))
    } else {
        value
            .parse()
            .map_err(|_| EmulatorError::Configuration(format!("invalid number: {value}")))
    }
}

fn required_argument(
    arguments: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<String, EmulatorError> {
    *index += 1;
    arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| EmulatorError::Configuration(format!("{flag} needs a value")))
}

#[cfg(test)]
#[path = "../../tests/cli/options.rs"]
mod tests;
