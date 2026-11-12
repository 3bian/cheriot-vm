// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::MachineConfiguration;
use crate::exec::machine::devices::uart::Uart;
use crate::exec::machine::error::{EmulatorError, ExceptionCause};
use crate::exec::machine::firmware::{load_elf, load_raw};
use crate::exec::machine::rv32::StepOutcome;

const BASE: u32 = 0x8000_0000;

#[test]
fn arithmetic_binary_uses_rv32m() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    load_raw(
        &mut computer.bus,
        BASE,
        include_bytes!("../../../examples/assembly/arithmetic.bin"),
    )
    .unwrap();

    for _ in 0..4 {
        computer.step().unwrap();
    }

    assert_eq!(computer.processor.read_register(3).unwrap(), 30);
    assert_eq!(computer.processor.read_register(4).unwrap(), 600);
    assert_eq!(computer.processor.read_register(0).unwrap(), 0);
}

#[test]
fn compressed_instructions_advance_by_two_bytes() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    load_raw(
        &mut computer.bus,
        BASE,
        include_bytes!("../../../examples/assembly/compressed.bin"),
    )
    .unwrap();

    for _ in 0..2 {
        computer.step().unwrap();
    }

    assert_eq!(computer.processor.read_register(1).unwrap(), 2);
    assert_eq!(computer.processor.program_counter, BASE + 4);
    assert!(matches!(
        computer.step(),
        Err(EmulatorError::Trap {
            cause: ExceptionCause::Breakpoint,
            ..
        })
    ));
}

#[test]
fn disabled_m_extension_traps() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32e.toml"
    )));
    load_raw(&mut computer.bus, BASE, &0x02218233_u32.to_le_bytes()).unwrap();
    assert!(matches!(
        computer.step(),
        Err(EmulatorError::Trap {
            cause: ExceptionCause::IllegalInstruction,
            ..
        })
    ));
}

#[test]
fn elf_loader_checks_architecture_and_zero_fills_bss() {
    let mut image = vec![0_u8; 132];
    crate::exec::machine::tests::rv32::firmware_elf::write_elf32_header(&mut image, BASE, 0, 1);
    let program_header = 52;
    image[program_header..program_header + 4].copy_from_slice(&1_u32.to_le_bytes());
    image[program_header + 4..program_header + 8].copy_from_slice(&128_u32.to_le_bytes());
    image[program_header + 16..program_header + 20].copy_from_slice(&4_u32.to_le_bytes());
    image[program_header + 20..program_header + 24].copy_from_slice(&8_u32.to_le_bytes());
    image[program_header + 24..program_header + 28].copy_from_slice(&5_u32.to_le_bytes());
    image[60..64].copy_from_slice(&BASE.to_le_bytes());
    image[64..68].copy_from_slice(&BASE.to_le_bytes());
    image[128..132].copy_from_slice(&0x00100073_u32.to_le_bytes());
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    assert_eq!(
        load_elf(&mut computer.bus, &image, computer.processor.profile).unwrap(),
        BASE
    );
    assert_eq!(computer.bus.inspect(BASE + 4, 4).unwrap(), 0);
    image[18..20].copy_from_slice(&3_u16.to_le_bytes());
    assert!(load_elf(&mut computer.bus, &image, computer.processor.profile).is_err());
}

#[test]
fn embedded_register_file_rejects_x16() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32e.toml"
    )));

    load_raw(&mut computer.bus, BASE, &0x00100813_u32.to_le_bytes()).unwrap();
    assert!(matches!(
        computer.step(),
        Err(EmulatorError::Trap {
            cause: ExceptionCause::IllegalInstruction,
            ..
        })
    ));
}

#[test]
fn guest_program_transmits_uart_byte() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    load_raw(
        &mut computer.bus,
        BASE,
        include_bytes!("../../../examples/assembly/hello-uart.bin"),
    )
    .unwrap();

    for _ in 0..3 {
        assert!(matches!(
            computer.step().unwrap(),
            StepOutcome::Executed { .. }
        ));
    }

    assert_eq!(
        computer.bus.device::<Uart>("uart0").unwrap().transmitted(),
        b"A"
    );
    assert_eq!(computer.processor.program_counter, BASE + 12);
    assert!(matches!(
        computer.step(),
        Err(EmulatorError::Trap {
            cause: ExceptionCause::Breakpoint,
            ..
        })
    ));
}

fn machine(profile: &str) -> crate::exec::machine::Computer {
    let mut computer = MachineConfiguration::parse(profile)
        .unwrap()
        .build(BASE)
        .unwrap();
    computer.trap_policy = crate::exec::machine::execution::TrapPolicy::Stop;
    computer
}

#[test]
fn relative_branch_loops_until_counter_matches() {
    fn encode_branch(first: u32, second: u32, offset: i32, funct3: u32) -> u32 {
        let bits = offset as u32;
        ((bits >> 12 & 1) << 31)
            | ((bits >> 5 & 63) << 25)
            | (second << 20)
            | (first << 15)
            | (funct3 << 12)
            | ((bits >> 1 & 15) << 8)
            | ((bits >> 11 & 1) << 7)
            | 0x63
    }
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    let program = [
        0x00000093_u32,
        0x00108093,
        0x00300113,
        encode_branch(1, 2, -8, 4),
        0x00100073,
    ];

    for (index, instruction) in program.iter().enumerate() {
        load_raw(
            &mut computer.bus,
            BASE + (index * 4) as u32,
            &instruction.to_le_bytes(),
        )
        .unwrap();
    }

    for _ in 0..10 {
        computer.step().unwrap();

        if computer.processor.program_counter == BASE + 16 {
            break;
        }
    }

    assert_eq!(computer.processor.program_counter, BASE + 16);
    assert_eq!(computer.processor.read_register(1).unwrap(), 3);
}

#[test]
fn trap_vector_accepts_illegal_instruction_without_panicking() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    load_raw(&mut computer.bus, BASE, &0xffff_ffff_u32.to_le_bytes()).unwrap();
    computer.trap_policy = crate::exec::machine::execution::TrapPolicy::Vector;
    computer.processor.machine_registers.trap_vector = BASE + 0x100;
    assert!(matches!(
        computer.step().unwrap(),
        StepOutcome::Trapped {
            cause: 2,
            interrupt: false,
            ..
        }
    ));
    assert_eq!(computer.processor.program_counter, BASE + 0x100);
    assert_eq!(
        computer
            .processor
            .machine_registers
            .exception_program_counter,
        BASE
    );
}

#[test]
fn word_load_store_and_misalignment() {
    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));

    for (index, instruction) in [0x800000b7_u32, 0x04008093, 0x0000a103, 0x0020a223]
        .iter()
        .enumerate()
    {
        load_raw(
            &mut computer.bus,
            BASE + (index * 4) as u32,
            &instruction.to_le_bytes(),
        )
        .unwrap();
    }

    load_raw(
        &mut computer.bus,
        BASE + 0x40,
        &0x1234_5678_u32.to_le_bytes(),
    )
    .unwrap();

    for _ in 0..4 {
        computer.step().unwrap();
    }

    assert_eq!(computer.processor.read_register(2).unwrap(), 0x1234_5678);
    assert_eq!(computer.bus.inspect(BASE + 0x44, 4).unwrap(), 0x1234_5678);

    let mut computer = machine(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/rv32imc.toml"
    )));
    load_raw(&mut computer.bus, BASE, &0x0010a103_u32.to_le_bytes()).unwrap();
    assert!(matches!(
        computer.step(),
        Err(EmulatorError::Trap {
            cause: ExceptionCause::LoadAddressMisaligned,
            ..
        })
    ));
}
