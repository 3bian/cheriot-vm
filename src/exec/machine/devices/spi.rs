// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;
use std::collections::BTreeMap;

use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

mod loopback_peripheral;
mod spi_peripheral;

pub use loopback_peripheral::LoopbackPeripheral;
pub use spi_peripheral::SpiPeripheral;

pub struct Spi {
    control: u32,
    host_error: Option<String>,
    mode: u8,
    peripherals: BTreeMap<u32, Box<dyn SpiPeripheral>>,
    response: u8,
    selected: u32,
    status: u32,
}

impl Spi {
    pub fn new() -> Self {
        Self {
            peripherals: BTreeMap::new(),
            selected: u32::MAX,
            mode: 0,
            control: 0,
            host_error: None,
            response: 0xff,
            status: 0,
        }
    }

    pub fn attach(
        &mut self,
        chip_select: u32,
        peripheral: Box<dyn SpiPeripheral>,
    ) -> Result<(), EmulatorError> {
        if chip_select >= 4 || self.peripherals.contains_key(&chip_select) {
            return Err(EmulatorError::Configuration(
                "SPI chip select must be unique and less than 4".into(),
            ));
        }

        self.peripherals.insert(chip_select, peripheral);

        Ok(())
    }
    pub fn replace_peripheral(
        &mut self,
        chip_select: u32,
        peripheral: Box<dyn SpiPeripheral>,
    ) -> Result<(), EmulatorError> {
        if chip_select >= 4 || self.selected != u32::MAX {
            return Err(EmulatorError::Configuration(
                "SPI attachment requires select 0..3 and a deselected controller".into(),
            ));
        }
        self.peripherals.insert(chip_select, peripheral);
        Ok(())
    }

    fn record_host_error<T>(&mut self, result: std::io::Result<T>) -> Result<T, EmulatorError> {
        result.map_err(|error| {
            let message = format!("SPI transport failed: {error}");
            self.host_error = Some(message.clone());
            EmulatorError::HostIo(std::io::Error::other(message))
        })
    }
}

impl Default for Spi {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Spi {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: true,
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

    fn host_error(&self) -> Option<&str> {
        self.host_error.as_deref()
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.control & 1 != 0 && self.status & 1 != 0).then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 24)?;
        Ok(match offset {
            0 => self.response as u32,
            4 => self.status,
            8 => self.selected,
            12 => self.mode as u32,
            16 => self.control,
            20 => 0,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 24)?;
        if let Some(error) = &self.host_error {
            return Err(EmulatorError::HostIo(std::io::Error::other(error.clone())));
        }

        match offset {
            0 => {
                if let Some(peripheral) = self.peripherals.get_mut(&self.selected) {
                    let response = peripheral.transfer(value as u8, self.mode);
                    self.response = self.record_host_error(response)?;
                    self.status = 1;
                } else {
                    self.response = 0xff;
                    self.status = 3;
                }
            }
            8 => {
                if value >= 4 && value != u32::MAX {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "invalid SPI chip select".into(),
                    });
                }

                if value != self.selected {
                    if let Some(peripheral) = self.peripherals.get_mut(&self.selected) {
                        let result = peripheral.select(false);
                        self.record_host_error(result)?;
                    }
                    if let Some(peripheral) = self.peripherals.get_mut(&value) {
                        let result = peripheral.select(true);
                        self.record_host_error(result)?;
                    }
                    self.selected = value;
                }
            }
            12 => self.mode = (value & 3) as u8,
            16 => self.control = value & 1,
            20 => {
                if value & 1 != 0 {
                    self.status &= !1;
                }
            }
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "SPI register is read-only".into(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/devices/spi/state.rs"]
mod tests;
