// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub trait I2cPeripheral {
    fn start(&mut self, _read: bool) -> bool {
        true
    }
    fn write_byte(&mut self, value: u8) -> bool;
    fn read_byte(&mut self) -> u8;
    fn stop(&mut self) {}
}
