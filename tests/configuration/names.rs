// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::configuration::DefaultDevices;
use cheriot_vm::memory::Tagging;
use cheriot_vm::{MachineConfiguration, TimingPolicy};

#[test]
fn cheriot1_names_select_one_profile_and_reject_previous_spellings() {
    let profile = include_str!("../../profiles/cheriot.toml");
    let configuration = MachineConfiguration::parse(profile).unwrap();
    assert!(configuration.is_cheriot());
    assert_eq!(configuration.name, "cheriot");
    assert_eq!(configuration.memory[0].tagging, Tagging::Cheriot);
    assert_eq!(configuration.timing, TimingPolicy::PerStep);
    assert_eq!(configuration.timer_hz, 2_000);

    for previous in ["cheriot1", "cheriot-v1"] {
        let renamed = profile.replace("type = \"cheriot\"", &format!("type = \"{previous}\""));
        assert!(MachineConfiguration::parse(&renamed).is_err());
    }

    for previous in ["cheriot1-memory", "cheriot-v1-memory"] {
        let renamed = profile.replace("[[cheriot-memory]]", &format!("[[{previous}]]"));
        assert!(MachineConfiguration::parse(&renamed).is_err());
    }
}

#[test]
fn processors_share_clock_defaults_and_accept_only_supported_modes() {
    for processor in [
        "rv32e", "rv32em", "rv32ec", "rv32emc", "rv32i", "rv32im", "rv32ic", "rv32imc", "cheriot",
    ] {
        let text = format!("[machine]\nname='defaults'\n[processor]\ntype='{processor}'\n");
        let configuration = MachineConfiguration::parse(&text).unwrap();
        let expected_devices = if processor == "cheriot" {
            DefaultDevices::CheriotCompatibility
        } else {
            DefaultDevices::Configured
        };
        assert_eq!(configuration.timing, TimingPolicy::Elapsed, "{processor}");
        assert_eq!(configuration.timer_hz, 2_000, "{processor}");
        assert_eq!(
            configuration.default_devices, expected_devices,
            "{processor}"
        );
        for (name, expected) in [
            ("elapsed", TimingPolicy::Elapsed),
            ("per-step", TimingPolicy::PerStep),
        ] {
            let profile = text.replace(
                "[machine]\n",
                &format!("[machine]\ntiming='{name}'\ntimer_hz=12345\n"),
            );
            let parsed = MachineConfiguration::parse(&profile).unwrap();
            assert_eq!((parsed.timing, parsed.timer_hz), (expected, 12_345));
        }
        for setting in [
            "default_devices='configured'",
            "timing='unknown'",
            "timing='monotonic'",
            "timing='cheriot1-compatible'",
            "timing='cheriot-compatible'",
            "timing='host-monotonic'",
            "timer_hz=0",
            "timer_hz=-1",
            "timer_hz=1000000001",
            "timer_hz=1.5",
            "timer_hz='2000'",
        ] {
            let invalid = text.replace("[machine]\n", &format!("[machine]\n{setting}\n"));
            assert!(
                MachineConfiguration::parse(&invalid).is_err(),
                "{processor}: {setting}"
            );
        }
    }

    for (profile, timing, quantum) in [
        (include_str!("../../profiles/cheriot.toml"), "per-step", 100),
        (include_str!("../../profiles/rv32e.toml"), "elapsed", 1),
        (include_str!("../../profiles/rv32imc.toml"), "elapsed", 1),
    ] {
        let document: toml::Value = toml::from_str(profile).unwrap();
        assert_eq!(document["machine"]["timing"].as_str(), Some(timing));
        assert_eq!(
            MachineConfiguration::parse(profile).unwrap().steps_per_tick,
            quantum
        );
        assert_eq!(document["machine"]["timer_hz"].as_integer(), Some(2_000));
        assert!(document["machine"].get("default_devices").is_none());
        MachineConfiguration::parse(profile).unwrap();
    }
}

#[test]
fn timer_frequency_is_validated_before_and_after_configuration_builds() {
    for (profile, capability) in [
        (include_str!("../../profiles/cheriot.toml"), true),
        (include_str!("../../profiles/rv32imc.toml"), false),
    ] {
        for hz in [0, 1_000_000_001] {
            let mut configuration = MachineConfiguration::parse(profile).unwrap();
            configuration.timer_hz = hz;
            if capability {
                assert!(configuration
                    .build_cheriot_raw(0x8000_0000, &[0x13, 0, 0, 0])
                    .is_err());
            } else {
                assert!(configuration
                    .build_raw(0x8000_0000, &[0x13, 0, 0, 0])
                    .is_err());
            }
        }
        for hz in [1, 2_000, 1_000_000_000] {
            let mut configuration = MachineConfiguration::parse(profile).unwrap();
            configuration.timer_hz = hz;
            configuration.timing = TimingPolicy::Elapsed;
            configuration.steps_per_tick = 1;
            if capability {
                let mut system = configuration
                    .build_cheriot_raw(0x8000_0000, &[0x13, 0, 0, 0])
                    .unwrap();
                assert_eq!(system.computer.timer_hz(), hz);
                system.step().unwrap();
                assert!(system.computer.set_timer_hz(3).is_err());
                assert_eq!(system.computer.timer_hz(), hz);
            } else {
                let mut computer = configuration
                    .build_raw(0x8000_0000, &[0x13, 0, 0, 0])
                    .unwrap();
                assert_eq!(computer.timer_hz(), hz);
                computer.step().unwrap();
                assert!(computer.set_timer_hz(3).is_err());
                assert_eq!(computer.timer_hz(), hz);
            }
        }
    }
}

#[test]
fn direct_constructors_share_the_same_timer_defaults() {
    let bus = cheriot_vm::bus::Bus::new();
    let capability = cheriot_vm::cheriot::Computer::new(bus, 0x8000_0000);
    let profile =
        MachineConfiguration::parse("[machine]\nname='rv32'\n[processor]\ntype='rv32i'\n")
            .unwrap()
            .processor_profile;
    let processor = cheriot_vm::rv32::Processor::new(profile, 0);
    let integer = cheriot_vm::Computer::new(processor, cheriot_vm::bus::Bus::new());
    assert_eq!(
        (capability.timing, capability.timer_hz()),
        (TimingPolicy::Elapsed, 2_000)
    );
    assert_eq!(
        (integer.timing, integer.timer_hz()),
        (TimingPolicy::Elapsed, 2_000)
    );
}
