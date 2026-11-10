// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::firmware::load_elf;
use cheriot_vm::MachineConfiguration;

const BASE: u32 = 0x8000_0000;

const PROFILE: &str = include_str!("../../profiles/rv32imc.toml");
fn elf() -> Vec<u8> {
    let mut image = vec![0u8; 136];
    crate::firmware_elf::write_elf32_header(&mut image, BASE, 0, 1);

    for (offset, value) in [
        (52, 1u32),
        (56, 128),
        (68, 4),
        (72, 8),
        (76, 5),
        (80, 4),
        (128, 0x00100073),
    ] {
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    for offset in [60, 64] {
        image[offset..offset + 4].copy_from_slice(&BASE.to_le_bytes());
    }
    image
}

#[test]
fn elf_rejection_is_atomic_and_checks_alignment_addresses_and_dynamic_linking() {
    let mut c = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(BASE)
        .unwrap();

    for (offset, value) in [
        (52, 3u32),
        (52, 2),
        (52, 7),
        (80, 3),
        (80, 4096),
        (64, 16),
        (68, 9),
        (72, 0),
        (28, u32::MAX),
        (36, 2),
        (24, 7),
        (76, 0x100),
    ] {
        let mut image = elf();
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        c.bus.write(BASE, 4, 0xdead_beef).unwrap();
        assert!(
            load_elf(&mut c.bus, &image, c.processor.profile).is_err(),
            "field {offset} value {value}"
        );
        assert_eq!(c.bus.inspect(BASE, 4).unwrap(), 0xdead_beef);
    }

    for length in 0..132 {
        let image = elf();
        assert!(load_elf(&mut c.bus, &image[..length], c.processor.profile).is_err());
    }

    let entry = load_elf(&mut c.bus, &elf(), c.processor.profile).unwrap();
    assert_eq!(entry, BASE);
    assert_eq!(c.bus.inspect(BASE + 4, 4).unwrap(), 0);
}

#[test]
fn elf_segment_zeros_existing_bss_and_clears_only_overlapping_tags() {
    let mut bus = tagged_bus();
    let before = (BASE - 8..BASE + 32)
        .map(|address| bus.inspect(address, 1).unwrap())
        .collect::<Vec<_>>();
    let mut image = elf();
    image[72..76].copy_from_slice(&20_u32.to_le_bytes());
    let profile = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .processor_profile;
    assert_eq!(load_elf(&mut bus, &image, profile).unwrap(), BASE);

    for address in BASE - 8..BASE + 32 {
        let actual = bus.inspect(address, 1).unwrap();
        let expected = if (BASE..BASE + 4).contains(&address) {
            u32::from(image[128 + (address - BASE) as usize])
        } else if (BASE + 4..BASE + 20).contains(&address) {
            0
        } else {
            before[(address - (BASE - 8)) as usize]
        };
        assert_eq!(actual, expected, "address {address:#x}");
    }
    for (address, expected) in [
        (BASE - 8, true),
        (BASE, false),
        (BASE + 8, false),
        (BASE + 16, false),
        (BASE + 24, true),
    ] {
        assert_eq!(bus.memory()[0].tag_at(address), Some(expected));
    }
}

#[test]
fn invalid_later_elf_segment_preserves_earlier_data_and_tags() {
    let mut bus = tagged_bus();
    let before = (BASE - 8..BASE + 32)
        .map(|address| bus.inspect(address, 1).unwrap())
        .collect::<Vec<_>>();
    let mut image = elf();
    image[44..46].copy_from_slice(&2_u16.to_le_bytes());
    for (index, value) in [1_u32, 128, 0x9000_0000, 0x9000_0000, 4, 4, 5, 4]
        .into_iter()
        .enumerate()
    {
        let start = 84 + index * 4;
        image[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    let profile = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .processor_profile;
    assert!(load_elf(&mut bus, &image, profile).is_err());
    for address in BASE - 8..BASE + 32 {
        assert_eq!(
            bus.inspect(address, 1).unwrap(),
            before[(address - (BASE - 8)) as usize],
            "address {address:#x}"
        );
    }
    for address in (BASE - 8..BASE + 32).step_by(8) {
        assert_eq!(bus.memory()[0].tag_at(address), Some(true));
    }
}

fn tagged_bus() -> cheriot_vm::bus::Bus {
    use cheriot_vm::cheriot::Capability;
    use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};

    let mut bus = cheriot_vm::bus::Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "ram".into(),
            BASE - 8,
            64,
            MemoryKind::Ram,
            Tagging::Cheriot,
        )
        .unwrap(),
    )
    .unwrap();
    let capability = Capability::from_memory(0x7e3e_0000_1234_5678, true);
    for address in (BASE - 8..BASE + 32).step_by(8) {
        bus.write_capability(address, capability).unwrap();
    }
    bus
}

#[test]
fn elf_rejects_writeable_rom_and_execute_disabled_memory() {
    let mut image = elf();
    image[76..80].copy_from_slice(&7u32.to_le_bytes());
    let mut c = MachineConfiguration::parse(&PROFILE.replace("type = \"ram\"", "type = \"rom\""))
        .unwrap()
        .build(BASE)
        .unwrap();
    assert!(load_elf(&mut c.bus, &image, c.processor.profile).is_err());
    let mut c = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(BASE)
        .unwrap();
    c.bus.set_memory_executable("ram0", false).unwrap();
    assert!(load_elf(&mut c.bus, &elf(), c.processor.profile).is_err());
}

#[test]
fn full_toml_syntax_is_supported_and_duplicates_are_rejected() {
    let text = PROFILE.replace("name = \"rv32imc\"", "name = 'machine # name'");
    assert_eq!(
        MachineConfiguration::parse(&text).unwrap().name,
        "machine # name"
    );
    assert!(invalid(&PROFILE.replace(
        "type = \"rv32imc\"",
        "type = \"rv32imc\"\ntype = \"rv32i\""
    )));
    assert!(invalid(
        &PROFILE.replace("size = 0x04000000", "size = 0x04000000\nsize = 4")
    ));
    assert!(invalid(&PROFILE.replace("base = 0x80000000", "base = -1")));
    assert!(invalid(&PROFILE.replace(
        "type = \"rv32imc\"",
        "type = \"rv32imc\"\n[processor.instruction_groups]\ncompressed = 'true'"
    )));
    assert!(invalid(&PROFILE.replace(
        "name = \"uart0\"",
        "name = \"uart0\"\ncompatibility = '16550'"
    )));
    assert!(invalid(
        &PROFILE.replace("size = 0x04000000", "size = 0x_0010")
    ));
}

fn invalid(text: &str) -> bool {
    MachineConfiguration::parse(text)
        .and_then(|configuration| configuration.build(BASE))
        .is_err()
}

#[test]
fn malformed_inputs_do_not_panic() {
    let mut c = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(BASE)
        .unwrap();
    let mut state = 0x123456789abcdef0u64;

    for length in 0..256 {
        let mut data = vec![0u8; length];

        for byte in &mut data {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }

        let _ = load_elf(&mut c.bus, &data, c.processor.profile);
        let _ = MachineConfiguration::parse(&String::from_utf8_lossy(&data));
    }
}

#[test]
fn required_properties_resource_limits_and_profile_consistency() {
    for text in [
        PROFILE.replace("base = 0x80000000", ""),
        PROFILE.replace("size = 0x04000000", "size = 0"),
        PROFILE.replace("size = 0x04000000", "size = 0x100000000"),
        PROFILE.replace("base = 0x80000000", "base = 0xfffffff0"),
        PROFILE.replace("name = \"uart0\"", "name = ''"),
        PROFILE.replace(
            "type = \"rv32imc\"",
            "type = \"rv32imc\"\n[processor.instruction_groups]\ncompressed = false",
        ),
        PROFILE.replace("type = \"rv32imc\"", "type = \"cheriot\""),
        PROFILE.replace("base = 0x10000000", "base = 0x80000004"),
    ] {
        assert!(invalid(&text), "{text}");
    }

    let mut oversized_storage = PROFILE.to_string();

    for index in 0..9 {
        oversized_storage.push_str(&format!(
            "\n[[storage]]\nname='disk{index}'\nbase={}\nsectors=65536\n",
            0x3000_0000 + index * 256
        ));
    }

    let result = MachineConfiguration::parse(&oversized_storage)
        .unwrap()
        .build(BASE);
    assert!(
        matches!(result, Err(cheriot_vm::EmulatorError::Configuration(message)) if message.contains("backing"))
    );
    let mut configuration = MachineConfiguration::parse(PROFILE).unwrap();
    configuration.processor_type = "cheriot".into();
    assert!(configuration.build(BASE).is_err());
    assert!(MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(1)
        .is_err());
}

#[test]
fn rv32ec_profile_supports_only_the_embedded_registers() {
    let profile = PROFILE.replace("rv32imc", "rv32ec");
    let mut c = MachineConfiguration::parse(&profile)
        .unwrap()
        .build(BASE)
        .unwrap();
    assert_eq!(c.processor.registers().len(), 16);
    c.bus
        .install_firmware(BASE, &[0x85, 0x07, 0x05, 0x08])
        .unwrap();
    c.step().unwrap();
    assert_eq!(c.processor.read_register(15).unwrap(), 1);
    assert!(matches!(
        c.step().unwrap(),
        cheriot_vm::rv32::StepOutcome::Trapped { cause: 2, .. }
    ));
}

#[test]
fn structured_elf_mutations_reach_headers_and_reject_atomically() {
    let mut computer = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(BASE)
        .unwrap();
    let valid = elf();
    for offset in 16..128 {
        for mask in [1, 0x80, 0xff] {
            let mut image = valid.clone();
            image[offset] ^= mask;
            computer.bus.write(BASE, 4, 0xdead_beef).unwrap();
            computer.bus.write(BASE + 4, 4, 0xcafe_babe).unwrap();
            if load_elf(&mut computer.bus, &image, computer.processor.profile).is_err() {
                assert_eq!(
                    computer.bus.inspect(BASE, 4).unwrap(),
                    0xdead_beef,
                    "offset {offset}, mask {mask}"
                );
                assert_eq!(
                    computer.bus.inspect(BASE + 4, 4).unwrap(),
                    0xcafe_babe,
                    "offset {offset}, mask {mask}"
                );
            }
        }
    }
}

#[test]
fn runnable_raw_builders_validate_storage_and_minimum_fetch_without_restricting_data_installation()
{
    for text in [
        PROFILE,
        include_str!("../../profiles/rv32e.toml"),
        include_str!("../../profiles/cheriot.toml"),
    ] {
        let alignment = MachineConfiguration::parse(text)
            .unwrap()
            .processor_profile
            .minimum_alignment() as usize;
        for (config, address, bytes, valid) in [
            (text.to_owned(), BASE, vec![0; alignment], true),
            (text.to_owned(), BASE, vec![0; alignment - 1], false),
            (text.to_owned(), BASE + 1, vec![0; alignment], false),
            (
                text.replace("type = \"ram\"", "type = \"ram\"\nexecutable = false"),
                BASE,
                vec![0; alignment],
                false,
            ),
        ] {
            let config = MachineConfiguration::parse(&config).unwrap();
            let result = if config.is_cheriot() {
                config.build_cheriot_raw(address, &bytes).map(|_| ())
            } else {
                config.build_raw(address, &bytes).map(|_| ())
            };
            assert_eq!(
                result.is_ok(),
                valid,
                "{text}: {address:x}, length={}",
                bytes.len()
            );
        }
    }
    let mut computer = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build(BASE)
        .unwrap();
    computer.bus.set_memory_executable("ram0", false).unwrap();
    cheriot_vm::firmware::load_raw(&mut computer.bus, BASE + 1, &[0x42]).unwrap();
    assert_eq!(computer.bus.inspect(BASE + 1, 1).unwrap(), 0x42);
}
