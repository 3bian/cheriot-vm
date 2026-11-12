// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use crate::exec::machine::memory::{MemoryKind, Tagging};

#[test]
fn crossing_and_overlapping_regions_are_rejected() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("one".into(), 0x1000, 64, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();

    assert!(
        bus.add_memory(
            MemoryRegion::new("two".into(), 0x103f, 64, MemoryKind::Ram, Tagging::None).unwrap()
        )
        .is_err()
    );
    assert!(bus.read(0x103f, 4).is_err());
}
