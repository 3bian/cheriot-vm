// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::devices::{register_width, Device, DeviceActivity, InterruptLine};
use crate::error::EmulatorError;

pub struct Timer64 {
    compare_value: u64,
    software_interrupt_pending: bool,
    time: u64,
    timer_interrupt_pending: bool,
}

impl Default for Timer64 {
    fn default() -> Self {
        Self {
            compare_value: 0,
            software_interrupt_pending: false,
            time: 0,
            timer_interrupt_pending: true,
        }
    }
}

impl Timer64 {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Device for Timer64 {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: &[InterruptLine::MachineSoftware, InterruptLine::MachineTimer],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        if self.software_interrupt_pending {
            Some(InterruptLine::MachineSoftware)
        } else {
            self.timer_interrupt_pending
                .then_some(InterruptLine::MachineTimer)
        }
    }

    fn interrupt_pending(&self, line: InterruptLine) -> bool {
        match line {
            InterruptLine::MachineSoftware => self.software_interrupt_pending,
            InterruptLine::MachineTimer => self.timer_interrupt_pending,
            InterruptLine::MachineExternal => false,
        }
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 0xc0000)?;

        match offset {
            0 => Ok(u32::from(self.software_interrupt_pending)),
            0x4000 => Ok(self.compare_value as u32),
            0x4004 => Ok((self.compare_value >> 32) as u32),
            0xbff8 => Ok(self.time as u32),
            0xbffc => Ok((self.time >> 32) as u32),
            _ => Err(EmulatorError::Bus {
                address: offset,
                explanation: "unmapped timer register".into(),
            }),
        }
    }

    fn tick(&mut self, ticks: u64) {
        self.time = self.time.wrapping_add(ticks);
        self.timer_interrupt_pending = self.time >= self.compare_value;
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 0xc0000)?;

        match offset {
            0 => self.software_interrupt_pending = value & 1 != 0,
            0x4000 => {
                self.compare_value = (self.compare_value & 0xffff_ffff_0000_0000) | u64::from(value)
            }
            0x4004 => {
                self.compare_value = (self.compare_value & 0xffff_ffff) | (u64::from(value) << 32)
            }
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "unmapped or read-only timer register".into(),
                })
            }
        }

        self.timer_interrupt_pending = self.time >= self.compare_value;

        Ok(())
    }
}
