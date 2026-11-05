// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::MachineConfiguration;

const PROFILE: &str = include_str!("../../profiles/rv32imc.toml");

#[test]
fn cheriot_tagging_requires_eight_byte_memory_alignment_and_size() {
    for (base, size) in [(1, 8), (0, 7), (8, 9)] {
        assert!(
            MemoryRegion::new("ram".into(), base, size, MemoryKind::Ram, Tagging::Cheriot1)
                .is_err()
        );
        assert!(
            MemoryRegion::new("ram".into(), base, size, MemoryKind::Ram, Tagging::None).is_ok()
        );
    }

    let region = MemoryRegion::new(
        "ram".into(),
        0xffff_fff8,
        8,
        MemoryKind::Ram,
        Tagging::Cheriot1,
    )
    .unwrap();
    assert_eq!(region.tag_at(u32::MAX), Some(false));
    assert_eq!(region.tag_at(0x8000_0000), None);
}

#[test]
fn memory_defaults_to_no_tags_and_explicit_models_build() {
    let ordinary = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(0x8000_0000)
        .unwrap();
    assert_eq!(ordinary.bus.memory()[0].tagging(), Tagging::None);
    assert_eq!(ordinary.bus.memory()[0].tag_at(0x8000_0000), None);

    for (name, expected) in [
        ("memory", Tagging::None),
        ("cheriot1-memory", Tagging::Cheriot1),
    ] {
        let text = PROFILE.replace("[[memory]]", &format!("[[{name}]]"));
        let computer = MachineConfiguration::parse(&text)
            .unwrap()
            .build(0x8000_0000)
            .unwrap();
        assert_eq!(computer.bus.memory()[0].tagging(), expected);
        assert_eq!(
            computer.bus.memory()[0].tag_at(0x8000_0000),
            if expected == Tagging::Cheriot1 {
                Some(false)
            } else {
                None
            }
        );
    }
}

#[test]
fn unsupported_models_legacy_booleans_and_conflicting_settings_are_rejected() {
    for setting in [
        "tagging = 'cheri'",
        "tagging = 'cheriot'",
        "tagging = 'cheriot-v2'",
        "tagging = 'mte'",
        "tagging = true",
        "tagging = 'cheriot1'\ntagging = 'none'",
        "tagging = 'cheriot1'\ntagged = false",
        "tagged = true",
        "tagged = false",
    ] {
        let text = PROFILE.replace("[[memory]]", &format!("[[memory]]\n{setting}"));
        let message = MachineConfiguration::parse(&text).unwrap_err().to_string();
        assert!(
            message.contains("unknown field") || message.contains("duplicate key"),
            "{message}"
        );
    }
}
