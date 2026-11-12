// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

use super::gpio::settings::GpioSettings;
use super::{
    I2cConnection, SpiConnection, device_path, ethernet::linux_tap, gpio::linux as linux_gpio,
    i2c::linux as linux_i2c, spi::linux as linux_spi,
};
use crate::exec::machine::bus::Bus;
use crate::exec::machine::devices::ethernet::{Ethernet, NetworkBackend};
use crate::exec::machine::devices::gpio::Gpio;
use crate::exec::machine::devices::i2c::{I2c, validate_host_addresses};
use crate::exec::machine::devices::spi::Spi;
use crate::exec::machine::error::EmulatorError;

#[derive(Default)]
pub struct HostConnections {
    pub gpio_chip: Option<(String, String)>,
    pub gpio_lines: Option<Vec<u32>>,
    pub gpio_settings: GpioSettings,
    pub i2c: Option<I2cConnection>,
    pub spi: Option<SpiConnection>,
    pub tap_interface: Option<(String, String)>,
}

impl HostConnections {
    pub fn connect(&self, bus: &mut Bus) -> Result<(), EmulatorError> {
        self.validate(bus)?;

        let ethernet = match &self.tap_interface {
            Some((_, interface)) => {
                let transport = linux_tap::open(interface)?;
                Some(Ethernet::new(NetworkBackend::Host(transport)))
            }
            None => None,
        };

        let gpio = match (&self.gpio_chip, &self.gpio_lines) {
            (Some((_, chip)), Some(lines)) => {
                let transport = linux_gpio::open(Path::new(chip), lines, self.gpio_settings)?;
                Some(Gpio::with_host(transport)?)
            }
            _ => None,
        };

        let spi = self
            .spi
            .as_ref()
            .map(|binding| linux_spi::open(Path::new(&binding.path), binding.speed))
            .transpose()?;
        let i2c = self
            .i2c
            .as_ref()
            .map(|binding| linux_i2c::open(Path::new(&binding.path)))
            .transpose()?;

        if let (Some((name, _)), Some(device)) = (&self.tap_interface, ethernet) {
            let target = bus.device_mut::<Ethernet>(name).ok_or_else(|| {
                EmulatorError::Configuration(format!("no Ethernet device named {name}"))
            })?;
            *target = device;
        }

        if let (Some((name, _)), Some(device)) = (&self.gpio_chip, gpio) {
            let target = bus.device_mut::<Gpio>(name).ok_or_else(|| {
                EmulatorError::Configuration(format!("no GPIO device named {name}"))
            })?;
            *target = device;
        }

        if let (Some(binding), Some(peripheral)) = (&self.spi, spi) {
            bus.device_mut::<Spi>(&binding.device)
                .ok_or_else(|| {
                    EmulatorError::Configuration(format!("no SPI device named {}", binding.device))
                })?
                .replace_peripheral(binding.select, peripheral)?;
        }
        if let (Some(binding), Some(transport)) = (&self.i2c, i2c) {
            let name = &binding.device;
            let mut device = I2c::with_host(transport);
            if let Some(addresses) = &binding.addresses {
                device.set_host_addresses(addresses.clone())?;
            }
            let controller = bus.device_mut::<I2c>(name).ok_or_else(|| {
                EmulatorError::Configuration(format!("no I2C device named {name}"))
            })?;
            *controller = device;
        }

        Ok(())
    }

    pub fn validate(&self, bus: &Bus) -> Result<(), EmulatorError> {
        if self.gpio_chip.is_some() != self.gpio_lines.is_some() {
            return Err(EmulatorError::Configuration(
                "GPIO chip and line offsets must be supplied together".into(),
            ));
        }

        if let Some((name, _)) = &self.tap_interface
            && bus.device::<Ethernet>(name).is_none()
        {
            return Err(EmulatorError::Configuration(format!(
                "no Ethernet device named {name}"
            )));
        }

        if let (Some((name, chip)), Some(lines)) = (&self.gpio_chip, &self.gpio_lines) {
            if bus.device::<Gpio>(name).is_none() {
                return Err(EmulatorError::Configuration(format!(
                    "no GPIO device named {name}"
                )));
            }

            linux_gpio::validate_lines(Path::new(chip), lines)?;
        }

        if let Some(binding) = &self.spi {
            if bus.device::<Spi>(&binding.device).is_none() {
                return Err(EmulatorError::Configuration(format!(
                    "no SPI device named {}",
                    binding.device
                )));
            }
            if binding.select >= 4 || !(1..=50_000_000).contains(&binding.speed) {
                return Err(EmulatorError::Configuration(
                    "SPI requires select 0..3 and speed 1..50000000 Hz".into(),
                ));
            }
            device_path::validate(Path::new(&binding.path), "SPI")?;
        }
        if let Some(binding) = &self.i2c {
            let name = &binding.device;
            if let Some(addresses) = &binding.addresses {
                validate_host_addresses(addresses)?;
            }
            if bus.device::<I2c>(name).is_none() {
                return Err(EmulatorError::Configuration(format!(
                    "no I2C device named {name}"
                )));
            }
            device_path::validate(Path::new(&binding.path), "I2C")?;
        }

        Ok(())
    }
}
