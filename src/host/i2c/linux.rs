// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::Path;

#[cfg(target_os = "linux")]
use i2cdev::core::{I2CMessage, I2CTransfer};
#[cfg(target_os = "linux")]
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CMessage};
#[cfg(target_os = "linux")]
use std::io;

use crate::devices::i2c::I2cTransport;
use crate::EmulatorError;

#[cfg(target_os = "linux")]
struct LinuxI2c {
    device: LinuxI2CDevice,
}

#[cfg(target_os = "linux")]
impl I2cTransport for LinuxI2c {
    fn transfer(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
        if !(0x08..=0x77).contains(&address) || write.len() > 4096 || read.len() > 4096 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "host I2C requires an unreserved 7-bit address and at most 4096 bytes per direction"));
        }
        self.device.set_slave_address(u16::from(address))?;
        let mut messages = Vec::new();
        if !write.is_empty() {
            messages.push(LinuxI2CMessage::write(write));
        }
        if !read.is_empty() {
            messages.push(LinuxI2CMessage::read(read));
        }
        if messages.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty I2C transfer",
            ));
        }
        let completed = self.device.transfer(&mut messages)?;
        if completed as usize != messages.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete I2C transfer",
            ));
        }
        Ok(())
    }
}

pub fn open(path: &Path) -> Result<Box<dyn I2cTransport>, EmulatorError> {
    crate::host::device_path::validate(path, "I2C")?;
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let file = crate::host::device_path::open(path, "I2C")?;
        let descriptor_path = format!("/proc/self/fd/{}", file.as_raw_fd());
        let device = LinuxI2CDevice::new(descriptor_path, 0).map_err(io::Error::from)?;
        Ok(Box::new(LinuxI2c { device }))
    }
    #[cfg(not(target_os = "linux"))]
    Err(EmulatorError::Configuration(
        "physical I2C requires Linux".into(),
    ))
}
