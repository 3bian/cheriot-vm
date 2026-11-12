// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

pub trait FrameTransport {
    fn receive(&mut self, frame: &mut [u8]) -> io::Result<usize>;
    fn transmit(&mut self, frame: &[u8]) -> io::Result<usize>;
}
