// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use crate::exec::machine::memory::{MemoryKind, MemoryRegion, Tagging};

#[test]
fn malformed_elf_is_rejected_without_panicking() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), 0, 4096, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();
    let profile = ProcessorProfile {
        embedded: false,
        multiply_divide: false,
        compressed: false,
        control_registers: true,
    };

    for bytes in [&[][..], &b"\x7fELF"[..], &[0_u8; 52][..]] {
        assert!(load_elf(&mut bus, bytes, profile).is_err());
    }
}
