// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;
use crate::exec::machine::bus::Bus;

pub fn inspect_memory(bus: &Bus, address: u32, data: &mut [u8]) -> Result<(), EmulatorError> {
    if data.len() > 1024 || u64::from(address) + data.len() as u64 > 1_u64 << 32 {
        return Err(EmulatorError::Configuration(
            "debug memory query exceeds its range or 1024-byte budget".into(),
        ));
    }
    for (offset, byte) in data.iter_mut().enumerate() {
        *byte = bus.inspect(address + offset as u32, 1)? as u8;
    }
    Ok(())
}
