// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::devices::uart::UartLayout;

#[derive(Debug, Clone)]
pub struct DeviceConfiguration {
    pub attached_device: String,
    pub base: u32,
    pub host_connection: Option<String>,
    pub interrupt_source: Option<u8>,
    pub kind: String,
    pub layout: Option<UartLayout>,
    pub name: String,
    pub read_only: bool,
    pub sectors: usize,
}

impl DeviceConfiguration {
    pub(super) fn validate_applicability(
        kind: &str,
        attachment: bool,
        storage: bool,
        uart_layout: bool,
    ) -> Result<(), crate::EmulatorError> {
        if !matches!(kind, "spi" | "i2c") && attachment {
            return Err(crate::EmulatorError::Configuration(format!(
                "attached_device is unsupported for {kind}"
            )));
        }
        if kind != "storage" && storage {
            return Err(crate::EmulatorError::Configuration(format!(
                "storage parameters given for {kind}"
            )));
        }
        if kind != "uart" && uart_layout {
            return Err(crate::EmulatorError::Configuration(format!(
                "UART layout is unsupported for {kind}"
            )));
        }
        Ok(())
    }
}
