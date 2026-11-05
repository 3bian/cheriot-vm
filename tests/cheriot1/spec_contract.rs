// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot1::{Capability, Cheriot1System, StepOutcome};
use cheriot_vm::completion::CompletionPolicy;
use cheriot_vm::configuration::DefaultDevices;
use cheriot_vm::devices::timer64::Timer64;
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::devices::{Device, InterruptLine};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::TimingPolicy;

const BASE: u32 = 0x8000_0000;
const CANARY_INSTRUCTIONS: [u32; 4] = [0x0062_a023, 0x0064_a023, 0x0004_2383, 0x0064_2023];
const CHERIOT_EXCEPTION_CONTRACT: &str = "https://github.com/CHERIoT-Platform/cheriot-sail/blob/b9e7ab7dc9a3d13c045410c2431977237548d92f/archdoc/chap-cheri-riscv.tex#L180-L204";
const DECLARED_MISA: u32 = 0x4080_1014;
const MEMORY_ROOT_BITS: u64 = 0x7e3e_0000_0000_0000;
const MISA_CONTRACT: &str = "https://github.com/riscv/riscv-isa-manual/blob/418bdc4990ae2d60a86f1e11e8d7795aa279da61/src/priv/machine.adoc#L18-L151";
const REPEATED_TRAP_ATTEMPTS: u64 = 64;
const TIMER: u32 = 0x0200_0000;
const TIMINGS: [TimingPolicy; 2] = [TimingPolicy::Cheriot1Compatible, TimingPolicy::PerStep];
const TOHOST: u32 = BASE + 0x100;
const UART: u32 = 0x2000_0000;

fn assert_counters(system: &mut Cheriot1System, steps: u64, fetched: u64, retired: u64) {
    let computer = &mut system.computer;
    let ticks = match computer.timing {
        TimingPolicy::Cheriot1Compatible => fetched / 100,
        TimingPolicy::PerStep => steps,
    };
    assert_eq!(computer.attempted_steps, steps);
    assert_eq!(computer.fetched_instructions, fetched);
    assert_eq!(computer.processor.machine_registers.retired, retired);
    assert_eq!(computer.processor.machine_registers.cycle, ticks);
    assert_eq!(computer.processor.machine_registers.time, ticks);
    assert_eq!(computer.bus.read(TIMER + 0xbff8, 4).unwrap(), ticks as u32);
    assert_eq!(computer.bus.read(TIMER + 0xbffc, 4).unwrap(), 0);
}

fn assert_no_guest_effects(
    system: &Cheriot1System,
    registers: &[Capability; 16],
    memory: &[(u32, Option<bool>)],
) {
    let computer = &system.computer;
    assert_eq!(computer.processor.registers(), registers);
    assert_eq!(memory_contents(&computer.bus), memory);
    assert_eq!(computer.last_store, None);
    assert_eq!(computer.processor.machine_registers.counter_inhibit, 0);
    assert_eq!(
        computer.processor.machine_registers.instruction_alignment,
        2
    );
    assert_eq!(computer.processor.machine_registers.interrupt_enable, 0);
    assert_eq!(
        computer.processor.machine_registers.interrupt_pending,
        0x800
    );
    assert_eq!(computer.processor.machine_registers.scratch, 0);
    assert_eq!(computer.processor.machine_registers.trap_vector, 0);
    assert_eq!(
        computer.processor.machine_data,
        Capability::from_memory(0x7e3e_0000_0000_0000, true)
    );
    assert_eq!(
        computer.processor.machine_scratch,
        Capability::from_memory(0x4e3e_0000_0000_0000, true)
    );
    assert_eq!(computer.processor.stack_base, 0);
    assert_eq!(computer.processor.stack_high_water, 0);
    assert_eq!(computer.processor.trigger_select, 0);
    assert!(computer
        .bus
        .interrupt_pending(InterruptLine::MachineExternal));
    assert!(!computer.bus.interrupt_pending(InterruptLine::MachineTimer));
    assert!(!computer
        .bus
        .interrupt_pending(InterruptLine::MachineSoftware));
    assert!(computer
        .bus
        .device::<Uart>("uart")
        .unwrap()
        .transmitted()
        .is_empty());
}

fn memory_contents(bus: &Bus) -> Vec<(u32, Option<bool>)> {
    (BASE..BASE + 512)
        .step_by(4)
        .map(|address| {
            (
                bus.inspect(address, 4).unwrap(),
                bus.memory()[0].tag_at(address),
            )
        })
        .collect()
}

fn system(words: &[u32], timing: TimingPolicy) -> Cheriot1System {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), BASE, 512, MemoryKind::Ram, Tagging::Cheriot1).unwrap(),
    )
    .unwrap();
    let mut timer = Timer64::new();
    timer.write(0x4000, 4, u32::MAX).unwrap();
    timer.write(0x4004, 4, u32::MAX).unwrap();
    bus.add_device("timer".into(), TIMER, 0xc0000, Box::new(timer))
        .unwrap();
    let mut uart = Uart::new();
    uart.inject_receive(b"retained input").unwrap();
    uart.write(8, 4, 1).unwrap();
    bus.add_device("uart".into(), UART, 12, Box::new(uart))
        .unwrap();
    bus.write(TOHOST, 4, 1).unwrap();
    bus.write_capability(
        BASE + 0x120,
        Capability::from_memory(0x7e3e_0000_8000_0120, true),
    )
    .unwrap();
    let bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let mut system = Cheriot1System::from_raw_with_policy(
        bus,
        BASE,
        &bytes,
        DefaultDevices::Configured,
        CompletionPolicy::Address(TOHOST),
    )
    .unwrap();
    system.computer.timing = timing;
    for index in 1..16 {
        system
            .computer
            .processor
            .write_register(
                index,
                Capability::from_memory(
                    MEMORY_ROOT_BITS | u64::from(BASE + 8 * u32::from(index)),
                    true,
                ),
            )
            .unwrap();
    }
    assert_eq!(system.host_interface.as_ref().unwrap().address(), TOHOST);
    system
}

fn system_with_handlers(timing: TimingPolicy) -> Cheriot1System {
    let mut system = system(&[0x03c6_805b, 0x0000_0073, 0x0000_0013], timing);
    let handler: Vec<_> = CANARY_INSTRUCTIONS
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    system
        .computer
        .bus
        .install_firmware(BASE + 0x40, &handler)
        .unwrap();
    for (index, bits, tag) in [
        (5, MEMORY_ROOT_BITS | u64::from(TOHOST), true),
        (6, 1, false),
        (8, MEMORY_ROOT_BITS | u64::from(UART), true),
        (9, MEMORY_ROOT_BITS | u64::from(BASE + 0x120), true),
    ] {
        system
            .computer
            .processor
            .write_register(index, Capability::from_memory(bits, tag))
            .unwrap();
    }
    system
}

#[test]
fn declared_misa_is_fixed_across_all_six_fetched_csr_forms() {
    assert_eq!(DECLARED_MISA >> 30, 1, "RV32 MXL: {MISA_CONTRACT}");
    for extension in [2, 4, 12, 23] {
        assert_ne!(
            DECLARED_MISA & (1 << extension),
            0,
            "C/E/M/X: {MISA_CONTRACT}"
        );
    }
    assert_eq!(
        DECLARED_MISA & ((1 << 1) | (1 << 8)),
        0,
        "No B/I claim: {MISA_CONTRACT}"
    );
    let mut cases = 0;
    for timing in TIMINGS {
        for function in [1, 2, 3, 5, 6, 7] {
            let immediate = function >= 5;
            let sources: &[u32] = if immediate {
                &[0, 1, 2, 16, 31]
            } else {
                &[0, 1, 15]
            };
            let values: &[u32] = if immediate {
                &[0]
            } else {
                &[0, 2, DECLARED_MISA, u32::MAX]
            };
            for &source in sources {
                for &value in values {
                    for destination in [0, 3] {
                        let encoded = (0x301 << 20)
                            | (source << 15)
                            | (function << 12)
                            | (destination << 7)
                            | 0x73;
                        let readback = 0x3010_2273;
                        let mut system = system(&[encoded, readback], timing);
                        if !immediate && source != 0 {
                            system
                                .computer
                                .processor
                                .write_register(
                                    source as u8,
                                    Capability::from_memory(
                                        MEMORY_ROOT_BITS | u64::from(value),
                                        true,
                                    ),
                                )
                                .unwrap();
                        }
                        let mut expected_registers = *system.computer.processor.registers();
                        let memory = memory_contents(&system.computer.bus);
                        let pcc = system.computer.processor.pcc;
                        let exception = system.computer.processor.machine_exception;
                        let trap = system.computer.processor.machine_trap;
                        for (index, word) in [encoded, readback].into_iter().enumerate() {
                            let report = system.step_report().unwrap();
                            assert_eq!(report.outcome, StepOutcome::Retired { address: BASE + 4 * index as u32, encoded: word }, "function={function}, source={source}, destination={destination}, value={value:#x}, timing={timing:?}; {MISA_CONTRACT}");
                            assert_eq!(report.completion, None);
                            let written = if index == 0 { destination as usize } else { 4 };
                            if written != 0 {
                                expected_registers[written] =
                                    Capability::from_memory(u64::from(DECLARED_MISA), false);
                            }
                            assert_no_guest_effects(&system, &expected_registers, &memory);
                            assert_eq!(
                                system.computer.processor.machine_registers.isa, DECLARED_MISA,
                                "Fixed WARL selection: {MISA_CONTRACT}"
                            );
                            assert_eq!(
                                system.computer.processor.program_counter,
                                BASE + 4 * (index + 1) as u32
                            );
                            assert_eq!(system.computer.processor.pcc, pcc);
                            assert_eq!(system.computer.processor.machine_exception, exception);
                            assert_eq!(system.computer.processor.machine_trap, trap);
                            assert_eq!(
                                system.computer.processor.machine_registers.machine_status,
                                0x1800
                            );
                            assert_eq!(system.computer.processor.machine_registers.trap_cause, 0);
                            assert_eq!(system.computer.processor.machine_registers.trap_value, 0);
                            assert_counters(
                                &mut system,
                                index as u64 + 1,
                                index as u64 + 1,
                                index as u64 + 1,
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 204);
}

#[test]
fn tagged_handler_positive_controls_expose_the_forbidden_effects() {
    let mut cases = 0;
    for timing in TIMINGS {
        for (index, encoded) in CANARY_INSTRUCTIONS.into_iter().enumerate() {
            let mut system = system_with_handlers(timing);
            let address = BASE + 0x40 + 4 * index as u32;
            let attempts = if index == 0 { 3 } else { 1 };
            for attempt in 1..=attempts {
                system.computer.processor.pcc =
                    Capability::from_memory(0x5e3e_0000_0000_0000 | u64::from(address), true);
                system.computer.processor.program_counter = address;
                let report = system.step_report().unwrap();
                assert_eq!(report.outcome, StepOutcome::Retired { address, encoded });
                assert_eq!(report.completion, (index == 0 && attempt == 3).then_some(0));
                assert_counters(&mut system, attempt, attempt, attempt);
            }
            match index {
                0 => assert_eq!(system.computer.last_store, Some((TOHOST, 4))),
                1 => {
                    assert_eq!(system.computer.last_store, Some((BASE + 0x120, 4)));
                    assert_eq!(system.computer.bus.inspect(BASE + 0x120, 4).unwrap(), 1);
                    assert_eq!(
                        system.computer.bus.memory()[0].tag_at(BASE + 0x120),
                        Some(false)
                    );
                }
                2 => {
                    assert_eq!(system.computer.last_store, None);
                    assert_eq!(
                        system
                            .computer
                            .processor
                            .read_register(7)
                            .unwrap()
                            .to_memory(),
                        (u64::from(b'r'), false)
                    );
                    assert_eq!(system.computer.bus.read(UART, 4).unwrap(), u32::from(b'e'));
                }
                3 => {
                    assert_eq!(system.computer.last_store, Some((UART, 4)));
                    assert_eq!(system.take_output(), [1]);
                }
                _ => unreachable!(),
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 8);
}

#[test]
fn untagged_mtcc_repeats_precise_fetch_traps_within_a_finite_budget() {
    let mut repeated_traps = 0;
    let cases = [
        (
            "non-executable unmapped",
            0x7e3e_0000_0000_0000,
            true,
            0x7e3e_0000_0000_0000,
        ),
        (
            "non-executable completion store",
            0x7e3e_0000_8000_0040,
            true,
            0x7e3e_0000_8000_0040,
        ),
        (
            "misaligned bit zero tag store",
            0x5e3e_0000_8000_0045,
            true,
            0x5e3e_0000_8000_0044,
        ),
        (
            "misaligned bit one tag store",
            0x5e3e_0000_8000_0046,
            true,
            0x5e3e_0000_8000_0044,
        ),
        (
            "sealed input load",
            0x5e7e_0000_8000_0048,
            true,
            0x5e7e_0000_8000_0048,
        ),
        (
            "already untagged output store",
            0x5e3e_0000_8000_004c,
            false,
            0x5e3e_0000_8000_004c,
        ),
    ];
    for timing in TIMINGS {
        for (name, source_bits, source_tag, expected_bits) in cases {
            let mut system = system_with_handlers(timing);
            system
                .computer
                .processor
                .write_register(13, Capability::from_memory(source_bits, source_tag))
                .unwrap();
            system.computer.processor.machine_registers.machine_status = 0x1808;
            let registers = *system.computer.processor.registers();
            let memory = memory_contents(&system.computer.bus);
            let data = system.computer.processor.machine_data;
            let scratch = system.computer.processor.machine_scratch;
            let expected_mtcc = Capability::from_memory(expected_bits, false);
            let handler_address = expected_bits as u32;
            let write = system.step_report().unwrap();
            assert_eq!(
                write.outcome,
                StepOutcome::Retired {
                    address: BASE,
                    encoded: 0x03c6_805b
                },
                "{name}: {CHERIOT_EXCEPTION_CONTRACT}"
            );
            assert_eq!(write.completion, None);
            assert_eq!(
                system.computer.processor.machine_trap, expected_mtcc,
                "{name}: {CHERIOT_EXCEPTION_CONTRACT}"
            );
            assert_eq!(
                system.computer.processor.machine_registers.machine_status,
                0x1808
            );
            assert_no_guest_effects(&system, &registers, &memory);
            assert_counters(&mut system, 1, 1, 1);
            let ecall = system.step_report().unwrap();
            assert_eq!(
                ecall.outcome,
                StepOutcome::Trapped {
                    address: BASE + 4,
                    cause: 11,
                    value: 0
                }
            );
            assert_eq!(ecall.completion, None);
            assert_eq!(
                system.computer.processor.machine_exception,
                Capability::from_memory(0x5e3e_0000_8000_0004, true)
            );
            assert_eq!(system.computer.processor.pcc, expected_mtcc);
            assert_eq!(system.computer.processor.program_counter, handler_address);
            assert_eq!(
                system.computer.processor.machine_registers.machine_status,
                0x1880
            );
            assert_eq!(system.computer.processor.machine_registers.trap_cause, 11);
            assert_eq!(system.computer.processor.machine_registers.trap_value, 0);
            assert_eq!(
                system
                    .computer
                    .processor
                    .machine_registers
                    .exception_program_counter,
                BASE + 4
            );
            assert_no_guest_effects(&system, &registers, &memory);
            assert_counters(&mut system, 2, 2, 1);
            for attempt in 1..=REPEATED_TRAP_ATTEMPTS {
                let report = system.step_report().unwrap();
                assert_eq!(
                    report.outcome,
                    StepOutcome::Trapped {
                        address: handler_address,
                        cause: 28,
                        value: 0x402
                    },
                    "{name}, attempt={attempt}, timing={timing:?}; {CHERIOT_EXCEPTION_CONTRACT}"
                );
                assert_eq!(report.completion, None);
                let processor = &system.computer.processor;
                assert_eq!(processor.machine_trap, expected_mtcc);
                assert_eq!(processor.pcc, expected_mtcc);
                assert_eq!(processor.program_counter, handler_address);
                assert_eq!(processor.machine_exception, expected_mtcc);
                assert_eq!(processor.machine_data, data);
                assert_eq!(processor.machine_scratch, scratch);
                assert_eq!(processor.machine_registers.machine_status, 0x1800);
                assert_eq!(
                    processor.machine_registers.exception_program_counter,
                    handler_address
                );
                assert_eq!(processor.machine_registers.trap_cause, 28);
                assert_eq!(processor.machine_registers.trap_value, 0x402);
                assert_eq!(processor.machine_registers.isa, DECLARED_MISA);
                assert_no_guest_effects(&system, &registers, &memory);
                assert_counters(&mut system, attempt + 2, 2, 1);
                repeated_traps += 1;
            }
            assert!(system.take_output().is_empty());
            for expected in b"retained input" {
                assert_eq!(
                    system.computer.bus.read(UART, 4).unwrap(),
                    u32::from(*expected)
                );
            }
            assert_eq!(system.computer.bus.read(UART + 4, 4).unwrap(), 2);
            assert_eq!(system.computer.bus.read(UART + 8, 4).unwrap(), 1);
        }
    }
    assert_eq!(repeated_traps, 768);
}
