// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::tag_bitmap::TagBitmap;

pub(super) struct CheriotTags {
    bitmap: TagBitmap,
}

impl CheriotTags {
    pub const GRANULE_BYTES: usize = 8;

    pub fn new(memory_size: usize) -> Self {
        Self {
            bitmap: TagBitmap::new(memory_size / Self::GRANULE_BYTES),
        }
    }

    pub fn clear(&mut self, offset: usize, length: usize) {
        if length == 0 {
            return;
        }

        let first = offset / Self::GRANULE_BYTES;
        let last = (offset + length - 1) / Self::GRANULE_BYTES;
        self.bitmap.clear(first, last);
    }

    pub fn get(&self, offset: usize) -> bool {
        self.bitmap.get(offset / Self::GRANULE_BYTES)
    }

    pub fn set(&mut self, offset: usize, tag: bool) {
        self.bitmap.set(offset / Self::GRANULE_BYTES, tag);
    }

    #[cfg(test)]
    pub fn storage_bytes(&self) -> usize {
        self.bitmap.storage_bytes()
    }
}
