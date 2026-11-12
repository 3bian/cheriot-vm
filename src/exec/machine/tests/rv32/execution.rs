// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::Computer;
use crate::exec::machine::EmulatorError;
use crate::exec::machine::ExceptionCause;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::devices::{Device, timer32::Timer32, uart::Uart};
use crate::exec::machine::execution::TrapPolicy;
use crate::exec::machine::instructions::{
    Instruction, RegisterOperation, decode_compressed, decode_standard,
};
use crate::exec::machine::memory::{MemoryKind, MemoryRegion, Tagging};
use crate::exec::machine::processor::ProcessorProfile;
use crate::exec::machine::rv32::Processor;
use crate::exec::machine::rv32::StepOutcome;
use crate::exec::machine::rv32::computer::perform_register_operation;

#[test]
fn compressed_link_and_hints() {
    let mut c = computer(true);
    c.processor.write_register(1, 0x20).unwrap();
    c.bus
        .install_firmware(0, &0x9082_u16.to_le_bytes())
        .unwrap();
    c.step().unwrap();
    assert_eq!(c.processor.program_counter, 0x20);
    assert_eq!(c.processor.read_register(1).unwrap(), 2);

    for hint in [0x0001, 0x0005, 0x4005, 0x6005, 0x0006, 0x8006, 0x9006] {
        let mut c = computer(true);
        c.processor.write_register(1, 17).unwrap();
        let before = c.processor.registers().to_vec();
        c.bus.install_firmware(0, &u16::to_le_bytes(hint)).unwrap();
        c.step().unwrap();
        assert_eq!(c.processor.registers(), before);
        assert_eq!(c.processor.program_counter, 2);
    }
}

#[test]
fn compressed_reserved_encodings_and_all_halfwords_are_safe_to_decode() {
    for illegal in [0, 0x6101, 0x6181, 0x4002, 0x8002, 0x1082, 0x9c01] {
        assert!(decode_compressed(illegal).is_none(), "{illegal:04x}");
    }

    for bits in 0..=u16::MAX {
        if let Some(decoded) = decode_compressed(bits) {
            assert_ne!(bits & 3, 3);
            assert_eq!(decoded.length, 2);
            assert_eq!(decoded.encoded, u32::from(bits));
        }
    }
}

fn computer(compressed: bool) -> Computer {
    let profile = ProcessorProfile {
        embedded: false,
        compressed,
        multiply_divide: true,
        control_registers: true,
    };
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), 0, 4096, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
    )
    .unwrap();
    let mut computer = Computer::new(Processor::new(profile, 0), bus);
    computer.timing = crate::exec::machine::TimingPolicy::PerStep;
    computer.trap_policy = TrapPolicy::Stop;
    computer
}

#[test]
fn counter_inhibit_does_not_freeze_virtual_time() {
    let mut c = computer(false);
    c.processor.machine_registers.counter_inhibit = 5;
    word(&mut c, 0x00000013);
    c.step().unwrap();
    assert_eq!(c.processor.machine_registers.cycle, 0);
    assert_eq!(c.processor.machine_registers.retired, 0);
    assert_eq!(c.processor.machine_registers.read(0xc01), Some(1));
}

#[test]
fn csr_write_rules_and_counter_write_override() {
    let mut c = computer(false);
    c.processor.write_register(1, 42).unwrap();
    word(&mut c, 0xb0209073);
    c.step().unwrap();
    assert_eq!(c.processor.machine_registers.retired, 42);
    word(&mut c, 0x34409073);
    c.step().unwrap();
    assert_eq!(c.processor.machine_registers.interrupt_pending, 0);
    word(&mut c, 0xc000a173);
    c.processor.machine_registers.retired = 0;
    fault(&mut c, ExceptionCause::IllegalInstruction, 0xc000a173);
    assert_eq!(c.processor.read_register(2).unwrap(), 0);
}

#[test]
fn external_precedes_timer_interrupt_and_uses_vectored_target() {
    let mut c = computer(false);
    c.trap_policy = TrapPolicy::Vector;
    let mut timer = Timer32::new();
    timer.write(4, 4, 1).unwrap();
    timer.write(8, 4, 3).unwrap();
    timer.tick(1);
    c.bus
        .add_device("timer".into(), 0x2000, 0x100, Box::new(timer))
        .unwrap();
    let mut uart = Uart::new();
    uart.inject_receive(b"x").unwrap();
    uart.write(8, 4, 1).unwrap();
    c.bus
        .add_device("uart".into(), 0x3000, 0x100, Box::new(uart))
        .unwrap();
    c.processor.machine_registers.interrupt_enable = (1 << 7) | (1 << 11);
    c.processor.machine_registers.machine_status |= 1 << 3;
    c.processor.machine_registers.trap_vector = 0x101;
    assert!(matches!(
        c.step().unwrap(),
        StepOutcome::Trapped {
            cause: 11,
            interrupt: true,
            ..
        }
    ));
    assert_eq!(c.processor.program_counter, 0x12c);
    assert_eq!(c.processor.machine_registers.retired, 0);
    assert_eq!(c.processor.machine_registers.machine_status & (1 << 3), 0);
}

#[test]
fn extreme_widths_are_errors_and_execute_never_reads_mmio() {
    let mut c = computer(false);

    for width in [0, 3, 5, usize::MAX] {
        assert!(c.bus.read(0, width).is_err());
        assert!(c.bus.write(1, width, 1).is_err());
        assert!(c.bus.fetch(0, width).is_err());
        assert!(!c.bus.memory()[0].contains(1, usize::MAX));
    }

    let mut uart = Uart::new();
    uart.inject_receive(b"x").unwrap();
    c.bus
        .add_device("uart".into(), 0x2000, 0x100, Box::new(uart))
        .unwrap();
    c.processor.program_counter = 0x2000;
    fault(&mut c, ExceptionCause::InstructionAccessFault, 0x2000);
    assert_eq!(c.bus.read(0x2000, 4).unwrap(), b'x' as u32);
}

fn fault(computer: &mut Computer, expected: ExceptionCause, value: u32) {
    assert!(
        matches!(computer.step(), Err(EmulatorError::Trap { cause, value: actual, .. }) if cause == expected && actual == value)
    );
    assert_eq!(
        computer.processor.machine_registers.trap_cause,
        expected.code()
    );
    assert_eq!(computer.processor.machine_registers.trap_value, value);
    assert_eq!(computer.processor.machine_registers.retired, 0);
}

#[test]
fn fences_ignore_reserved_operands_and_wfi_is_a_legal_nop() {
    assert!(matches!(
        decode_standard(0xffff_8f8f, false).unwrap().instruction,
        Instruction::Fence
    ));
    assert!(matches!(
        decode_standard(0xffff_9f8f, false).unwrap().instruction,
        Instruction::FenceInstruction
    ));
    let mut c = computer(false);
    word(&mut c, 0x10500073);
    c.step().unwrap();
    assert_eq!(c.processor.program_counter, 4);
}

#[test]
fn invalid_target_does_not_write_link_and_untaken_branch_does_not_trap() {
    let mut c = computer(false);
    c.processor.write_register(1, 99).unwrap();
    word(&mut c, 0x002000ef);
    fault(&mut c, ExceptionCause::InstructionAddressMisaligned, 2);
    assert_eq!(c.processor.read_register(1).unwrap(), 99);
    let mut c = computer(false);
    word(&mut c, 0x00001163);
    c.step().unwrap();
    assert_eq!(c.processor.program_counter, 4);
}

#[test]
fn jalr_reads_old_base_when_destination_aliases_source_and_clears_low_bit() {
    let mut c = computer(false);
    c.processor.write_register(1, 0x41).unwrap();
    word(&mut c, 0x000080e7);
    c.step().unwrap();
    assert_eq!(c.processor.program_counter, 0x40);
    assert_eq!(c.processor.read_register(1).unwrap(), 4);
}

#[test]
fn m_division_zero_and_overflow() {
    use RegisterOperation::*;

    for first in [0, 1, u32::MAX, 0x8000_0000] {
        for operation in [DivSigned, DivUnsigned] {
            assert_eq!(perform_register_operation(operation, first, 0), u32::MAX);
        }

        for operation in [RemainderSigned, RemainderUnsigned] {
            assert_eq!(perform_register_operation(operation, first, 0), first);
        }
    }

    assert_eq!(
        perform_register_operation(DivSigned, 0x8000_0000, u32::MAX),
        0x8000_0000
    );
    assert_eq!(
        perform_register_operation(RemainderSigned, 0x8000_0000, u32::MAX),
        0
    );
}

#[test]
fn mepc_alignment_mret_and_warl_registers() {
    for compressed in [false, true] {
        let mut c = computer(compressed);
        c.processor.machine_registers.write(0x341, 0x103).unwrap();
        let expected = if compressed { 0x102 } else { 0x100 };
        assert_eq!(c.processor.machine_registers.read(0x341), Some(expected));
        c.processor.machine_registers.write(0x300, 1 << 7).unwrap();
        word(&mut c, 0x30200073);
        c.step().unwrap();
        assert_eq!(c.processor.program_counter, expected);
        assert_eq!(
            c.processor.machine_registers.machine_status,
            (3 << 11) | (1 << 7) | (1 << 3)
        );
        c.processor.machine_registers.write(0x305, 0x103).unwrap();
        assert_eq!(c.processor.machine_registers.trap_vector, 0x100);
    }
}

#[test]
fn nonexecutable_memory_can_be_inspected_but_not_fetched() {
    let mut c = computer(false);
    c.bus.set_memory_executable("ram", false).unwrap();
    assert_eq!(c.bus.inspect(0, 4).unwrap(), 0);
    fault(&mut c, ExceptionCause::InstructionAccessFault, 0);
}

#[test]
fn register_alu_golden_vectors_execute_real_encodings() {
    let vectors = [
        (0x002081b3, u32::MAX, 2, 1),
        (0x402081b3, 0, 1, u32::MAX),
        (0x002091b3, 1, 63, 0x8000_0000),
        (0x0020a1b3, u32::MAX, 0, 1),
        (0x0020b1b3, u32::MAX, 0, 0),
        (0x0020c1b3, 0xaaaa_5555, 0xffff_0000, 0x5555_5555),
        (0x0020d1b3, 0x8000_0000, 31, 1),
        (0x4020d1b3, 0x8000_0000, 31, u32::MAX),
        (0x0020e1b3, 0xaaaa_0000, 0x5555_ffff, u32::MAX),
        (0x0020f1b3, 0xaaaa_5555, 0xffff_0000, 0xaaaa_0000),
        (0x022081b3, u32::MAX, 3, 0xffff_fffd),
        (0x022091b3, 0x8000_0000, 2, u32::MAX),
        (0x0220a1b3, 0x8000_0000, u32::MAX, 0x8000_0000),
        (0x0220b1b3, u32::MAX, u32::MAX, 0xffff_fffe),
        (0x0220c1b3, (-7i32) as u32, 3, (-2i32) as u32),
        (0x0220d1b3, u32::MAX, 2, 0x7fff_ffff),
        (0x0220e1b3, (-7i32) as u32, 3, u32::MAX),
        (0x0220f1b3, u32::MAX, 2, 1),
    ];
    for (instruction, first, second, expected) in vectors {
        let mut c = computer(false);
        c.processor.write_register(1, first).unwrap();
        c.processor.write_register(2, second).unwrap();
        word(&mut c, instruction);
        c.step().unwrap();
        assert_eq!(
            c.processor.read_register(3).unwrap(),
            expected,
            "{instruction:08x}"
        );
        assert_eq!(c.processor.program_counter, 4);
    }
}

#[test]
fn second_fetch_parcel_may_cross_mappings_and_reports_its_fault_address() {
    let mut c = computer(true);
    c.bus.remove_memory("ram").unwrap();
    for base in [0, 2] {
        c.bus
            .add_memory(
                MemoryRegion::new(
                    format!("ram{base}"),
                    base,
                    2,
                    MemoryKind::Ram,
                    Tagging::None,
                )
                .unwrap(),
            )
            .unwrap();
    }
    c.bus.install_firmware(0, &[0x93, 0x00]).unwrap();
    c.bus.install_firmware(2, &[0x10, 0x00]).unwrap();
    c.step().unwrap();
    assert_eq!(c.processor.read_register(1).unwrap(), 1);
    c.processor.program_counter = 0;
    c.processor.machine_registers.retired = 0;
    c.bus.remove_memory("ram2").unwrap();
    fault(&mut c, ExceptionCause::InstructionAccessFault, 2);
}

#[test]
fn signed_loads_and_zero_destination_still_fault() {
    for (instruction, expected) in [
        (0x00008103, 0xffff_ff80),
        (0x0000c103, 128),
        (0x00009103, 0xffff_8080),
        (0x0000d103, 0x8080),
        (0x0000a103, 0x1234_8080),
    ] {
        let mut c = computer(false);
        c.processor.write_register(1, 0x80).unwrap();
        c.bus.write(0x80, 4, 0x1234_8080).unwrap();
        word(&mut c, instruction);
        c.step().unwrap();
        assert_eq!(c.processor.read_register(2).unwrap(), expected);
    }
    let mut c = computer(false);
    c.processor.write_register(1, 0x4000).unwrap();
    word(&mut c, 0x0000a003);
    fault(&mut c, ExceptionCause::LoadAccessFault, 0x4000);
}

#[test]
fn trap_at_zero_is_architectural_and_faults_advance_time_only() {
    let mut c = computer(false);
    c.trap_policy = TrapPolicy::Vector;
    c.processor.program_counter = 0x20;
    word(&mut c, 0x00000073);
    assert!(matches!(
        c.step().unwrap(),
        StepOutcome::Trapped { cause: 11, .. }
    ));
    assert_eq!(c.processor.program_counter, 0);
    let r = &c.processor.machine_registers;
    assert_eq!(r.exception_program_counter, 0x20);
    assert_eq!(r.retired, 0);
    assert_eq!(r.cycle, 1);
    assert_eq!(r.time, 1);
}

fn word(computer: &mut Computer, instruction: u32) {
    computer
        .bus
        .install_firmware(
            computer.processor.program_counter,
            &instruction.to_le_bytes(),
        )
        .unwrap();
}
