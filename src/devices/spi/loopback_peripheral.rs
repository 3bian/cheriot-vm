// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

use super::SpiPeripheral;

#[derive(Default)]
pub struct LoopbackPeripheral;

impl SpiPeripheral for LoopbackPeripheral {
    fn transfer(&mut self, outgoing: u8, _mode: u8) -> io::Result<u8> {
        Ok(outgoing)
    }
}
