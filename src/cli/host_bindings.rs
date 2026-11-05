// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use cheriot_vm::bus::Bus;
use cheriot_vm::devices::{ethernet::Ethernet, gpio::Gpio, i2c::I2c, spi::Spi};
use cheriot_vm::host::device_selection::resolve;
use cheriot_vm::host::{HostConnections, I2cConnection, SpiConnection};
use cheriot_vm::EmulatorError;

use crate::cli::host_options::HostOptions;
use crate::cli::uart::connections::UartConnections;

pub(crate) fn connect(
    bus: &mut Bus,
    mut options: HostOptions,
    debugger: bool,
) -> Result<UartConnections, EmulatorError> {
    let uart_options = std::mem::take(&mut options.uart);
    let connections = prepare(bus, options)?;
    let uart = UartConnections::open(bus, uart_options, debugger)?;
    for connection in connections {
        connection.connect(bus)?;
    }
    Ok(uart)
}

pub(crate) fn prepare(
    bus: &Bus,
    options: HostOptions,
) -> Result<Vec<HostConnections>, EmulatorError> {
    let mut devices = HashSet::new();
    let mut interfaces = HashSet::new();
    let mut lines = HashSet::new();
    let mut connections = Vec::new();

    for option in options.gpio {
        let device = resolve::<Gpio>(bus, option.device.as_deref(), "GPIO")?;
        if !devices.insert(device.clone()) {
            return Err(EmulatorError::Configuration(format!(
                "duplicate GPIO binding for {device}; use one --gpio per device"
            )));
        }
        for offset in &option.lines {
            if !lines.insert((option.path.clone(), *offset)) {
                return Err(EmulatorError::Configuration(
                    "GPIO line offsets must be unique across bindings on the same chip".into(),
                ));
            }
        }
        connections.push(HostConnections {
            gpio_chip: Some((device, option.path)),
            gpio_lines: Some(option.lines),
            gpio_settings: option.settings,
            ..HostConnections::default()
        });
    }

    for option in options.tap {
        let device = resolve::<Ethernet>(bus, option.device.as_deref(), "Ethernet")?;
        if !devices.insert(device.clone()) {
            return Err(EmulatorError::Configuration(format!(
                "duplicate TAP binding for {device}; use one --tap per device"
            )));
        }
        if !interfaces.insert(option.interface.clone()) {
            return Err(EmulatorError::Configuration(
                "a host TAP interface may only be bound once".into(),
            ));
        }
        connections.push(HostConnections {
            tap_interface: Some((device, option.interface)),
            ..HostConnections::default()
        });
    }

    let mut paths = HashSet::new();
    let mut selections = HashSet::new();
    for option in options.spi {
        let device = resolve::<Spi>(bus, option.device.as_deref(), "SPI")?;
        if !selections.insert((device.clone(), option.select)) || !paths.insert(option.path.clone())
        {
            return Err(EmulatorError::Configuration(
                "duplicate SPI device/select or host path".into(),
            ));
        }
        connections.push(HostConnections {
            spi: Some(SpiConnection {
                device,
                path: option.path,
                select: option.select,
                speed: option.speed,
            }),
            ..HostConnections::default()
        });
    }
    for option in options.i2c {
        let device = resolve::<I2c>(bus, option.device.as_deref(), "I2C")?;
        if !devices.insert(device.clone()) || !paths.insert(option.path.clone()) {
            return Err(EmulatorError::Configuration(
                "duplicate I2C device or host path".into(),
            ));
        }
        connections.push(HostConnections {
            i2c: Some(I2cConnection {
                addresses: option.addresses,
                device,
                path: option.path,
            }),
            ..HostConnections::default()
        });
    }

    for connection in &connections {
        connection.validate(bus)?;
    }
    Ok(connections)
}

#[cfg(test)]
#[path = "../../tests/cli/host_bindings.rs"]
mod tests;
