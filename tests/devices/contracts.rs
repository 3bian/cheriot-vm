// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::interrupt_controller::InterruptController;
use cheriot_vm::devices::{
    ethernet::{Ethernet, NetworkBackend},
    gpio::Gpio,
    i2c::I2c,
    spi::Spi,
    storage::Storage,
    timer32::Timer32,
    uart::Uart,
    Device,
};

#[test]
fn every_device_rejects_invalid_widths_and_offsets() {
    let mut devices: Vec<Box<dyn Device>> = vec![
        Box::new(Uart::new()),
        Box::new(Spi::new()),
        Box::new(I2c::new()),
        Box::new(Ethernet::new(NetworkBackend::Disconnected)),
        Box::new(Gpio::new()),
        Box::new(Timer32::new()),
        Box::new(Storage::new(1, false).unwrap()),
        Box::new(InterruptController::new()),
    ];

    for device in &mut devices {
        for width in [0, 1, 2, 3, 8, usize::MAX] {
            assert!(device.read(0, width).is_err());
            assert!(device.write(0, width, 0).is_err());
        }

        for offset in [1, 2, 3, 0xfc, u32::MAX] {
            assert!(device.read(offset, 4).is_err());
            assert!(device.write(offset, 4, 0).is_err());
        }
    }
}
