// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
const BASE: &str = "[machine]\nname = \"demo\"\n[processor]\ntype = \"rv32imc\"\n[[memory]]\nname = \"ram\"\ntype = \"ram\"\nbase = 0\nsize = 4096\n";
#[test]
fn invalid_group_is_rejected() {
    let text = BASE.replace("[processor]", "[processor]\nno_such_key = true");
    assert!(MachineConfiguration::parse(&text).is_err());
}

#[test]
fn parse_and_build() {
    let config = MachineConfiguration::parse(BASE).unwrap();
    assert!(config.processor_profile.compressed);
    assert_eq!(config.build(0).unwrap().bus.memory.len(), 1);
}

#[test]
fn reject_unsupported_architecture_and_overlaps() {
    let text = BASE.replace("rv32imc", "minimal-cheriot");
    assert!(MachineConfiguration::parse(&text).is_err());
    let text = format!(
        "{BASE}\n[[memory]]\nname = \"overlap\"\ntype = \"rom\"\nbase = 2048\nsize = 1024\n"
    );
    assert!(
        MachineConfiguration::parse(&text)
            .unwrap()
            .build(0)
            .is_err()
    );
}

#[test]
fn a_revoker_belongs_to_a_cheriot_machine_and_is_named_in_checkpoints() {
    let text = format!("{BASE}\n[[revoker]]\nname = \"revoker0\"\nbase = 0x10004000\n");
    let error = MachineConfiguration::parse(&text)
        .unwrap()
        .build(0)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("[[revoker]] needs a CHERIoT processor"),
        "{error}"
    );

    let shipped = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/cheriot.toml"
    ));
    let system = MachineConfiguration::parse(shipped)
        .unwrap()
        .build_cheriot_raw(0x8000_0000, &[0x13, 0, 0, 0])
        .unwrap();
    let revoker = system
        .computer
        .bus
        .devices()
        .iter()
        .find(|mapping| mapping.name == "revoker0")
        .unwrap();
    assert_eq!(
        (revoker.base, revoker.interrupt_source),
        (0x1000_4000, Some(7))
    );
    let mut checkpoint = Vec::new();
    system.write_checkpoint(&mut checkpoint).unwrap();
    let needle = b"revoker0\x07\0\0\0revoker";
    assert!(
        checkpoint
            .windows(needle.len())
            .any(|window| window == needle)
    );
}
