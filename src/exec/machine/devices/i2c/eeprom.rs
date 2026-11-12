// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::I2cPeripheral;

pub struct Eeprom {
    pub(super) bytes: [u8; 256],
    pub(super) cursor: u8,
    pub(super) next_write_is_address: bool,
}

impl Eeprom {
    pub fn new() -> Self {
        Self {
            bytes: [0xff; 256],
            next_write_is_address: true,
            cursor: 0,
        }
    }
}

impl Default for Eeprom {
    fn default() -> Self {
        Self::new()
    }
}

impl I2cPeripheral for Eeprom {
    fn read_byte(&mut self) -> u8 {
        let value = self.bytes[self.cursor as usize];
        self.cursor = self.cursor.wrapping_add(1);
        value
    }

    fn start(&mut self, read: bool) -> bool {
        if !read {
            self.next_write_is_address = true;
        }

        true
    }

    fn stop(&mut self) {
        self.next_write_is_address = true;
    }

    fn write_byte(&mut self, value: u8) -> bool {
        if self.next_write_is_address {
            self.cursor = value;
            self.next_write_is_address = false;
        } else {
            self.bytes[self.cursor as usize] = value;
            self.cursor = self.cursor.wrapping_add(1);
        }

        true
    }
}
