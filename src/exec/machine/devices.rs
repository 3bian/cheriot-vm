// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::exec::machine::bus::MemoryPort;
use crate::exec::machine::error::EmulatorError;

mod device_activity;
pub mod ethernet;
pub mod gpio;
pub mod i2c;
pub mod interrupt_controller;
mod interrupt_line;
pub(crate) mod output_capture;
pub mod revoker;
pub mod spi;
pub mod storage;
pub mod timer32;
pub mod timer64;
pub mod uart;

pub use device_activity::DeviceActivity;
pub use interrupt_line::InterruptLine;

pub trait Device: Any {
    fn drain_output(&mut self, _output: &mut Vec<u8>) -> Option<&'static str> {
        None
    }

    fn activity(&self) -> DeviceActivity {
        DeviceActivity::default()
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError>;
    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError>;
    fn tick(&mut self, _ticks: u64) {}

    fn drive_memory(&mut self, _ticks: u64, _memory: &mut MemoryPort<'_>) {}

    fn poll_host_inputs(&mut self) {}

    fn host_error(&self) -> Option<&str> {
        None
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        None
    }

    fn interrupt_taken(&mut self) {}

    fn interrupt_pending(&self, line: InterruptLine) -> bool {
        self.interrupt() == Some(line)
    }
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(crate) fn register_width(offset: u32, width: usize, bytes: u32) -> Result<(), EmulatorError> {
    if width != 4 || !offset.is_multiple_of(4) || offset >= bytes {
        return Err(EmulatorError::Bus {
            address: offset,
            explanation: "device requires an aligned 32-bit register access".into(),
        });
    }

    Ok(())
}
