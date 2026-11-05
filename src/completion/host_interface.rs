// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::error::EmulatorError;

pub struct HostInterface {
    address: u32,
    command_written: bool,
    completed: Option<u64>,
    payload_writes: u8,
    terminal: Vec<u8>,
    value: u64,
}

impl HostInterface {
    pub fn new(address: u32) -> Self {
        Self {
            address,
            command_written: false,
            completed: None,
            payload_writes: 0,
            terminal: Vec::new(),
            value: 0,
        }
    }

    pub fn address(&self) -> u32 {
        self.address
    }

    pub fn on_bus(bus: &Bus, address: u32) -> Result<Self, EmulatorError> {
        if address & 7 != 0
            || !bus.memory().iter().any(|memory| {
                memory.kind == crate::memory::MemoryKind::Ram && memory.contains(address, 8)
            })
        {
            return Err(EmulatorError::Firmware(
                "tohost must identify an aligned RAM doubleword".into(),
            ));
        }
        Ok(Self::new(address))
    }

    pub fn drain_transmitted(&mut self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.terminal);
        self.terminal.clear();
    }

    pub fn observe_store(
        &mut self,
        bus: &mut Bus,
        address: u32,
        width: usize,
    ) -> Result<Option<u64>, EmulatorError> {
        if self.completed.is_some() {
            return Ok(None);
        }
        let high_address = self.address.checked_add(4).ok_or_else(|| {
            EmulatorError::Configuration("HTIF address exceeds the address space".into())
        })?;
        if !((address == self.address && matches!(width, 4 | 8))
            || (address == high_address && width == 4))
        {
            return Ok(None);
        }

        let low = u64::from(bus.inspect(self.address, 4)?);
        let high = u64::from(bus.inspect(high_address, 4)?);
        let value = low | high << 32;
        let same_payload = if address == self.address {
            value as u32 == self.value as u32
        } else {
            ((value ^ self.value) & 0xffff_0000_0000) == 0
        };
        self.payload_writes = if width == 8 || same_payload {
            self.payload_writes.saturating_add(1)
        } else {
            1
        };
        self.command_written |= width == 8 || address != self.address;
        self.value = value;

        if (self.command_written && self.payload_writes > 0) || self.payload_writes > 2 {
            match value >> 56 {
                0 if (value >> 48) & 255 != 0 => {
                    return Err(EmulatorError::Firmware(
                        "unsupported HTIF exit command".into(),
                    ));
                }
                0 if value & 1 != 0 => {
                    let code = (value & 0xffff_ffff_ffff) >> 1;
                    self.completed = Some(code);
                    return Ok(Some(code));
                }
                0 => (),
                1 if (value >> 48) & 255 == 1 => {
                    if self.terminal.len() >= 65536 {
                        return Err(EmulatorError::Firmware(
                            "HTIF terminal capture is full".into(),
                        ));
                    }

                    self.terminal.push(value as u8);
                    bus.write(self.address, 4, 0)?;
                    bus.write(high_address, 4, 0)?;
                    self.command_written = false;
                    self.payload_writes = 0;
                    self.value = 0;
                }
                _ => {
                    return Err(EmulatorError::Firmware(
                        "unsupported HTIF request; only exit and terminal output are available"
                            .into(),
                    ))
                }
            }
        }

        Ok(None)
    }

    pub fn take_transmitted(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.terminal)
    }
}
