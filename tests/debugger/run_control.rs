// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
use crate::memory::{MemoryKind, MemoryRegion, Tagging};

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
        Some(StopReason::Breakpoint(0x1000))
    );
    assert!(control.remove_breakpoint(BreakpointType::Software, 0x1000, 2, 2));
    assert_eq!(
        control.before_step(0x1000, 0),
        Some(StopReason::Breakpoint(0x1000))
    );
    assert!(control.remove_breakpoint(BreakpointType::Software, 0x1000, 4, 2));
    assert!(control.insert_breakpoint(&bus, BreakpointType::Hardware, 0x1000, 2, 2));
    assert!(!control.insert_breakpoint(&bus, BreakpointType::Software, 0x1000, 1, 2));
    assert!(!control.remove_breakpoint(BreakpointType::Hardware, 0x1001, 2, 2));
    assert_eq!(
        control.before_step(0x1000, 0),
        Some(StopReason::Breakpoint(0x1000))
    );
    assert!(control.remove_breakpoint(BreakpointType::Hardware, 0x1000, 2, 2));
    assert_eq!(control.before_step(0x1000, 0), None);
}
