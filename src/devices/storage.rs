// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;

use crate::devices::{register_width, Device, DeviceActivity, InterruptLine};
use crate::error::EmulatorError;

const SECTOR_BYTES: usize = 512;

pub struct Storage {
    buffer: [u8; SECTOR_BYTES],
    control: u32,
    media: Option<Vec<u8>>,
    read_only: bool,
    sector: u32,
    status: u32,
    word_index: usize,
}

impl Storage {
    pub fn new(sectors: usize, read_only: bool) -> Result<Self, EmulatorError> {
        if sectors == 0 || sectors > 65_536 {
            return Err(EmulatorError::Configuration(
                "storage size must be 1..65536 sectors".into(),
            ));
        }

        Ok(Self {
            media: Some(vec![0; sectors * SECTOR_BYTES]),
            read_only,
            sector: 0,
            word_index: 0,
            buffer: [0; SECTOR_BYTES],
            status: 4,
            control: 0,
        })
    }

    pub fn eject(&mut self) {
        self.media = None;
        self.status = 0;
    }

    pub fn from_image(image: &[u8], read_only: bool) -> Result<Self, EmulatorError> {
        if !image.len().is_multiple_of(SECTOR_BYTES) {
            return Err(EmulatorError::Configuration(
                "storage image length must be a multiple of 512 bytes".into(),
            ));
        }

        let mut storage = Self::new(image.len() / SECTOR_BYTES, read_only)?;

        if let Some(media) = &mut storage.media {
            media.copy_from_slice(image);
        }

        Ok(storage)
    }

    pub fn media_bytes(&self) -> Option<&[u8]> {
        self.media.as_deref()
    }
}

impl Device for Storage {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: &[InterruptLine::MachineExternal],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.control & 1 != 0 && self.status & 3 != 0).then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 24)?;
        Ok(match offset {
            0 => self.sector,
            4 => self.word_index as u32,
            8 => u32::from_le_bytes(
                self.buffer[self.word_index * 4..self.word_index * 4 + 4]
                    .try_into()
                    .map_err(|_| {
                        EmulatorError::Configuration("invalid storage word index".into())
                    })?,
            ),
            12 => 0,
            16 => self.status,
            20 => self.control,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 24)?;

        match offset {
            0 => self.sector = value,
            4 => {
                if value >= 128 {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "storage word index must be 0..127".into(),
                    });
                }

                self.word_index = value as usize;
            }
            8 => self.buffer[self.word_index * 4..self.word_index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes()),
            12 => {
                self.status = 0;
                let Some(media) = &mut self.media else {
                    self.status = 2;
                    return Ok(());
                };
                let start = self.sector as u64 * SECTOR_BYTES as u64;

                if start + SECTOR_BYTES as u64 > media.len() as u64 {
                    self.status = 6;
                } else if value == 1 {
                    self.buffer
                        .copy_from_slice(&media[start as usize..start as usize + SECTOR_BYTES]);
                    self.status = 5;
                } else if value == 2 && !self.read_only {
                    media[start as usize..start as usize + SECTOR_BYTES]
                        .copy_from_slice(&self.buffer);
                    self.status = 5;
                } else {
                    self.status = 6;
                }
            }
            16 => self.status &= !(value & 3),
            20 => self.control = value & 1,
            _ => unreachable!(),
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/devices/storage/state.rs"]
mod tests;
