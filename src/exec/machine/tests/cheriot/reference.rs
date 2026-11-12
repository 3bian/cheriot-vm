// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::Computer;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::cheriot::Capability;
use crate::exec::machine::memory::{MemoryKind, MemoryRegion, Tagging};

fn compare_instruction_vectors(vectors: &str) {
    let mut computer = None;
    let mut cases = 0;
    let mut checks = 0;
    let mut context = String::new();

    for line in vectors.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();

        if fields[0] == "CASE" {
            let index: usize = fields[1].parse().unwrap();
            assert_eq!(index, cases);
            cases += 1;
            let word: u32 = fields[2].parse().unwrap();
            context = format!("case {index}, word {word:08x}");
            let mut bus = Bus::new();
            bus.add_memory(
                MemoryRegion::new(
                    "ram".into(),
                    0x8000_0000,
                    0x4000,
                    MemoryKind::Ram,
                    Tagging::Cheriot,
                )
                .unwrap(),
            )
            .unwrap();
            bus.add_memory(
                MemoryRegion::new(
                    "shadow".into(),
                    0x8300_0000,
                    0x1000,
                    MemoryKind::Ram,
                    Tagging::Cheriot,
                )
                .unwrap(),
            )
            .unwrap();
            bus.install_firmware(0x8000_0000, &word.to_le_bytes())
                .unwrap();
            computer = Some(Computer::new(bus, 0x8000_0000));
            continue;
        }

        let computer = computer.as_mut().expect("CASE must precede state");
        let before = fields[1] == "before";
        let values: Vec<u64> = fields[2..]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();

        match fields[0] {
            "STATE" => {
                assert_eq!(values.len(), 8);

                if before {
                    computer.processor.program_counter = values[0] as u32;
                    computer.processor.machine_registers.machine_status = values[1] as u32;
                    computer.processor.machine_registers.trap_cause = values[2] as u32;
                    computer.processor.machine_registers.trap_value = values[3] as u32;
                    computer.processor.machine_registers.retired = values[4];
                    computer.processor.machine_registers.counter_inhibit = values[5] as u32;
                    computer.processor.stack_high_water = values[6] as u32;
                    computer.processor.stack_base = values[7] as u32;
                } else {
                    let outcome = computer.step();
                    let processor = &computer.processor;
                    assert_eq!(
                        [
                            u64::from(processor.program_counter),
                            u64::from(processor.machine_registers.machine_status),
                            u64::from(processor.machine_registers.trap_cause),
                            u64::from(processor.machine_registers.trap_value),
                            processor.machine_registers.retired,
                            u64::from(processor.machine_registers.counter_inhibit),
                            u64::from(processor.stack_high_water),
                            u64::from(processor.stack_base),
                        ],
                        values.as_slice(),
                        "{context}, {outcome:?}"
                    );
                    checks += 1;
                }
            }
            "CAP" => {
                assert_eq!(values.len(), 3);
                let index = values[0] as u8;
                let capability = Capability::from_memory(values[1], values[2] != 0);

                if before {
                    match index {
                        0..=15 => computer
                            .processor
                            .write_register(index, capability)
                            .unwrap(),
                        16 => computer.processor.pcc = capability,
                        17 => computer.processor.machine_trap = capability,
                        18 => computer.processor.machine_data = capability,
                        19 => computer.processor.machine_scratch = capability,
                        20 => computer.processor.machine_exception = capability,
                        _ => panic!("unknown capability register"),
                    }
                } else {
                    let actual = match index {
                        0..=15 => computer.processor.read_register(index).unwrap(),
                        16 => computer.processor.pcc,
                        17 => computer.processor.machine_trap,
                        18 => computer.processor.machine_data,
                        19 => computer.processor.machine_scratch,
                        20 => computer.processor.machine_exception,
                        _ => panic!("unknown capability register"),
                    };
                    assert_eq!(actual, capability, "{context}, capability {index}");
                    checks += 1;
                }
            }
            "MEM" => {
                assert_eq!(values.len(), 3);
                let address = values[0] as u32;
                let capability = Capability::from_memory(values[1], values[2] != 0);

                if before {
                    computer.bus.write_capability(address, capability).unwrap();
                } else {
                    assert_eq!(
                        computer.bus.read_capability(address).unwrap(),
                        capability,
                        "{context}, memory {address:x}"
                    );
                    checks += 1;
                }
            }
            _ => panic!("unexpected reference output: {line}"),
        }
    }

    assert_eq!(cases, 2304);
    assert_eq!(checks, cases * 26);
    println!(
        "Compared {cases} instruction cases: PC, CSRs, 21 capability registers, four memory granules and tags ({checks} checks)"
    );
}

#[test]
fn pinned_instruction_vectors() {
    compare_instruction_vectors(include_str!("../vectors/instruction-vectors.txt"));
}

#[test]
#[ignore = "opt-in: set SAIL_INSTRUCTION_VECTORS to vectors regenerated from Sail"]
fn regenerated_instruction_vectors() {
    compare_instruction_vectors(&super::reference_input::read_reference_text(
        "SAIL_INSTRUCTION_VECTORS",
    ));
}
