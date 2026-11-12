// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

#[derive(Default)]
pub struct InterruptController {
    enabled: u32,
    in_service: u32,
    latched: u32,
    levels: u32,
}

impl InterruptController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_levels(&mut self, levels: u32, edges: u32) {
        let levels = levels & !1;
        self.latched |= levels & edges;
        self.levels = levels & !edges;
    }

    fn pending(&self) -> u32 {
        (self.levels | self.latched) & !self.in_service
    }
}

impl Device for InterruptController {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: &[InterruptLine::MachineExternal],
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
        (self.pending() & self.enabled != 0).then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 16)?;
        Ok(match offset {
            0 => self.pending(),
            4 => self.enabled,
            8 => {
                let pending = self.pending() & self.enabled;

                if pending == 0 {
                    0
                } else {
                    let source = pending.trailing_zeros();
                    self.in_service |= 1 << source;
                    self.latched &= !(1 << source);
                    source
                }
            }
            12 => self.in_service,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 16)?;

        match offset {
            4 => self.enabled = value & !1,
            8 if (1..32).contains(&value) && self.in_service & (1 << value) != 0 => {
                self.in_service &= !(1 << value)
            }
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "read-only register or completion of an unclaimed interrupt"
                        .into(),
                });
            }
        }

        Ok(())
    }
}
