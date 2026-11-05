// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Device;

pub struct DeviceMapping {
    pub base: u32,
    pub device: Box<dyn Device>,
    pub interrupt_source: Option<u8>,
    pub name: String,
    pub size: u32,
}

impl DeviceMapping {
    pub(crate) fn contains(&self, address: u32, width: usize) -> bool {
        u64::from(address) >= u64::from(self.base)
            && (width as u64) <= u64::from(self.size)
            && u64::from(address) - u64::from(self.base) <= u64::from(self.size) - width as u64
    }
}
