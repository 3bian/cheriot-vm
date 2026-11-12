// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

#[derive(Default)]
pub struct Timer32 {
    compare: u32,
    control: u32,
    count: u32,
    pending: bool,
}

impl Timer32 {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Device for Timer32 {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: &[InterruptLine::MachineTimer],
            ..DeviceActivity::default()
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.pending && self.control & 2 != 0).then_some(InterruptLine::MachineTimer)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 16)?;
        Ok(match offset {
            0 => self.count,
            4 => self.compare,
            8 => self.control,
            12 => u32::from(self.pending),
            _ => unreachable!(),
        })
    }

    fn tick(&mut self, ticks: u64) {
        if self.control & 1 == 0 {
            return;
        }

        let distance = self.compare.wrapping_sub(self.count);
        let distance = if distance == 0 {
            1_u64 << 32
        } else {
            u64::from(distance)
        };

        if ticks >= distance {
            self.pending = true;
        }

        self.count = self.count.wrapping_add(ticks as u32);
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 16)?;

        match offset {
            0 => self.count = value,
            4 => self.compare = value,
            8 => self.control = value & 3,
            12 => {
                if value & 1 != 0 {
                    self.pending = false;
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/devices/timer32/state.rs"]
mod tests;
