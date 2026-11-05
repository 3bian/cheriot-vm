// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

pub trait I2cTransport {
    fn transfer(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()>;
}
