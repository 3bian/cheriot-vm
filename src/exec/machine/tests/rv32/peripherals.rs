// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;
use crate::exec::machine::ExceptionCause;
use crate::exec::machine::MachineConfiguration;
use crate::exec::machine::devices::{ethernet::Ethernet, gpio::Gpio, storage::Storage};
use crate::exec::machine::execution::TrapPolicy;
use crate::exec::machine::firmware::load_raw;
use crate::exec::machine::rv32::StepOutcome;

const BASE: u32 = 0x8000_0000;

#[test]
fn guest_trap_handler_returns_to_the_following_instruction() {
    let config = MachineConfiguration::parse(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )))
    .unwrap();
    let mut computer = config.build(BASE).unwrap();
    load_raw(
        &mut computer.bus,
        BASE,
        include_bytes!("../../../examples/assembly/trap-return.bin"),
    )
    .unwrap();
    let mut traps = 0;

    for _ in 0..50 {
        if computer.processor.program_counter == BASE + 0x18 {
            break;
        }

        if let StepOutcome::Trapped {
            address,
            cause,
            interrupt,
        } = computer.step().unwrap()
        {
            assert_eq!((address, cause, interrupt), (BASE + 0x10, 11, false));
            traps += 1;
        }
    }

    assert_eq!(computer.processor.program_counter, BASE + 0x18);
    assert_eq!(traps, 1);
    assert_eq!(computer.processor.read_register(10).unwrap(), 1);
    assert_eq!(computer.processor.read_register(11).unwrap(), 42);
    assert_eq!(
        computer
            .processor
            .machine_registers
            .exception_program_counter,
        BASE + 0x14
    );
}

#[test]
fn peripheral_firmware_checks_registers_and_observable_devices() {
    let mut config = MachineConfiguration::parse(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )))
    .unwrap();
    config.timing = crate::exec::machine::TimingPolicy::PerStep;
    let mut computer = config.build(BASE).unwrap();
    computer.trap_policy = TrapPolicy::Stop;
    load_raw(
        &mut computer.bus,
        BASE,
        include_bytes!("../../../examples/assembly/peripherals.bin"),
    )
    .unwrap();
    let stopped = (0..200).any(|_| match computer.step() {
        Ok(StepOutcome::Executed { .. }) => false,
        Err(EmulatorError::Trap {
            cause: ExceptionCause::Breakpoint,
            address,
            ..
        }) if address == BASE + 0xf8 => true,
        result => panic!("unexpected result {result:?}"),
    });
    assert!(stopped, "guest did not finish");
    assert_eq!(
        computer.processor.read_register(10).unwrap(),
        0,
        "guest self-check failed"
    );
    assert_eq!(computer.processor.machine_registers.retired, 62);
    assert_eq!(computer.processor.machine_registers.time, 63);
    assert_eq!(
        computer.bus.device_mut::<Gpio>("gpio0").unwrap().outputs(),
        1
    );
    assert_eq!(
        &computer
            .bus
            .device_mut::<Storage>("storage0")
            .unwrap()
            .media_bytes()
            .unwrap()[..4],
        &[0x78, 0x56, 0x34, 0x12]
    );
    assert_eq!(
        computer
            .bus
            .device_mut::<Ethernet>("ethernet0")
            .unwrap()
            .take_transmitted(),
        Some(b"abc".to_vec())
    );
}
