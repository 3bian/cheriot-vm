// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn little_endian_and_rom() {
    let mut memory =
        MemoryRegion::new("rom".into(), 0x1000, 16, MemoryKind::Rom, Tagging::None).unwrap();
    memory
        .install_firmware(0x1000, &[0x78, 0x56, 0x34, 0x12])
        .unwrap();
    assert_eq!(memory.read(0x1000, 4).unwrap(), 0x12345678);
    assert!(memory.write(0x1000, 1, 1).is_err());
    assert!(memory.read(0x100f, 4).is_err());
}

#[test]
fn ordinary_and_dma_writes_clear_tags() {
    let mut memory =
        MemoryRegion::new("ram".into(), 0, 32, MemoryKind::Ram, Tagging::Cheriot).unwrap();
    memory.mark_tag_for_test(8);
    memory.mark_tag_for_test(16);
    memory.write(15, 2, 42).unwrap();
    assert_eq!(memory.tag_at(8), Some(false));
    assert_eq!(memory.tag_at(16), Some(false));
}

impl MemoryRegion {
    fn mark_tag_for_test(&mut self, address: u32) {
        if let Some(tags) = &mut self.tags {
            tags.set((address - self.base) as usize, true);
        }
    }
}
