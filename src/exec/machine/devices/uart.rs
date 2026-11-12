// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;
use std::collections::VecDeque;

use super::output_capture::OutputCapture;
use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

mod layout;

pub use layout::UartLayout;

#[derive(Default)]
pub struct Uart {
    control: u32,
    divisor_latch: bool,
    incoming: VecDeque<u8>,
    layout: UartLayout,
    transmitted: OutputCapture,
}

impl Uart {
    pub const RECEIVE_CAPACITY: usize = 4096;

    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drain_transmitted(&mut self, output: &mut Vec<u8>) {
        self.transmitted.drain_into(output);
    }

    pub fn inject_receive(&mut self, bytes: &[u8]) -> Result<(), EmulatorError> {
        if bytes.len() > Self::RECEIVE_CAPACITY.saturating_sub(self.incoming.len()) {
            return Err(EmulatorError::Configuration(
                "UART receive FIFO exceeds 4096 bytes".into(),
            ));
        }

        self.incoming.extend(bytes);

        Ok(())
    }

    pub fn layout(&self) -> UartLayout {
        self.layout
    }

    pub fn receive_remaining(&self) -> usize {
        Self::RECEIVE_CAPACITY - self.incoming.len()
    }

    #[cfg(test)]
    pub fn take_transmitted(&mut self) -> Vec<u8> {
        self.transmitted.take()
    }

    #[cfg(test)]
    pub fn transmitted(&self) -> &[u8] {
        self.transmitted.as_slice()
    }

    pub fn with_layout(layout: UartLayout) -> Self {
        Self {
            layout,
            ..Self::default()
        }
    }

    fn transmit(&mut self, value: u32) -> Result<(), EmulatorError> {
        if !self.transmitted.push(value as u8) {
            return Err(EmulatorError::Bus {
                address: 0,
                explanation: "UART capture buffer full; host must drain transmitted bytes".into(),
            });
        }
        Ok(())
    }
}

impl Device for Uart {
    fn drain_output(&mut self, output: &mut Vec<u8>) -> Option<&'static str> {
        self.drain_transmitted(output);
        Some("UART")
    }

    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: match self.layout {
                UartLayout::Ns16550 => &[],
                UartLayout::Simple => &[InterruptLine::MachineExternal],
            },
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
        (self.layout == UartLayout::Simple && self.control & 1 != 0 && !self.incoming.is_empty())
            .then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        if self.layout == UartLayout::Ns16550 {
            return match (offset, width) {
                (0, 4) if !self.divisor_latch => Ok(self.incoming.pop_front().unwrap_or(0) as u32),
                (0x14, 4) => Ok((u32::from(self.transmitted.has_capacity()) << 5)
                    | u32::from(!self.incoming.is_empty())),
                _ => Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "unsupported 16550 UART register read".into(),
                }),
            };
        }
        register_width(offset, width, 12)?;
        Ok(match offset {
            0 => self.incoming.pop_front().unwrap_or(0) as u32,
            4 => {
                u32::from(!self.incoming.is_empty())
                    | (u32::from(self.transmitted.has_capacity()) << 1)
            }
            8 => self.control,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        if self.layout == UartLayout::Ns16550 {
            register_width(offset, width, 0x100)?;
            match offset {
                0 if !self.divisor_latch => self.transmit(value)?,
                0x0c => self.divisor_latch = value & 0x80 != 0,
                _ => (),
            }
            return Ok(());
        }
        register_width(offset, width, 12)?;

        match offset {
            0 => self.transmit(value)?,
            8 => self.control = value & 1,
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "UART register is read-only".into(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/devices/uart/state.rs"]
mod tests;
