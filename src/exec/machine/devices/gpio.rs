// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

mod line_transport;

pub use line_transport::LineTransport;

#[derive(Default)]
pub struct Gpio {
    direction: u32,
    host: Option<Box<dyn LineTransport>>,
    host_error: Option<String>,
    inputs: u32,
    interrupt_mask: u32,
    output_latches: u32,
    pending: u32,
}

impl Gpio {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut host: Box<dyn LineTransport>) -> Result<Self, EmulatorError> {
        host.write(0, 0)?;
        let inputs = host.read()?;

        Ok(Self {
            host: Some(host),
            inputs,
            ..Self::default()
        })
    }

    pub fn outputs(&self) -> u32 {
        self.output_latches & self.direction
    }

    pub fn set_inputs(&mut self, values: u32) {
        let changed_to_high = !self.inputs & values & !self.direction;
        self.pending |= changed_to_high;
        self.inputs = values;
    }

    fn check_host(&self) -> Result<(), EmulatorError> {
        if let Some(error) = &self.host_error {
            return Err(EmulatorError::HostIo(std::io::Error::other(error.clone())));
        }

        Ok(())
    }

    fn poll_host(&mut self) {
        if self.host_error.is_some() {
            return;
        }

        if let Some(host) = &mut self.host {
            match host.read() {
                Ok(values) => self.set_inputs(values),
                Err(error) => self.host_error = Some(format!("GPIO input failed: {error}")),
            }
        }
    }

    fn write_host(&mut self, direction: u32, outputs: u32) -> Result<(), EmulatorError> {
        if let Some(host) = &mut self.host
            && let Err(error) = host.write(direction, outputs)
        {
            self.host_error = Some(format!("GPIO output failed: {error}"));
            return self.check_host();
        }

        Ok(())
    }
}

impl Device for Gpio {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: self.host.is_some(),
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
        (self.pending & self.interrupt_mask != 0).then_some(InterruptLine::MachineExternal)
    }

    fn host_error(&self) -> Option<&str> {
        self.host_error.as_deref()
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 20)?;

        if matches!(offset, 0 | 16) {
            self.poll_host();
        }

        self.check_host()?;

        Ok(match offset {
            0 => (self.inputs & !self.direction) | self.outputs(),
            4 => self.output_latches,
            8 => self.direction,
            12 => self.interrupt_mask,
            16 => self.pending,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 20)?;
        self.check_host()?;

        match offset {
            4 => {
                self.write_host(self.direction, value)?;
                self.output_latches = value;
            }
            8 => {
                self.write_host(value, self.output_latches)?;
                self.direction = value;
            }
            12 => self.interrupt_mask = value,
            16 => self.pending &= !value,
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "GPIO register is read-only".into(),
                });
            }
        }

        Ok(())
    }

    fn tick(&mut self, _instructions: u64) {
        self.poll_host();
    }

    fn poll_host_inputs(&mut self) {
        self.poll_host();
    }
}

#[cfg(test)]
#[path = "../tests/devices/gpio/state.rs"]
mod tests;
