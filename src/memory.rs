// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::cheriot1::Capability;
use crate::error::EmulatorError;

mod cheriot1_tags;
mod memory_kind;
mod tag_bitmap;
#[cfg(test)]
#[path = "../tests/memory/tag_tests.rs"]
mod tag_tests;
mod tagging;

pub use memory_kind::MemoryKind;
pub use tagging::Tagging;

use cheriot1_tags::Cheriot1Tags;

pub struct MemoryRegion {
    pub base: u32,
    bytes: Vec<u8>,
    pub executable: bool,
    pub kind: MemoryKind,
    pub name: String,
    tags: Option<Cheriot1Tags>,
}

impl MemoryRegion {
    pub fn new(
        name: String,
        base: u32,
        size: usize,
        kind: MemoryKind,
        tagging: Tagging,
    ) -> Result<Self, EmulatorError> {
        if size == 0 || size > 64 * 1024 * 1024 {
            return Err(EmulatorError::Configuration(format!(
                "memory region {name} must have size from 1 byte to 64 MiB"
            )));
        }

        if (base as u64) + (size as u64) > (u32::MAX as u64) + 1 {
            return Err(EmulatorError::Configuration(format!(
                "memory region {name} extends beyond the 32-bit address space"
            )));
        }

        if let Some(granule) = tagging.granule_bytes() {
            if !(base as usize).is_multiple_of(granule) || !size.is_multiple_of(granule) {
                return Err(EmulatorError::Configuration(format!(
                    "CHERIoT memory region {name} must be {granule}-byte aligned and sized"
                )));
            }
        }

        let tags = match tagging {
            Tagging::Cheriot1 => Some(Cheriot1Tags::new(size)),
            Tagging::None => None,
        };
        Ok(Self {
            name,
            base,
            kind,
            executable: true,
            bytes: vec![0; size],
            tags,
        })
    }

    pub fn contains(&self, address: u32, width: usize) -> bool {
        let start = address as u64;
        start >= self.base as u64
            && width as u64 <= (self.base as u64 + self.bytes.len() as u64).saturating_sub(start)
            && start <= self.base as u64 + self.bytes.len() as u64
    }

    pub fn install_firmware(&mut self, address: u32, bytes: &[u8]) -> Result<(), EmulatorError> {
        let offset = self.offset(address, bytes.len())?;
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.clear_tags(offset, bytes.len());

        Ok(())
    }

    pub fn read(&self, address: u32, width: usize) -> Result<u32, EmulatorError> {
        if !matches!(width, 1 | 2 | 4) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "unsupported width".into(),
            });
        }

        let offset = self.offset(address, width)?;
        let mut value = 0;

        for index in 0..width {
            value |= (self.bytes[offset + index] as u32) << (index * 8);
        }

        Ok(value)
    }

    pub fn inspect_capability(&self, address: u32) -> Result<Capability, EmulatorError> {
        self.read_capability(address)
    }

    pub(crate) fn read_capability(&self, address: u32) -> Result<Capability, EmulatorError> {
        let offset = self.offset(address, 8)?;

        if !address.is_multiple_of(8) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "capability address must be eight-byte aligned".into(),
            });
        }

        let mut bytes = [0; 8];
        bytes.copy_from_slice(&self.bytes[offset..offset + 8]);

        Ok(Capability::from_memory(
            u64::from_le_bytes(bytes),
            self.tag_at(address).unwrap_or(false),
        ))
    }

    pub fn size(&self) -> usize {
        self.bytes.len()
    }

    pub fn tag_at(&self, address: u32) -> Option<bool> {
        if !self.contains(address, 1) {
            return None;
        }

        self.tags
            .as_ref()
            .map(|tags| tags.get((address - self.base) as usize))
    }

    pub fn tag_storage_bytes(&self) -> usize {
        self.tags.as_ref().map_or(0, Cheriot1Tags::storage_bytes)
    }

    pub fn tagging(&self) -> Tagging {
        match self.tags {
            Some(_) => Tagging::Cheriot1,
            None => Tagging::None,
        }
    }

    pub fn write(&mut self, address: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        if self.kind == MemoryKind::Rom {
            return Err(EmulatorError::Bus {
                address,
                explanation: "write to ROM".into(),
            });
        }

        if !matches!(width, 1 | 2 | 4) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "unsupported width".into(),
            });
        }

        let offset = self.offset(address, width)?;

        for index in 0..width {
            self.bytes[offset + index] = (value >> (8 * index)) as u8;
        }

        self.clear_tags(offset, width);

        Ok(())
    }

    pub(crate) fn write_checkpoint(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        output.write_all(&(self.name.len() as u32).to_le_bytes())?;
        output.write_all(self.name.as_bytes())?;
        output.write_all(&self.base.to_le_bytes())?;
        output.write_all(&(self.bytes.len() as u64).to_le_bytes())?;
        output.write_all(&[
            u8::from(self.kind == MemoryKind::Rom),
            u8::from(self.executable),
            u8::from(self.tags.is_some()),
        ])?;
        output.write_all(&self.bytes)?;
        if let Some(tags) = &self.tags {
            let mut buffer = [0_u8; 4096];
            let bytes = self.bytes.len().div_ceil(64);
            for start in (0..bytes).step_by(buffer.len()) {
                let length = (bytes - start).min(buffer.len());
                buffer.fill(0);
                for (index, byte) in buffer[..length].iter_mut().enumerate() {
                    for bit in 0..8 {
                        let offset = (start + index) * 64 + bit * 8;
                        if offset < self.bytes.len() && tags.get(offset) {
                            *byte |= 1 << bit;
                        }
                    }
                }
                output.write_all(&buffer[..length])?;
            }
        }
        Ok(())
    }

    pub fn write_capability(
        &mut self,
        address: u32,
        capability: Capability,
    ) -> Result<(), EmulatorError> {
        let offset = self.offset(address, 8)?;

        if self.kind == MemoryKind::Rom || !address.is_multiple_of(8) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "capability write requires aligned RAM".into(),
            });
        }

        let (bits, tag) = capability.to_memory();
        self.bytes[offset..offset + 8].copy_from_slice(&bits.to_le_bytes());

        if let Some(tags) = &mut self.tags {
            tags.set(offset, tag);
        }

        Ok(())
    }

    fn clear_tags(&mut self, offset: usize, size: usize) {
        if let Some(tags) = &mut self.tags {
            tags.clear(offset, size);
        }
    }

    fn offset(&self, address: u32, width: usize) -> Result<usize, EmulatorError> {
        if !self.contains(address, width) {
            return Err(EmulatorError::Bus {
                address,
                explanation: "access outside memory region".to_string(),
            });
        }

        Ok((address - self.base) as usize)
    }
}

#[cfg(test)]
#[path = "../tests/memory/tests.rs"]
mod tests;
