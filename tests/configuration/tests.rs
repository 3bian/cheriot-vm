// Copyright 3bian Limited and Contributors
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
    assert!(MachineConfiguration::parse(&text)
        .unwrap()
        .build(0)
        .is_err());
}
