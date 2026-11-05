// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

pub trait SpiPeripheral {
    fn select(&mut self, _selected: bool) -> io::Result<()> {
        Ok(())
    }

    fn transfer(&mut self, outgoing: u8, mode: u8) -> io::Result<u8>;
}
