// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(feature = "gdb")]

use gdbstub::arch::{Arch, RegId, Registers as GdbRegisters};
use gdbstub::target::ext::base::singlethread::{SingleThreadResume, SingleThreadSingleStep};
use gdbstub::target::ext::breakpoints::{HwBreakpoint, SwBreakpoint};

use crate::debug::gdb::{BreakpointType, StopReason};
use crate::exec::machine::MachineConfiguration;
use crate::exec::machine::TimingPolicy;
use crate::exec::machine::firmware::load_raw;
use crate::exec::machine::rv32::debugger::Architecture;
use crate::exec::machine::rv32::debugger::RegisterId;
use crate::exec::machine::rv32::debugger::Registers;
use crate::exec::machine::rv32::debugger::Target;

const PROFILES: [&str; 8] = [
    "rv32e", "rv32em", "rv32ec", "rv32emc", "rv32i", "rv32im", "rv32ic", "rv32imc",
];

#[test]
fn host_clock_pauses_during_debugger_stops() {
    for profile in ["rv32e", "rv32imc"] {
        let mut target = target(
            profile,
            TimingPolicy::Elapsed,
            &[0x0000_0013, 0xffdf_f06f],
            0,
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(target.computer().processor.machine_registers.time, 0);
        target.resume(None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(target.advance().unwrap(), None);
        target.halt(StopReason::Interrupted);
        let stopped = target.computer().processor.machine_registers.time;
        assert!(stopped >= 40, "{profile}: {stopped}");
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(target.computer().processor.machine_registers.time, stopped);
        target.step(None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
        assert!(target.computer().processor.machine_registers.time > stopped);
    }
}

fn target(profile: &str, timing: TimingPolicy, words: &[u32], limit: u64) -> Target {
    let mut configuration = MachineConfiguration::parse(&format!(
        "[machine]\nname='debugger-test'\n[processor]\ntype='{profile}'\n\
         [processor.instruction_groups]\ncontrol_registers=true\n\
         [[memory]]\nname='ram'\ntype='ram'\nbase=0x80000000\nsize=256\n"
    ))
    .unwrap();
    configuration.timing = timing;
    configuration.steps_per_tick = 1;
    let mut computer = configuration.build(0x8000_0000).unwrap();
    let bytes: Vec<_> =
        crate::exec::machine::tests::rv32::firmware_words::firmware_from_words(words);
    load_raw(&mut computer.bus, 0x8000_0000, &bytes).unwrap();
    Target::new(computer, Some(limit))
}

#[test]
fn all_integer_profiles_share_breakpoint_budget_and_readonly_inspection() {
    for profile in PROFILES {
        for timing in [TimingPolicy::PerStep, TimingPolicy::Elapsed] {
            let mut target = target(profile, timing, &[0x0010_8093, 0xffdf_f06f], 2);
            let mut before = [0; 8];
            target.inspect(0x8000_0000, &mut before).unwrap();
            assert!(target.add_sw_breakpoint(0x8000_0000, 4).is_ok());
            target.resume(None).unwrap();
            assert_eq!(
                target.advance().unwrap(),
                Some(StopReason::Breakpoint(
                    0x8000_0000,
                    BreakpointType::Software
                ))
            );
            assert_eq!(target.computer().attempted_steps, 0);
            target.step(None).unwrap();
            assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
            assert_eq!(target.computer().processor.registers()[1], 1);
            target.resume(None).unwrap();
            assert_eq!(target.advance().unwrap(), None);
            assert_eq!(
                target.advance().unwrap(),
                Some(StopReason::Breakpoint(
                    0x8000_0000,
                    BreakpointType::Software
                ))
            );
            target.resume(None).unwrap();
            assert_eq!(target.advance().unwrap(), Some(StopReason::RunLimit));
            assert_eq!(target.computer().attempted_steps, 2);
            let mut after = [0; 8];
            target.inspect(0x8000_0000, &mut after).unwrap();
            assert_eq!(before, after);
            assert!(target.inspect(0xffff_ffff, &mut [0; 2]).is_err());
            assert!(target.inspect(0x8000_0000, &mut [0; 1025]).is_err());
            assert!(target.monitor(b"capability pcc").is_err());
            assert!(target.add_sw_breakpoint(0x8000_0001, 2).is_err());
            assert_eq!(
                target.add_sw_breakpoint(0x8000_0002, 2).is_ok(),
                profile.ends_with('c')
            );
        }
    }
}

#[test]
fn integer_register_packets_match_individual_reads_and_actual_architecture() {
    for profile in PROFILES {
        let target = target(profile, TimingPolicy::PerStep, &[0x0010_8093], 10);
        let registers = Registers::new(&target.computer().processor);
        let mut bytes = Vec::new();
        registers.gdb_serialize(|byte| bytes.push(byte));
        assert_eq!(bytes.len(), 236);
        let (pc, _) = RegisterId::from_raw_id(32).unwrap();
        let mut short = [0xa5; 3];
        assert_eq!(registers.register(pc, &mut short), None);
        assert_eq!(short, [0xa5; 3]);
        for number in 0..33 {
            let (id, size) = RegisterId::from_raw_id(number).unwrap();
            assert_eq!(id.to_raw_id(), Some(number));
            let mut single = [0xa5; 4];
            let size = size.unwrap().get();
            let absent = profile.contains('e') && (16..32).contains(&number);
            assert_eq!(
                registers.register(id, &mut single),
                Some(if absent { 0 } else { size })
            );
            if absent {
                assert_eq!(&bytes[number * 4..number * 4 + 4], &[None; 4]);
                assert_eq!(single, [0xa5; 4]);
            } else {
                assert_eq!(bytes[number * 4..number * 4 + 4], single.map(Some));
            }
        }
        let (misa, _) = RegisterId::from_raw_id(65 + 0x301).unwrap();
        let mut value = [0; 4];
        assert_eq!(registers.register(misa, &mut value), Some(4));
        assert_eq!(
            u32::from_le_bytes(value),
            target.computer().processor.machine_registers.isa
        );
        assert!(RegisterId::from_raw_id(33).is_none());
        assert!(RegisterId::from_raw_id(usize::MAX).is_none());
    }
    let xml = Architecture::target_description_xml().unwrap();
    assert!(xml.contains("name=\"pc\" bitsize=\"32\" regnum=\"32\""));
    assert!(xml.contains("name=\"misa\" bitsize=\"32\" regnum=\"834\""));
    assert!(!xml.contains("cheriot"));
}

#[test]
fn integer_fault_record_preserves_transition_and_single_step_observes_traps() {
    for profile in PROFILES {
        let mut computer = target(
            profile,
            TimingPolicy::PerStep,
            &[0xffff_ffff, 0x0000_0013],
            10,
        )
        .into_computer();
        computer.processor.machine_registers.trap_vector = 0x8000_0004;
        let mut target = Target::new(computer, Some(10));
        target.step(None).unwrap();
        assert_eq!(target.advance().unwrap(), Some(StopReason::Fault(2)));
        let fault = target.monitor(b"fault").unwrap();
        assert!(fault.contains("resulting_pc=0x80000004"));
        target.step(None).unwrap();
        assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
        assert_eq!(target.monitor(b"fault").unwrap(), fault);
        assert_eq!(target.computer().processor.program_counter, 0x8000_0008);
    }
}

#[test]
fn integer_ebreak_is_a_host_stop_without_guest_completion() {
    let mut target = target("rv32i", TimingPolicy::PerStep, &[0x0010_0073], 1);
    target.step(None).unwrap();
    assert_eq!(
        target.advance().unwrap(),
        Some(StopReason::Ebreak(0x8000_0000))
    );
    assert_eq!(target.computer().completion, None);
    assert!(
        target
            .monitor(b"fault")
            .unwrap()
            .contains("cause=0x00000003")
    );
}

#[test]
fn disabled_integer_csr_group_is_unavailable_in_bulk_and_single_reads() {
    let mut computer = target("rv32e", TimingPolicy::PerStep, &[0x0000_0013], 1).into_computer();
    computer.processor.profile.control_registers = false;
    let registers = Registers::new(&computer.processor);
    let mut bytes = Vec::new();
    registers.gdb_serialize(|byte| bytes.push(byte));
    assert!(bytes[132..].iter().all(Option::is_none));
    let (misa, _) = RegisterId::from_raw_id(65 + 0x301).unwrap();
    assert_eq!(registers.register(misa, &mut [0; 4]), Some(0));
}

#[test]
fn stop_policy_uses_architectural_exception_codes_and_preserves_fault_details() {
    for (words, pc, cause, value) in [
        (vec![0x0010_0073], 0x8000_0000, 3, 0u32),
        (vec![0x0000_0073], 0x8000_0000, 11, 0),
        (vec![0x0000_0013], 0x8000_0100, 1, 0x8000_0100),
    ] {
        let mut computer = target("rv32i", TimingPolicy::PerStep, &words, 4).into_computer();
        computer.trap_policy = crate::exec::machine::execution::TrapPolicy::Stop;
        computer.processor.program_counter = pc;
        let mut target = Target::new(computer, Some(4));
        target.step(None).unwrap();
        assert_eq!(
            target.advance().unwrap(),
            Some(if cause == 3 {
                StopReason::Ebreak(pc)
            } else {
                StopReason::Fault(cause)
            })
        );
        let fault = target.monitor(b"fault").unwrap();
        assert!(fault.contains(&format!("address=0x{pc:08x}")), "{fault}");
        assert!(fault.contains(&format!("cause=0x{cause:08x}")), "{fault}");
        assert!(fault.contains(&format!("value=0x{value:08x}")), "{fault}");
    }
}

#[test]
fn removing_one_breakpoint_type_preserves_the_other_and_duplicate_requests_are_idempotent() {
    for remove_software in [false, true] {
        let mut target = target(
            "rv32imc",
            TimingPolicy::PerStep,
            &[0x0010_8093, 0xffdf_f06f],
            10,
        );
        let address = 0x8000_0000;
        for _ in 0..2 {
            assert!(matches!(target.add_sw_breakpoint(address, 4), Ok(true)));
            assert!(matches!(target.add_hw_breakpoint(address, 4), Ok(true)));
        }
        for _ in 0..2 {
            assert!(matches!(
                if remove_software {
                    target.remove_sw_breakpoint(address, 4)
                } else {
                    target.remove_hw_breakpoint(address, 4)
                },
                Ok(true)
            ));
        }
        target.resume(None).unwrap();
        assert_eq!(
            target.advance().unwrap(),
            Some(StopReason::Breakpoint(
                address,
                if remove_software {
                    BreakpointType::Hardware
                } else {
                    BreakpointType::Software
                },
            ))
        );
        let computer = target.computer();
        assert_eq!(computer.attempted_steps, 0);
        assert_eq!(computer.processor.machine_registers.retired, 0);
        assert_eq!(computer.processor.program_counter, address);
    }
}

#[test]
fn retransmitted_breakpoint_insertions_need_only_one_removal() {
    for hardware in [false, true] {
        let mut target = target(
            "rv32imc",
            TimingPolicy::PerStep,
            &[0x0010_8093, 0xffdf_f06f],
            10,
        );
        for _ in 0..2 {
            assert!(matches!(
                if hardware {
                    target.add_hw_breakpoint(0x8000_0000, 4)
                } else {
                    target.add_sw_breakpoint(0x8000_0000, 4)
                },
                Ok(true)
            ));
        }
        assert!(matches!(
            if hardware {
                target.remove_hw_breakpoint(0x8000_0000, 4)
            } else {
                target.remove_sw_breakpoint(0x8000_0000, 4)
            },
            Ok(true)
        ));
        target.resume(None).unwrap();
        assert_eq!(target.advance().unwrap(), None);
        let computer = target.computer();
        assert_eq!(computer.attempted_steps, 1);
        assert_eq!(computer.processor.machine_registers.retired, 1);
    }
}

#[test]
fn monitor_rejects_oversized_and_invalid_utf8_commands() {
    let target = target("rv32imc", TimingPolicy::PerStep, &[0x0000_0013], 1);
    assert_eq!(
        target.monitor(&[b'a'; 129]).unwrap_err(),
        "monitor command too long"
    );
    assert_eq!(
        target.monitor(&[0xff]).unwrap_err(),
        "monitor requires UTF-8"
    );
}
