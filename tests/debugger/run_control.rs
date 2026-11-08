// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use crate::memory::{MemoryKind, MemoryRegion, Tagging};

fn regions(mappings: &[(u32, usize, bool)]) -> Bus {
    let mut bus = Bus::new();
    for (index, (address, size, executable)) in mappings.iter().copied().enumerate() {
        let mut region = MemoryRegion::new(
            format!("memory{index}"),
            address,
            size,
            MemoryKind::Ram,
            Tagging::None,
        )
        .unwrap();
        region.executable = executable;
        bus.add_memory(region).unwrap();
    }
    bus
}

#[test]
fn breakpoint_accepts_adjacent_executable_instruction_parcels() {
    for mappings in [
        vec![(0x1000, 0x2000, true)],
        vec![(0x1000, 0x1000, true), (0x2000, 0x1000, true)],
    ] {
        let bus = regions(&mappings);
        let mut control = RunControl::new(Some(10));
        for kind in [BreakpointType::Software, BreakpointType::Hardware] {
            assert!(control.insert_breakpoint(&bus, kind, 0x1ffe, 4, 2));
            assert_eq!(
                control.before_step(0x1ffe, 0),
                Some(StopReason::Breakpoint(0x1ffe, kind))
            );
            assert!(control.remove_breakpoint(kind, 0x1ffe, 4, 2));
        }
    }
    let bus = regions(&[(0x1000, 2, true), (0x1002, 2, true)]);
    let mut control = RunControl::new(None);
    for kind in [BreakpointType::Software, BreakpointType::Hardware] {
        assert!(control.insert_breakpoint(&bus, kind, 0x1000, 4, 4));
        assert!(!control.insert_breakpoint(&bus, kind, 0x1000, 2, 4));
        assert!(!control.insert_breakpoint(&bus, kind, 0x1002, 4, 4));
    }
    let mut bus = regions(&[(0x1000, 2, true)]);
    bus.add_memory(
        MemoryRegion::new("rom".into(), 0x1002, 2, MemoryKind::Rom, Tagging::None).unwrap(),
    )
    .unwrap();
    assert!(RunControl::new(None).insert_breakpoint(&bus, BreakpointType::Software, 0x1000, 4, 4));
}

#[test]
fn breakpoint_rejects_gaps_non_executable_parcels_and_address_wrap() {
    for mappings in [
        vec![(0x1000, 0x1000, true), (0x2002, 0x0ffe, true)],
        vec![(0x1000, 0x1000, true), (0x2000, 0x1000, false)],
        vec![(0x1000, 0x1000, false), (0x2000, 0x1000, true)],
        vec![(0x1ffe, 1, true), (0x1fff, 3, true)],
    ] {
        let bus = regions(&mappings);
        let mut control = RunControl::new(None);
        for kind in [BreakpointType::Software, BreakpointType::Hardware] {
            assert!(!control.insert_breakpoint(&bus, kind, 0x1ffe, 4, 2));
        }
    }
    let bus = regions(&[(0xffff_fffe, 2, true), (0, 2, true)]);
    let mut control = RunControl::new(None);
    for kind in [BreakpointType::Software, BreakpointType::Hardware] {
        assert!(!control.insert_breakpoint(&bus, kind, 0xffff_fffe, 4, 2));
        assert!(control.insert_breakpoint(&bus, kind, 0xffff_fffe, 2, 2));
        assert!(!control.insert_breakpoint(&bus, kind, 0xffff_ffff, 2, 2));
        assert!(!control.insert_breakpoint(&bus, kind, 0xffff_fffe, 3, 2));
    }
    let bus = regions(&[(0xffff_fffc, 4, true)]);
    assert!(RunControl::new(None).insert_breakpoint(
        &bus,
        BreakpointType::Hardware,
        0xffff_fffc,
        4,
        4
    ));
}

#[test]
fn breakpoint_identity_capacity_and_width_are_checked_independently() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("code".into(), 0x1000, 4096, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();
    let mut control = RunControl::new(None);
    for index in 0..64 {
        assert!(control.insert_breakpoint(
            &bus,
            BreakpointType::Software,
            0x1000 + index * 4,
            4,
            2
        ));
    }
    assert!(control.insert_breakpoint(&bus, BreakpointType::Software, 0x1000, 4, 2));
    assert!(!control.insert_breakpoint(&bus, BreakpointType::Hardware, 0x1000, 4, 2));
    assert!(control.remove_breakpoint(BreakpointType::Hardware, 0x1000, 4, 2));
    assert_eq!(
        control.before_step(0x1000, 0),
        Some(StopReason::Breakpoint(0x1000, BreakpointType::Software))
    );
    assert!(control.remove_breakpoint(BreakpointType::Software, 0x1000, 2, 2));
    assert_eq!(
        control.before_step(0x1000, 0),
        Some(StopReason::Breakpoint(0x1000, BreakpointType::Software))
    );
    assert!(control.remove_breakpoint(BreakpointType::Software, 0x1000, 4, 2));
    assert!(control.insert_breakpoint(&bus, BreakpointType::Hardware, 0x1000, 2, 2));
    assert!(!control.insert_breakpoint(&bus, BreakpointType::Software, 0x1000, 1, 2));
    assert!(!control.remove_breakpoint(BreakpointType::Hardware, 0x1001, 2, 2));
    assert_eq!(
        control.before_step(0x1000, 0),
        Some(StopReason::Breakpoint(0x1000, BreakpointType::Hardware))
    );
    assert!(control.remove_breakpoint(BreakpointType::Hardware, 0x1000, 2, 2));
    assert_eq!(control.before_step(0x1000, 0), None);
}

#[test]
fn software_breakpoints_take_precedence_and_both_types_bypass_once() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("code".into(), 0x1000, 32, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();
    let mut control = RunControl::new(Some(1));
    assert!(control.insert_breakpoint(&bus, BreakpointType::Hardware, 0x1000, 4, 2));
    assert!(control.insert_breakpoint(&bus, BreakpointType::Software, 0x1000, 2, 2));
    let software = StopReason::Breakpoint(0x1000, BreakpointType::Software);
    assert_eq!(control.before_step(0x1000, 0), Some(software));
    let mut state = RunState::Halted;
    control
        .resume(&mut state, software, RunState::Continuing, None)
        .unwrap();
    assert_eq!(control.before_step(0x1000, 0), None);
    assert_eq!(control.before_step(0x1000, 0), Some(software));
    assert!(control.remove_breakpoint(BreakpointType::Software, 0x1000, 2, 2));
    let hardware = StopReason::Breakpoint(0x1000, BreakpointType::Hardware);
    assert_eq!(control.before_step(0x1000, 0), Some(hardware));
    state = RunState::Halted;
    control
        .resume(&mut state, hardware, RunState::SingleStep, None)
        .unwrap();
    assert_eq!(control.before_step(0x1000, 0), None);
    assert_eq!(control.before_step(0x1000, 1), Some(hardware));
    assert!(control.remove_breakpoint(BreakpointType::Hardware, 0x1000, 4, 2));
    assert_eq!(control.before_step(0x1000, 1), Some(StopReason::RunLimit));
}

#[test]
fn synchronous_faults_stop_and_interrupts_continue() {
    assert_eq!(
        StopReason::from_fault(0x1000, 3),
        Some(StopReason::Ebreak(0x1000))
    );
    assert_eq!(
        StopReason::from_fault(0x1000, 5),
        Some(StopReason::Fault(5))
    );
    assert_eq!(StopReason::from_fault(0x1000, 0x8000_0007), None);
}
