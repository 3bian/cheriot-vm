// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

pub trait LineTransport {
    fn read(&mut self) -> io::Result<u32>;
    fn write(&mut self, direction: u32, outputs: u32) -> io::Result<()>;
}
