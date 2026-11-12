// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use crate::exec::machine::bus::Bus;
use crate::exec::machine::error::EmulatorError;
use crate::exec::machine::firmware::elf_symbol_address;

use super::HostInterface;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CompletionPolicy {
    Address(u32),
    Disabled,
    ElfSymbol,
}

impl CompletionPolicy {
    pub fn build(
        self,
        bus: &Bus,
        firmware: Option<&[u8]>,
    ) -> Result<Option<HostInterface>, EmulatorError> {
        let address = match self {
            Self::Address(address) => address,
            Self::Disabled => return Ok(None),
            Self::ElfSymbol => elf_symbol_address(
                firmware.ok_or_else(|| {
                    EmulatorError::Configuration(
                        "ELF-symbol completion requires ELF firmware".into(),
                    )
                })?,
                "tohost",
            )?,
        };
        HostInterface::on_bus(bus, address).map(Some)
    }
}
