// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub struct SpiConnection {
    pub device: String,
    pub path: String,
    pub select: u32,
    pub speed: u32,
}
