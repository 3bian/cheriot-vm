// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub struct I2cConnection {
    pub addresses: Option<Vec<u8>>,
    pub device: String,
    pub path: String,
}
