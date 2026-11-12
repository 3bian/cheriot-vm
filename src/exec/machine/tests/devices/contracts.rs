// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::devices::interrupt_controller::InterruptController;
use crate::exec::machine::devices::{
    Device,
    ethernet::{Ethernet, NetworkBackend},
    gpio::Gpio,
    i2c::I2c,
    revoker::Revoker,
    spi::Spi,
    storage::Storage,
    timer32::Timer32,
    timer64::Timer64,
    uart::{Uart, UartLayout},
};

#[test]
fn every_device_rejects_invalid_widths_and_offsets() {
    let mut devices: Vec<(Box<dyn Device>, u32)> = vec![
        (Box::new(Uart::new()), 0xfc),
        (Box::new(Spi::new()), 0xfc),
        (Box::new(I2c::new()), 0xfc),
        (Box::new(Ethernet::new(NetworkBackend::Disconnected)), 0xfc),
        (Box::new(Gpio::new()), 0xfc),
        (Box::new(Timer32::new()), 0xfc),
        (Box::new(Storage::new(1, false).unwrap()), 0xfc),
        (Box::new(InterruptController::new()), 0xfc),
        (Box::new(Timer64::new()), 0xfc),
        (Box::new(Revoker::new(0x8000_0000, 0x8300_0000)), 0xfc),
        (Box::new(Uart::with_layout(UartLayout::Ns16550)), 0x100),
    ];

    for (device, unmapped_offset) in &mut devices {
        for width in [0, 1, 2, 3, 8, usize::MAX] {
            assert!(device.read(0, width).is_err());
            assert!(device.write(0, width, 0).is_err());
        }

        for offset in [1, 2, 3, *unmapped_offset, u32::MAX] {
            assert!(device.read(offset, 4).is_err());
            assert!(device.write(offset, 4, 0).is_err());
        }
    }
}
