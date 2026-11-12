// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::execution::TrapPolicy;

use crate::exec::runtime::output::OutputPresentation;
use crate::exec::runtime::register_selection::RegisterSelection;

use super::host_options::HostOptions;
use crate::exec::machine::EmulatorError;

pub(crate) struct Options {
    pub breakpoint: Vec<u32>,
    pub checkpoint: Option<String>,
    pub completion: Option<CompletionPolicy>,
    pub firmware_is_elf: bool,
    pub firmware_path: String,
    pub gdb: Option<std::net::SocketAddr>,
    pub gdb_client: Option<std::path::PathBuf>,
    pub gdb_raw_output: bool,
    pub host: HostOptions,
    pub memory_dump: Option<(u32, usize)>,
    pub output: Option<OutputPresentation>,
    pub profile_path: String,
    pub profile_text: Option<String>,
    pub stats: Option<Option<std::path::PathBuf>>,
    pub raw_address: u32,
    pub registers: Option<RegisterSelection>,
    pub steps: Option<u64>,
    pub trace: bool,
    pub trap_policy: Option<TrapPolicy>,
}

pub(super) fn parse_u32(value: &str, overflow_message: &str) -> Result<u32, EmulatorError> {
    u32::try_from(parse_unsigned(value)?)
        .map_err(|_| EmulatorError::Configuration(overflow_message.into()))
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
