// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::devices::timer32::Timer32;
use cheriot_vm::devices::timer64::Timer64;
use cheriot_vm::devices::InterruptLine;
use cheriot_vm::MachineConfiguration;

#[test]
fn timer32_selects_the_original_counter() {
    let profile = include_str!("../../profiles/rv32imc.toml").replace("[[timer64]]", "[[timer32]]");

    let mut computer = MachineConfiguration::parse(&profile)
        .unwrap()
        .build(0x8000_0000)
        .unwrap();

    assert!(computer
        .bus
        .devices()
        .iter()
        .any(|mapping| mapping.device.as_any().is::<Timer32>()));

    computer.bus.write(0x0200_0004, 4, 2).unwrap();
    computer.bus.write(0x0200_0008, 4, 3).unwrap();
    computer.bus.tick_devices(2);

    assert_eq!(computer.bus.read(0x0200_0000, 4).unwrap(), 2);
    assert!(computer.bus.interrupt_pending(InterruptLine::MachineTimer));
    assert!(computer.bus.read(0x0200_bff8, 4).is_err());
}

#[test]
fn timer64_selects_the_same_registers_on_both_profiles() {
    for processor in ["rv32imc", "cheriot1"] {
        let text = format!(
                "[machine]\nname = 'timer-test'\n[processor]\ntype = '{processor}'\n\
                 [[cheriot1-memory]]\nname = 'ram'\ntype = 'ram'\nbase = 0x80000000\nsize = 4096\n\
                 [[cheriot1-memory]]\nname = 'shadow'\ntype = 'ram'\nbase = 0x83000000\nsize = 0x100000\n\
                 [[timer64]]\nname = 'timer'\nbase = 0x02000000\n"
            );
        let configuration = MachineConfiguration::parse(&text).unwrap();
        let mut bus = if configuration.is_cheriot1() {
            configuration
                .build_cheriot1_raw(0x8000_0000, &[0x13, 0, 0, 0])
                .unwrap()
                .computer
                .bus
        } else {
            configuration.build(0x8000_0000).unwrap().bus
        };

        assert!(bus.device::<Timer64>("timer").is_some());
        bus.write(0x0200_4000, 4, 2).unwrap();
        bus.tick_devices(1);
        assert_eq!(bus.read(0x0200_bff8, 4).unwrap(), 1);
        assert!(!bus.interrupt_pending(InterruptLine::MachineTimer));
        bus.tick_devices(1);
        assert!(bus.interrupt_pending(InterruptLine::MachineTimer));
        bus.write(0x0200_0000, 4, 1).unwrap();
        assert!(bus.interrupt_pending(InterruptLine::MachineSoftware));
        assert_eq!(bus.read(0x0200_bffc, 4).unwrap(), 0);
        assert!(bus.write(0x0200_bff8, 4, 0).is_err());
    }
}
