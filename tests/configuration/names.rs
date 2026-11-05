// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::memory::Tagging;
use cheriot_vm::{MachineConfiguration, TimingPolicy};

#[test]
fn cheriot1_names_select_one_profile_and_reject_previous_spellings() {
    let profile = include_str!("../../profiles/cheriot1.toml");
    let configuration = MachineConfiguration::parse(profile).unwrap();
    assert!(configuration.is_cheriot1());
    assert_eq!(configuration.name, "cheriot1");
    assert_eq!(configuration.memory[0].tagging, Tagging::Cheriot1);
    assert_eq!(configuration.timing, TimingPolicy::Cheriot1Compatible);

    for previous in ["cheriot", "cheriot-v1"] {
        let renamed = profile.replace("type = \"cheriot1\"", &format!("type = \"{previous}\""));
        assert!(MachineConfiguration::parse(&renamed).is_err());
    }

    for previous in ["cheriot-memory", "cheriot-v1-memory"] {
        let renamed = profile.replace("[[cheriot1-memory]]", &format!("[[{previous}]]"));
        assert!(MachineConfiguration::parse(&renamed).is_err());
    }

    for previous in ["cheriot-compatible", "cheriot-v1-compatible"] {
        let renamed = profile.replace("cheriot1-compatible", previous);
        assert!(MachineConfiguration::parse(&renamed).is_err());
    }
}
