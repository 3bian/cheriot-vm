// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::exec::machine::bus::MemoryPort;
use crate::exec::machine::cheriot::Capability;
use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

pub struct Revoker {
    base: u32,
    bitmap_base: u32,
    completed: u32,
    cursor: Option<u32>,
    interrupt_requested: bool,
    memory_base: u32,
    status: bool,
    top: u32,
}

impl Revoker {
    pub const IDENTITY: u32 = 0x5500_0000;

    pub const GRANULES_PER_TICK: u64 = 64;

    const GRANULES_PER_CALL: u64 = 1 << 20;

    const REGISTER_BYTES: u32 = 24;

    pub fn new(memory_base: u32, bitmap_base: u32) -> Self {
        Self {
            base: 0,
            bitmap_base,
            completed: 0,
            cursor: None,
            interrupt_requested: false,
            memory_base,
            status: false,
            top: 0,
        }
    }

    fn epoch(&self) -> u32 {
        (self.completed << 1) | u32::from(self.cursor.is_some())
    }

    fn is_revoked(&self, capability: Capability, memory: &MemoryPort<'_>) -> bool {
        let Some((offset, bit)) = capability.revocation_bit(self.memory_base) else {
            return false;
        };
        self.bitmap_base
            .checked_add(offset)
            .and_then(|address| memory.read_byte(address))
            .is_some_and(|byte| byte & bit != 0)
    }
}

impl Device for Revoker {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            bus_master: true,
            edge_triggered: true,
            host_errors: false,
            interrupt_lines: &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn drive_memory(&mut self, ticks: u64, memory: &mut MemoryPort<'_>) {
        let Some(mut cursor) = self.cursor else {
            return;
        };
        let last = self.top / MemoryPort::GRANULE_BYTES;
        let mut budget = ticks
            .saturating_mul(Self::GRANULES_PER_TICK)
            .min(Self::GRANULES_PER_CALL);
        while budget != 0 && cursor <= last {
            let address = cursor * MemoryPort::GRANULE_BYTES;
            if let Some(capability) = memory.read_granule(address)
                && capability.tag()
                && self.is_revoked(capability, memory)
            {
                memory.clear_tag(address);
            }
            cursor += 1;
            budget -= 1;
        }
        if cursor > last {
            self.cursor = None;
            self.completed = self.completed.wrapping_add(1) & (u32::MAX >> 1);
            self.status = true;
        } else {
            self.cursor = Some(cursor);
        }
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.status && self.interrupt_requested).then_some(InterruptLine::MachineExternal)
    }

    fn interrupt_taken(&mut self) {
        self.status = false;
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, Self::REGISTER_BYTES)?;
        Ok(match offset {
            0x00 => self.base,
            0x04 => self.top,
            0x08 => Self::IDENTITY,
            0x0c => self.epoch(),
            0x10 => u32::from(self.status && !self.interrupt_requested),
            0x14 => u32::from(self.interrupt_requested),
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, Self::REGISTER_BYTES)?;
        match offset {
            0x00 => self.base = value,
            0x04 => self.top = value,
            0x08 => {
                if self.cursor.is_none() {
                    self.cursor = Some(self.base / MemoryPort::GRANULE_BYTES);
                }
            }
            0x0c => {}
            0x10 => self.status = false,
            0x14 => self.interrupt_requested = value & 1 != 0,
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/devices/revoker/state.rs"]
mod tests;
