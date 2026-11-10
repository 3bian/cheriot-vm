// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

use super::I2cTransport;

pub(super) struct HostBus {
    address: Option<u8>,
    pub(super) addresses: Option<Vec<u8>>,
    next_read: usize,
    read: Vec<u8>,
    reading: bool,
    restart: bool,
    transport: Box<dyn I2cTransport>,
    write: Vec<u8>,
}

impl HostBus {
    pub fn new(transport: Box<dyn I2cTransport>) -> Self {
        Self {
            address: None,
            addresses: None,
            next_read: 0,
            read: Vec::new(),
            reading: false,
            restart: false,
            transport,
            write: Vec::new(),
        }
    }

    pub fn command(
        &mut self,
        command: u32,
        address: u8,
        data: u8,
        read_length: usize,
    ) -> io::Result<(u8, u32)> {
        if command == 3 {
            if !self.write.is_empty() {
                self.transport.transfer(
                    self.address.ok_or_else(|| invalid("missing I2C address"))?,
                    &self.write,
                    &mut [],
                )?;
            }
            self.address = None;
            self.next_read = 0;
            self.read.clear();
            self.reading = false;
            self.restart = false;
            self.write.clear();
            return Ok((data, 5));
        }
        if !(0x08..=0x77).contains(&address)
            || self
                .addresses
                .as_ref()
                .is_some_and(|addresses| !addresses.contains(&address))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("host I2C address 0x{address:02x} is not permitted"),
            ));
        }
        if self.address.is_some_and(|previous| previous != address) {
            return Err(invalid("host I2C requires STOP before changing address"));
        }
        match command {
            1 => {
                if self.reading || self.restart {
                    return Err(invalid("host I2C requires STOP before writing after a read"));
                }
                if self.write.len() == 4096 {
                    return Err(invalid("host I2C write exceeds 4096 bytes"));
                }
                self.write.push(data);
                self.address = Some(address);
                Ok((data, 7))
            }
            2 => {
                if !self.reading {
                    self.read.resize(read_length, 0);
                    if let Err(error) = self.transport.transfer(address, &self.write, &mut self.read) {
                        self.read.clear();
                        return Err(error);
                    }
                    self.write.clear();
                    self.address = Some(address);
                    self.reading = true;
                }
                let value = self.read.get(self.next_read).copied().ok_or_else(|| {
                    invalid("host I2C read burst exhausted; set READ_LENGTH before starting and issue STOP between bursts")
                })?;
                self.next_read += 1;
                Ok((value, 7))
            }
            4 if !self.reading => {
                self.address = Some(address);
                self.restart = true;
                Ok((data, 7))
            }
            _ => Err(invalid("host I2C supports writes, write-then-read repeated START and STOP; other sequences require STOP first")),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
