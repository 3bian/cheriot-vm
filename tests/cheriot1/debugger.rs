// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(feature = "gdb")]

use gdbstub::arch::{RegId, Registers as GdbRegisters};
use gdbstub::target::ext::base::singlethread::{SingleThreadResume, SingleThreadSingleStep};
use gdbstub::target::ext::breakpoints::{HwBreakpoint, SwBreakpoint};

use cheriot_vm::cheriot1::debugger::RegisterId;
use cheriot_vm::cheriot1::debugger::Registers;
use cheriot_vm::cheriot1::debugger::Target;
use cheriot_vm::cheriot1::Capability;
use cheriot_vm::debugger::StopReason;
use cheriot_vm::MachineConfiguration;

fn target() -> Target {
    let words = [0x0010_8093_u32, 0xffdf_f06f];
    let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    let system = MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
        .unwrap()
        .build_cheriot1_raw(0x8000_0000, &bytes)
        .unwrap();
    Target::new(system, Some(10))
}

#[test]
fn breakpoint_bypasses_once_without_changing_bytes_tags_or_counters() {
    let mut target = target();
    let mut original = [0; 8];
    target.inspect(0x8000_0000, &mut original).unwrap();
    assert!(target.add_sw_breakpoint(0x8000_0000, 4).is_ok());
    target.resume(None).unwrap();
    assert_eq!(
        target.advance().unwrap(),
        Some(StopReason::Breakpoint(0x8000_0000))
    );
    assert_eq!(target.system().computer.attempted_steps, 0);
    target.step(None).unwrap();
    assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
    assert_eq!(target.system().computer.attempted_steps, 1);
    target.resume(None).unwrap();
    assert_eq!(target.advance().unwrap(), None);
    assert_eq!(
        target.advance().unwrap(),
        Some(StopReason::Breakpoint(0x8000_0000))
    );
    assert_eq!(target.system().computer.attempted_steps, 2);
    assert!(target.remove_sw_breakpoint(0x8000_0000, 4).is_ok());
    let mut current = [0; 8];
    target.inspect(0x8000_0000, &mut current).unwrap();
    assert_eq!(current, original);
    assert_eq!(
        target.system().computer.bus.memory()[0].tag_at(0x8000_0000),
        Some(false)
    );
}

#[test]
fn memory_and_monitor_requests_are_bounded_and_side_effect_free() {
    let mut target = target();
    let before = target.system().computer.processor.machine_registers.time;
    assert!(target.inspect(0x1000_0000, &mut [0; 4]).is_err());
    assert!(target.inspect(u32::MAX, &mut [0; 2]).is_err());
    assert!(target.inspect(0x8000_0000, &mut [0; 1025]).is_err());
    for command in [
        "tags 0x80000001 1",
        "tags 0xfffffff8 2",
        "tags 0x80000000 65",
        "capability c16",
        "capability c99999999999999999999999",
        "shell anything",
    ] {
        assert!(target.monitor(command.as_bytes()).is_err(), "{command}");
    }
    assert!(target
        .monitor(b"capability pcc")
        .unwrap()
        .contains("top=0x100000000"));
    assert!(target
        .monitor(b"tags 0x80000000 4")
        .unwrap()
        .contains("without revocation filtering"));
    assert!(target.add_sw_breakpoint(0x8000_0001, 2).is_err());
    assert!(target.add_sw_breakpoint(0x1000_0000, 4).is_err());
    assert_eq!(target.system().computer.attempted_steps, 0);
    assert_eq!(
        target.system().computer.processor.machine_registers.time,
        before
    );
}

#[cfg(target_os = "linux")]
#[test]
fn debug_output_does_not_change_inherited_descriptor_flags() {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::unistd::{pipe, read};
    let (reader, writer) = pipe().unwrap();
    let flags = fcntl(&writer, FcntlArg::F_GETFL).unwrap();
    assert_eq!(flags & OFlag::O_NONBLOCK.bits(), 0);
    let mut output = cheriot_vm::host::debugger::output::DebugOutput::new(&writer).unwrap();
    output.enqueue(b"\x1bQ", false).unwrap();
    output.pump().unwrap();
    assert_eq!(fcntl(&writer, FcntlArg::F_GETFL).unwrap(), flags);
    let mut bytes = [0; 5];
    assert_eq!(read(&reader, &mut bytes).unwrap(), 5);
    assert_eq!(&bytes, b"\\x1bQ");
}

#[test]
fn fault_monitor_preserves_the_recorded_transition_after_handler_progress() {
    let mut system = target().into_system();
    system
        .computer
        .bus
        .install_firmware(0x8000_0000, &[0xff, 0xff, 0xff, 0xff, 0x13, 0, 0, 0])
        .unwrap();
    system.computer.processor.machine_trap = Capability::from_memory(0x5e3e_0000_8000_0004, true);
    let mut target = Target::new(system, None);
    target.step(None).unwrap();
    assert_eq!(target.advance().unwrap(), Some(StopReason::Fault(2)));
    let fault = target.monitor(b"fault").unwrap();
    assert!(fault.contains("resulting_pc=0x80000004"));
    target.step(None).unwrap();
    assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
    assert_eq!(
        target.system().computer.processor.program_counter,
        0x8000_0008
    );
    assert_eq!(target.monitor(b"fault").unwrap(), fault);
}

#[test]
fn register_wire_bytes_distinguish_cursors_payloads_tags_and_pc() {
    let mut system = target().into_system();
    system
        .computer
        .processor
        .write_register(3, Capability::from_memory(0x7e3e_0000_1234_5678, true))
        .unwrap();
    let registers = Registers::new(&system.computer.processor);
    let mut bytes = Vec::new();
    registers.gdb_serialize(|byte| bytes.push(byte));
    assert_eq!(bytes.len(), 321);
    assert_eq!(
        &bytes[12..16],
        &[Some(0x78), Some(0x56), Some(0x34), Some(0x12)]
    );
    assert_eq!(&bytes[64..128], &[None; 64]);
    assert_eq!(&bytes[128..132], &[Some(0), Some(0), Some(0), Some(0x80)]);
    assert_eq!(
        &bytes[156..164],
        &[
            Some(0x78),
            Some(0x56),
            Some(0x34),
            Some(0x12),
            Some(0),
            Some(0),
            Some(0x3e),
            Some(0x7e)
        ]
    );
    assert_eq!(bytes[303], Some(1));
    assert_eq!(&bytes[260..264], &[Some(0); 4]);
    for number in 0..75 {
        let (id, size) = RegisterId::from_raw_id(number).unwrap();
        assert_eq!(id.to_raw_id(), Some(number));
        let mut buffer = [0; 8];
        let read = registers.register(id, &mut buffer).unwrap();
        assert_eq!(
            read,
            if (16..32).contains(&number) {
                0
            } else {
                size.unwrap().get()
            }
        );
    }
    assert!(RegisterId::from_raw_id(75).is_none());
    assert!(RegisterId::from_raw_id(usize::MAX).is_none());
}

#[test]
fn pauses_preserve_budget_and_single_step_does_not_skip_traps() {
    let mut target = target();
    for _ in 0..10 {
        target.step(None).unwrap();
        assert_eq!(target.advance().unwrap(), Some(StopReason::Step));
        assert_eq!(target.advance().unwrap(), None);
    }
    target.resume(None).unwrap();
    assert_eq!(target.advance().unwrap(), Some(StopReason::RunLimit));
    let mut system = target.into_system();
    system
        .computer
        .bus
        .install_firmware(0x8000_0000, &[0xff; 4])
        .unwrap();
    system.computer.processor.program_counter = 0x8000_0000;
    let mut target = Target::new(system, None);
    target.step(None).unwrap();
    assert_eq!(target.advance().unwrap(), Some(StopReason::Fault(2)));
    assert_eq!(target.system().computer.attempted_steps, 11);
    assert_eq!(target.system().computer.processor.program_counter, 0);
    assert!(target
        .monitor(b"fault")
        .unwrap()
        .contains("address=0x80000000 cause=0x00000002"));
}

#[test]
fn pause_resume_has_identical_architectural_and_memory_effects() {
    let mut direct = target().into_system();
    let mut debug = target();
    for index in 0..10 {
        direct.step().unwrap();
        debug.step(None).unwrap();
        debug.advance().unwrap();
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        direct.write_state(&mut expected).unwrap();
        debug.system().write_state(&mut actual).unwrap();
        assert_eq!(expected, actual, "step {index}");
        for offset in 0..16 {
            let address = 0x8000_0000 + offset;
            assert_eq!(
                direct.computer.bus.inspect(address, 1).unwrap(),
                debug.system().computer.bus.inspect(address, 1).unwrap()
            );
        }
    }
}

#[test]
fn rtos_pause_schedule_preserves_complete_final_ram_tags_and_registers() {
    compare_guest(include_bytes!("../../examples/hello_world/hello_world.elf"));
}

#[test]
#[ignore = "requires CHERIOT_DEBUG_COMPARE, a canonical-board RTOS firmware path"]
fn supplied_guest_pause_schedule_preserves_complete_final_state() {
    let path = std::env::var_os("CHERIOT_DEBUG_COMPARE").expect("guest path");
    let firmware =
        cheriot_vm::input::read_file(path, cheriot_vm::input::MAX_FIRMWARE_BYTES, "firmware")
            .unwrap();
    compare_guest(&firmware);
}

fn compare_guest(firmware: &[u8]) {
    let configuration =
        MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml")).unwrap();
    let mut direct = configuration.build_cheriot1(firmware).unwrap();
    let mut debug = Target::new(
        MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))
            .unwrap()
            .build_cheriot1(firmware)
            .unwrap(),
        Some(100_000_000),
    );
    let mut direct_output = Vec::new();
    let mut debug_output = Vec::new();
    let mut seed = 0x4c47_2026_u32;
    debug.resume(None).unwrap();
    for step in 1..=100_000_000 {
        let expected_exit = direct.step().unwrap();
        let stop = debug.advance().unwrap();
        match stop {
            Some(StopReason::Completed(code)) => assert_eq!(expected_exit, Some(code)),
            Some(StopReason::Fault(_)) => debug.resume(None).unwrap(),
            None => assert_eq!(expected_exit, None),
            other => panic!("unexpected stop {other:?}"),
        }
        if step % 1000 == 0 || expected_exit.is_some() {
            direct.drain_output(&mut direct_output);
        }
        if step % 64 == 0 || expected_exit.is_some() {
            debug.drain_output(&mut debug_output);
        }
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        if expected_exit.is_some() {
            assert_eq!(expected_exit, Some(0));
            assert_eq!(direct_output, debug_output);
            let mut expected = Vec::new();
            let mut actual = Vec::new();
            direct.write_checkpoint(&mut expected).unwrap();
            debug.system().write_checkpoint(&mut actual).unwrap();
            assert_eq!(expected.len(), actual.len());
            assert!(
                expected == actual,
                "register/RAM/tag checkpoint diverged at completion"
            );
            return;
        }
        if seed & 255 == 0 {
            debug.halt(StopReason::Interrupted);
            assert_eq!(debug.advance().unwrap(), None);
            debug.resume(None).unwrap();
        }
    }
    panic!("guest did not finish within the comparison budget");
}

#[test]
fn cheriot_ebreak_is_distinct_from_completion_and_budget() {
    let mut system = target().into_system();
    system
        .computer
        .bus
        .install_firmware(0x8000_0000, &0x0010_0073_u32.to_le_bytes())
        .unwrap();
    let mut target = Target::new(system, Some(1));
    target.step(None).unwrap();
    assert_eq!(
        target.advance().unwrap(),
        Some(StopReason::Ebreak(0x8000_0000))
    );
    assert!(target
        .monitor(b"fault")
        .unwrap()
        .contains("cause=0x00000003"));
}

#[test]
fn removing_one_breakpoint_type_preserves_the_other_and_duplicate_requests_are_idempotent() {
    for remove_software in [false, true] {
        let mut target = target();
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
            Some(StopReason::Breakpoint(address))
        );
        let computer = &target.system().computer;
        assert_eq!(computer.attempted_steps, 0);
        assert_eq!(computer.processor.machine_registers.retired, 0);
        assert_eq!(computer.processor.program_counter, address);
    }
}

#[test]
fn retransmitted_breakpoint_insertions_need_only_one_removal() {
    for hardware in [false, true] {
        let mut target = target();
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
        let computer = &target.system().computer;
        assert_eq!(computer.attempted_steps, 1);
        assert_eq!(computer.processor.machine_registers.retired, 1);
    }
}
