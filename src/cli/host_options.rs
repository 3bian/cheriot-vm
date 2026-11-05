// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{
    gpio_options::GpioOptions, i2c_options::I2cOptions, spi_options::SpiOptions,
    tap_options::TapOptions, uart::options::UartOptions,
};

#[derive(Default)]
pub(crate) struct HostOptions {
    pub gpio: Vec<GpioOptions>,
    pub i2c: Vec<I2cOptions>,
    pub spi: Vec<SpiOptions>,
    pub tap: Vec<TapOptions>,
    pub uart: Vec<UartOptions>,
}

impl HostOptions {
    pub fn count(&self) -> usize {
        self.gpio.len() + self.i2c.len() + self.uart.len() + self.spi.len() + self.tap.len()
    }

    pub fn has_hardware(&self) -> bool {
        !self.gpio.is_empty()
            || !self.i2c.is_empty()
            || !self.spi.is_empty()
            || !self.tap.is_empty()
    }
}
