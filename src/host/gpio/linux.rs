// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[cfg(target_os = "linux")]
use std::io;
use std::path::Path;

#[cfg(target_os = "linux")]
use gpiocdev::line::{Value, Values};
#[cfg(target_os = "linux")]
use gpiocdev::Request;

use super::settings::GpioSettings;
use crate::devices::gpio::LineTransport;
use crate::error::EmulatorError;

#[cfg(target_os = "linux")]
struct LinuxGpio {
    direction: u32,
    offsets: Vec<u32>,
    request: Request,
    settings: GpioSettings,
}

#[cfg(target_os = "linux")]
impl LinuxGpio {
    fn new(chip: &Path, offsets: &[u32], settings: GpioSettings) -> io::Result<Self> {
        let mut configuration = gpiocdev::request::Config::default();
        configuration.as_input().with_lines(offsets);
        settings.configure(&mut configuration);
        let request = Request::builder()
            .on_chip(chip)
            .with_consumer("cheriot-vm")
            .with_config(configuration)
            .request()
            .map_err(io::Error::other)?;

        Ok(Self {
            direction: 0,
            offsets: offsets.to_vec(),
            request,
            settings,
        })
    }
}

#[cfg(target_os = "linux")]
impl LineTransport for LinuxGpio {
    fn read(&mut self) -> io::Result<u32> {
        let mut values = Values::from_offsets(&self.offsets);
        self.request.values(&mut values).map_err(io::Error::other)?;

        let mut inputs = 0;

        for (index, offset) in self.offsets.iter().enumerate() {
            if values.get(*offset) == Some(Value::Active) {
                inputs |= 1 << index;
            }
        }

        Ok(inputs)
    }

    fn write(&mut self, direction: u32, outputs: u32) -> io::Result<()> {
        let mut values = Values::default();

        for (index, offset) in self.offsets.iter().enumerate() {
            if direction & (1 << index) != 0 {
                values.set(*offset, Value::from(outputs & (1 << index) != 0));
            }
        }

        if direction != self.direction {
            let mut configuration = self.request.config();

            for (index, offset) in self.offsets.iter().enumerate() {
                configuration.with_line(*offset);

                if direction & (1 << index) == 0 {
                    configuration.as_input();
                } else {
                    configuration
                        .as_output(Value::from(outputs & (1 << index) != 0))
                        .with_drive(self.settings.drive());
                }
            }

            self.request
                .reconfigure(&configuration)
                .map_err(io::Error::other)?;
            self.direction = direction;
        } else if !values.is_empty() {
            self.request.set_values(&values).map_err(io::Error::other)?;
        }

        Ok(())
    }
}

pub fn open(
    chip: &Path,
    offsets: &[u32],
    settings: GpioSettings,
) -> Result<Box<dyn LineTransport>, EmulatorError> {
    validate_lines(chip, offsets)?;

    #[cfg(target_os = "linux")]
    {
        let gpio = LinuxGpio::new(chip, offsets, settings).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot open GPIO chip {}: {error}", chip.display()),
            )
        })?;

        Ok(Box::new(gpio))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = settings;
        Err(EmulatorError::Configuration(
            "physical GPIO requires Linux with the GPIO character device v2 interface".into(),
        ))
    }
}

pub fn validate_lines(chip: &Path, offsets: &[u32]) -> Result<(), EmulatorError> {
    if !chip.is_absolute() || offsets.is_empty() || offsets.len() > 32 {
        return Err(EmulatorError::Configuration(
            "GPIO requires an absolute chip path and 1..32 line offsets".into(),
        ));
    }

    for (index, offset) in offsets.iter().enumerate() {
        if offsets[..index].contains(offset) {
            return Err(EmulatorError::Configuration(
                "GPIO line offsets must be unique".into(),
            ));
        }
    }

    Ok(())
}
