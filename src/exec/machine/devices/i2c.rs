// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;
use std::collections::BTreeMap;

use crate::exec::machine::devices::{Device, DeviceActivity, InterruptLine, register_width};
use crate::exec::machine::error::EmulatorError;

mod address_filter;
mod eeprom;
mod host_bus;
mod i2c_peripheral;
mod transport;

pub use address_filter::validate_host_addresses;
pub use eeprom::Eeprom;
pub use i2c_peripheral::I2cPeripheral;
pub use transport::I2cTransport;

pub struct I2c {
    active: bool,
    active_address: Option<u8>,
    address: u8,
    control: u32,
    data: u8,
    host: Option<host_bus::HostBus>,
    host_error: Option<String>,
    peripherals: BTreeMap<u8, Box<dyn I2cPeripheral>>,
    reading: bool,
    restart: bool,
    status: u32,
}

impl I2c {
    pub fn new() -> Self {
        Self {
            peripherals: BTreeMap::new(),
            address: 0,
            data: 0,
            host: None,
            host_error: None,
            status: 0,
            control: 0,
            active: false,
            active_address: None,
            reading: false,
            restart: false,
        }
    }

    pub fn attach(
        &mut self,
        address: u8,
        peripheral: Box<dyn I2cPeripheral>,
    ) -> Result<(), EmulatorError> {
        if address > 0x7f || self.peripherals.contains_key(&address) {
            return Err(EmulatorError::Configuration(
                "I2C address must be a unique 7-bit address".into(),
            ));
        }

        self.peripherals.insert(address, peripheral);

        Ok(())
    }
    pub fn set_host_addresses(&mut self, addresses: Vec<u8>) -> Result<(), EmulatorError> {
        validate_host_addresses(&addresses)?;
        if self.active {
            return Err(EmulatorError::Configuration(
                "I2C address policy cannot change during a transaction".into(),
            ));
        }
        let host = self.host.as_mut().ok_or_else(|| {
            EmulatorError::Configuration("I2C host address policy requires a host transport".into())
        })?;
        host.addresses = Some(addresses);
        Ok(())
    }

    pub fn with_host(transport: Box<dyn I2cTransport>) -> Self {
        Self {
            host: Some(host_bus::HostBus::new(transport)),
            ..Self::new()
        }
    }
}

impl Default for I2c {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for I2c {
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

    fn host_error(&self) -> Option<&str> {
        self.host_error.as_deref()
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.control & 1 != 0 && self.status & 4 != 0).then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 24)?;
        Ok(match offset {
            0 => self.address as u32,
            4 => self.data as u32,
            8 => 0,
            12 => self.status,
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
                if value > 0x7f {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "I2C address exceeds seven bits".into(),
                    });
                }

                self.address = value as u8;
            }
            4 => self.data = value as u8,
            8 => {
                if !(1..=4).contains(&value) {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "invalid I2C command".into(),
                    });
                }

                if let Some(host) = &mut self.host {
                    match host.command(
                        value,
                        self.address,
                        self.data,
                        ((self.control >> 16) + 1) as usize,
                    ) {
                        Ok((data, status)) => {
                            self.data = data;
                            self.status = status;
                            self.active = status & 2 != 0;
                            return Ok(());
                        }
                        Err(error) => {
                            let message = format!("I2C transport failed: {error}");
                            self.host_error = Some(message.clone());
                            return Err(EmulatorError::HostIo(std::io::Error::other(message)));
                        }
                    }
                }
                self.status = 4;

                if value == 1 || value == 2 {
                    let reading = value == 2;

                    if self.restart
                        || self.active_address != Some(self.address)
                        || self.reading != reading
                    {
                        let acknowledged = self
                            .peripherals
                            .get_mut(&self.address)
                            .is_some_and(|peripheral| peripheral.start(reading));
                        self.active_address = Some(self.address);
                        self.active = true;
                        self.reading = reading;
                        self.restart = false;

                        if !acknowledged {
                            self.active_address = None;
                            self.status |= 8 | 2;
                            return Ok(());
                        }
                    }
                }

                match value {
                    1 => {
                        self.active = true;
                        let acknowledged = self
                            .peripherals
                            .get_mut(&self.address)
                            .is_some_and(|peripheral| peripheral.write_byte(self.data));
                        self.status |= if acknowledged { 1 } else { 8 };
                    }
                    2 => {
                        self.active = true;

                        if let Some(peripheral) = self.peripherals.get_mut(&self.address) {
                            self.data = peripheral.read_byte();
                            self.status |= 1;
                        } else {
                            self.status |= 8;
                        }
                    }
                    3 => {
                        if let Some(address) = self.active_address.take()
                            && let Some(peripheral) = self.peripherals.get_mut(&address)
                        {
                            peripheral.stop();
                        }

                        self.restart = false;
                        self.active = false;
                        self.status |= 1;
                    }
                    4 => {
                        self.active = true;
                        self.restart = true;
                        self.status |= if self.peripherals.contains_key(&self.address) {
                            1
                        } else {
                            8
                        };
                    }
                    _ => {
                        return Err(EmulatorError::Bus {
                            address: offset,
                            explanation: "invalid I2C command".into(),
                        });
                    }
                }

                if self.active {
                    self.status |= 2;
                }
            }
            16 => self.control = value & 0x0fff_0001,
            20 => self.status &= !(value & 4),
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "I2C status is read-only".into(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/devices/i2c/state.rs"]
mod tests;
