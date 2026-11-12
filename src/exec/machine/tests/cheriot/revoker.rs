// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{Capability, CapabilityFaultCause, CheriotSystem, Computer, Fault, StepOutcome};
use crate::exec::machine::MachineConfiguration;

const PROFILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/profiles/cheriot.toml"
));

const PROGRAM: u32 = 0x8000_0000;
const FREED: u32 = 0x8000_1000;
const HOLDER: u32 = 0x8000_2000;
const REVOKER: u32 = 0x1000_4000;
const CONTROLLER: u32 = 0x0c00_0000;
const REVOKER_SOURCE: u32 = 7;

const BITMAP_BYTE: u32 = 9;
const DEVICE: u32 = 10;
const HOLDER_POINTER: u32 = 11;
const LOADED: u32 = 12;
const RANGE_BASE: u32 = 13;
const RANGE_TOP: u32 = 14;
const SCRATCH: u32 = 15;

const BYTE: u32 = 0;
const WORD: u32 = 2;
const CAPABILITY: u32 = 3;

fn load(width: u32, destination: u32, base: u32, offset: u32) -> u32 {
    (offset << 20) | (base << 15) | (width << 12) | (destination << 7) | 0x03
}

fn store(width: u32, source: u32, base: u32, offset: u32) -> u32 {
    ((offset >> 5) << 25)
        | (source << 20)
        | (base << 15)
        | (width << 12)
        | ((offset & 31) << 7)
        | 0x23
}

fn and_immediate(destination: u32, source: u32, immediate: u32) -> u32 {
    (immediate << 20) | (source << 15) | (7 << 12) | (destination << 7) | 0x13
}

fn loop_while_set(register: u32, bytes: u32) -> u32 {
    let offset = 0u32.wrapping_sub(bytes) & 0x1fff;
    ((offset >> 12) << 31)
        | (((offset >> 5) & 0x3f) << 25)
        | (register << 15)
        | (1 << 12)
        | (((offset >> 1) & 0xf) << 8)
        | (((offset >> 11) & 1) << 7)
        | 0x63
}

const NO_OPERATION: u32 = 0x0000_0013;

fn program(sweep: bool) -> Vec<u8> {
    [
        store(WORD, RANGE_BASE, DEVICE, 0x00),
        store(WORD, RANGE_TOP, DEVICE, 0x04),
        if sweep {
            store(WORD, 0, DEVICE, 0x08)
        } else {
            NO_OPERATION
        },
        load(WORD, SCRATCH, DEVICE, 0x0c),
        and_immediate(SCRATCH, SCRATCH, 1),
        loop_while_set(SCRATCH, 8),
        store(BYTE, 0, BITMAP_BYTE, 0),
        load(CAPABILITY, LOADED, HOLDER_POINTER, 0),
        load(WORD, SCRATCH, LOADED, 0),
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect()
}

fn machine(sweep: bool) -> CheriotSystem {
    let mut system = MachineConfiguration::parse(PROFILE)
        .unwrap()
        .build_cheriot_raw(PROGRAM, &program(sweep))
        .unwrap();
    let memory = Capability::from_memory(0x7e3e_0000_0000_0000, true);
    let bus = &mut system.computer.bus;

    let pointer = memory.set_address(FREED).set_bounds(64, true).0;
    bus.write_capability(HOLDER, pointer).unwrap();
    bus.write(FREED, 4, 0x5ec2e7).unwrap();
    let (offset, bit) = pointer
        .revocation_bit(Computer::REVOCABLE_MEMORY_BASE)
        .unwrap();
    let mark = Computer::REVOCATION_BITMAP_BASE + offset;
    bus.write(mark, 1, u32::from(bit)).unwrap();

    let processor = &mut system.computer.processor;
    for (register, value) in [
        (BITMAP_BYTE, memory.set_address(mark)),
        (DEVICE, memory.set_address(REVOKER)),
        (HOLDER_POINTER, memory.set_address(HOLDER)),
        (RANGE_BASE, Capability::integer(HOLDER - 0x100)),
        (RANGE_TOP, Capability::integer(HOLDER + 0x100)),
    ] {
        processor.write_register(register as u8, value).unwrap();
    }
    system
}

fn run_to_the_use(system: &mut CheriotSystem) -> StepOutcome {
    let last = PROGRAM + 8 * 4;
    for _ in 0..100_000 {
        let at = system.computer.processor.program_counter;
        system.step().unwrap();
        let outcome = system.last_outcome.unwrap();
        if at == last {
            return outcome;
        }
        assert!(
            matches!(outcome, StepOutcome::Retired { .. }),
            "unexpected {outcome:?}"
        );
    }
    panic!("the program never reached its last instruction");
}

#[test]
fn a_pointer_left_in_memory_faults_when_used_after_a_sweep() {
    let mut system = machine(true);
    let outcome = run_to_the_use(&mut system);
    let expected = Fault::capability_cause(CapabilityFaultCause::Tag, LOADED as u8);
    assert_eq!(
        outcome,
        StepOutcome::Trapped {
            address: PROGRAM + 8 * 4,
            cause: expected.cause,
            value: expected.value,
        }
    );
    assert!(system.computer.attempted_steps > 100);
    assert_eq!(system.computer.bus.read(REVOKER + 0x0c, 4).unwrap(), 2);
    assert!(!system.computer.bus.read_capability(HOLDER).unwrap().tag());
}

#[test]
fn without_a_sweep_the_same_pointer_still_works() {
    let mut system = machine(false);
    let outcome = run_to_the_use(&mut system);
    assert!(
        matches!(outcome, StepOutcome::Retired { .. }),
        "{outcome:?}"
    );
    let value = system.computer.processor.read_register(SCRATCH as u8);
    assert_eq!(value.unwrap().address(), 0x5ec2e7);
    assert_eq!(system.computer.bus.read(REVOKER + 0x0c, 4).unwrap(), 0);
}

#[test]
fn the_end_of_a_sweep_interrupts_the_processor_once() {
    let mut system = machine(true);
    let computer = &mut system.computer;
    computer
        .bus
        .write(CONTROLLER + 4, 4, 1 << REVOKER_SOURCE)
        .unwrap();
    computer.bus.write(REVOKER + 0x14, 4, 1).unwrap();
    computer.processor.machine_registers.interrupt_enable = 1 << 11;
    computer.processor.machine_registers.machine_status |= 1 << 3;

    let mut interrupted_at = None;
    for _ in 0..1_000 {
        system.step().unwrap();
        if let Some(StepOutcome::Trapped { cause, .. }) = system.last_outcome {
            assert_eq!(cause, 0x8000_000b, "a machine external interrupt");
            interrupted_at = Some(system.computer.attempted_steps);
            break;
        }
    }
    assert!(
        interrupted_at.is_some(),
        "the sweep ended without an interrupt"
    );
    let bus = &mut system.computer.bus;
    assert_eq!(bus.read(REVOKER + 0x0c, 4).unwrap(), 2);
    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), REVOKER_SOURCE);
    bus.write(CONTROLLER + 8, 4, REVOKER_SOURCE).unwrap();
    bus.refresh_interrupts();
    assert_eq!(bus.read(CONTROLLER + 8, 4).unwrap(), 0, "it fired twice");
}
