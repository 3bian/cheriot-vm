// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::cheriot::Capability;
use crate::exec::machine::memory::MemoryRegion;

pub struct MemoryPort<'a> {
    memory: &'a mut [MemoryRegion],
}

impl<'a> MemoryPort<'a> {
    pub const GRANULE_BYTES: u32 = 8;

    pub(super) fn new(memory: &'a mut [MemoryRegion]) -> Self {
        Self { memory }
    }

    pub fn read_byte(&self, address: u32) -> Option<u8> {
        let region = self
            .memory
            .iter()
            .find(|region| region.contains(address, 1))?;
        region.read(address, 1).ok().map(|value| value as u8)
    }

    pub fn read_granule(&self, address: u32) -> Option<Capability> {
        let region = self
            .memory
            .iter()
            .find(|region| region.contains(address, Self::GRANULE_BYTES as usize))?;
        region.read_capability(address).ok()
    }

    pub fn clear_tag(&mut self, address: u32) -> bool {
        self.memory
            .iter_mut()
            .find(|region| region.contains(address, 1))
            .is_some_and(|region| region.clear_tag(address))
    }
}
