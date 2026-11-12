// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

pub fn validate_host_addresses(addresses: &[u8]) -> Result<(), EmulatorError> {
    if addresses.is_empty() || addresses.len() > 112 {
        return Err(EmulatorError::Configuration(
            "I2C addresses requires 1..112 unreserved seven-bit addresses".into(),
        ));
    }
    for (index, address) in addresses.iter().enumerate() {
        if !(0x08..=0x77).contains(address) || addresses[..index].contains(address) {
            return Err(EmulatorError::Configuration(
                "I2C addresses must be unique and in 0x08..0x77".into(),
            ));
        }
    }
    Ok(())
}
