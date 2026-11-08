// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::io::BufRead;

#[cfg(feature = "verification")]
use super::RegisterAccess;
use super::{Capability, Computer, StepOutcome};
use crate::bus::Bus;
use crate::memory::{MemoryKind, MemoryRegion, Tagging};

const CASE_COUNT: usize = 65_536 * 8;

fn initial_computer(encoded: u32, variant: usize) -> Computer {
    let mut bus = Bus::new();
    for (name, base, size) in [
        ("ram", 0x8000_0000, 0x2400),
        ("shadow", 0x8300_0000, 0x1000),
    ] {
        bus.add_memory(
            MemoryRegion::new(name.into(), base, size, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
        )
        .unwrap();
    }
    bus.install_firmware(0x8000_0000, &encoded.to_le_bytes())
        .unwrap();
    let sentinel = Capability::from_memory(0x7e00400080002000, true);
    bus.write_capability(0x8000_2000, sentinel).unwrap();
    bus.write_capability(0x8000_2008, sentinel).unwrap();
    if variant == 4 {
        bus.write(0x8300_0080, 1, 1).unwrap();
    }
    let mut machine = Computer::new(bus, 0x8000_0000);
    machine.timing = crate::TimingPolicy::PerStep;
    machine.processor.machine_registers.machine_status = 0x1808;
    machine.processor.machine_registers.retired = 7;
    machine.processor.machine_registers.counter_inhibit = if variant == 7 { 4 } else { 0 };
    machine.processor.stack_high_water = 0x8000_2400;
    machine.processor.stack_base = 0x8000_2000;
    let bits = [
        0,
        0x7e3e000080002000,
        0x7e00100080002000,
        0x7e3e000080002000,
        0x5ebe000080002000,
        0x7e7e000080002000,
        0x663e000080002000,
        u64::MAX,
    ][variant];
    let source = Capability::from_memory(bits, !matches!(variant, 0 | 3 | 7));
    for index in 1..16 {
        machine.processor.write_register(index, source).unwrap();
    }
    machine
}

fn compare_compressed_vectors(input: impl BufRead, strict_reference: bool) {
    let mut lines = input.lines();
    assert_eq!(
        lines.next().expect("missing reference header").unwrap(),
        "COMPRESSED 1 65536 8"
    );
    let mut failures = [0_usize; 2];
    let mut examples = BTreeMap::new();
    let mut partitions: BTreeMap<&str, [usize; 3]> = BTreeMap::new();
    for case in 0..CASE_COUNT {
        let line = lines
            .next()
            .expect("truncated compressed reference")
            .unwrap();
        let mut fields = line.split_whitespace();
        assert_eq!(fields.next(), Some("CASE"), "case {case}");
        let values: Vec<u64> = fields
            .map(|value| value.parse().expect("invalid reference number"))
            .collect();
        assert_eq!(values.len(), 54, "case {case}: wrong state layout");
        assert_eq!(values[0], case as u64);
        assert_eq!(values[1], (case / 8) as u64);
        assert_eq!(values[2], (case % 8) as u64);
        let encoded = values[1] as u32;
        let mut machine = initial_computer(encoded, case % 8);
        let class = encoding_class(encoded);
        partitions.entry(class).or_default()[0] += 1;
        let before = (class != "shared-comparison").then(|| {
            (
                capability_state(&machine),
                scalar_state(&machine),
                memory_state(&machine),
            )
        });
        let outcome = machine.step();
        if let Some((capabilities, scalars, memory)) = before {
            if !policy_matches(&machine, outcome, class, capabilities, scalars, &memory) {
                partitions.get_mut(class).unwrap()[2] += 1;
                if partitions[class][2] <= 4 {
                    eprintln!("Contract policy failed: case {case}, encoded {encoded:04x}, class {class}, {outcome:?}");
                }
            }
        }
        let context = format!("case {case}, encoded {encoded:04x}, {outcome:?}");
        let mut differences = Vec::new();
        macro_rules! compare_state {
            ($actual:expr, $expected:expr, $label:expr) => {{
                let actual = &$actual;
                let expected = &$expected;
                if actual != expected {
                    differences.push(format!(
                        "{context}, {}: actual {actual:?}, expected {expected:?}",
                        $label
                    ));
                }
            }};
        }
        compare_state!(machine.fetched_instructions, values[3], "fetched");
        let processor = &machine.processor;
        compare_state!(
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
            values[4..12],
            "scalar state"
        );
        let capabilities = processor.registers().iter().copied().chain([
            processor.pcc,
            processor.machine_trap,
            processor.machine_data,
            processor.machine_scratch,
            processor.machine_exception,
        ]);
        for (index, actual) in capabilities.enumerate() {
            assert!(values[13 + index * 2] <= 1, "{context}, invalid tag");
            let expected =
                Capability::from_memory(values[12 + index * 2], values[13 + index * 2] != 0);
            compare_state!(actual, expected, format!("capability {index}"));
        }
        let mut expected_memory = BTreeMap::new();
        loop {
            let line = lines
                .next()
                .expect("truncated compressed memory state")
                .unwrap();
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.first().copied() {
                Some("MEM") => {
                    assert_eq!(fields.len(), 4, "{context}");
                    let cell: usize = fields[1].parse().unwrap();
                    assert!(cell < 128, "{context}, invalid memory cell");
                    let bits: u64 = fields[2].parse().unwrap();
                    let tag: u8 = fields[3].parse().unwrap();
                    assert!(tag <= 1, "{context}, invalid memory tag");
                    let expected = Capability::from_memory(bits, tag != 0);
                    assert_ne!(
                        expected,
                        Capability::integer(0),
                        "{context}, noncanonical empty memory cell"
                    );
                    assert!(
                        expected_memory.insert(cell, expected).is_none(),
                        "{context}, duplicate memory cell"
                    );
                }
                Some("END") => {
                    assert_eq!(fields.len(), 3, "{context}");
                    assert_eq!(
                        fields[1].parse::<usize>().unwrap(),
                        expected_memory.len(),
                        "{context}"
                    );
                    assert_eq!(fields[2], "128", "{context}");
                    break;
                }
                _ => panic!("{context}, invalid memory record"),
            }
        }
        for cell in 0..128 {
            let address = 0x8000_2000 + cell as u32 * 8;
            let expected = expected_memory
                .get(&cell)
                .copied()
                .unwrap_or(Capability::integer(0));
            compare_state!(
                machine.bus.read_capability(address).unwrap(),
                expected,
                format!("memory {address:08x}")
            );
        }
        if !differences.is_empty() {
            failures[usize::from(encoded & 3 == 3)] += 1;
            partitions.get_mut(class).unwrap()[1] += 1;
            let group = (encoded & 3, encoded >> 13);
            examples
                .entry(group)
                .or_insert_with(|| differences.join("\n"));
        }
    }
    assert_eq!(
        lines.next().expect("missing completion count").unwrap(),
        "COMPLETE 524288"
    );
    assert!(
        lines.next().is_none(),
        "unexpected trailing reference records"
    );
    println!(
        "Compared {CASE_COUNT} independent Sail states; mismatching compressed states: {}, zero-extended 32-bit states: {}",
        failures[0], failures[1]
    );
    for (group, example) in examples {
        eprintln!("First mismatch in quadrant/function group {group:?}:\n{example}");
    }
    for (class, counts) in &partitions {
        println!(
            "Partition {class}: cases={}, mismatches={}, policy_failures={}",
            counts[0], counts[1], counts[2]
        );
    }
    assert_eq!(
        partitions
            .iter()
            .map(|(class, counts)| (*class, counts[0]))
            .collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            ("shared-comparison", 415_400),
            ("hint", 2_904),
            ("reserved-register", 93_696),
            ("custom-shift", 12_288),
        ]),
        "incomplete compressed classification"
    );
    let policy_failures: usize = partitions.values().map(|counts| counts[2]).sum();
    assert_eq!(policy_failures, 0, "compressed product contract violated");
    if strict_reference {
        assert_eq!(
            failures,
            [0, 0],
            "independent compressed campaign disagreed"
        );
        println!("Matched {CASE_COUNT} independent Sail states: all 65536 halfwords, 8 initial states, 21 capabilities, scalars, 128 RAM/tag cells each.");
    } else {
        assert_eq!(
            partitions["shared-comparison"][1], 0,
            "unclassified reference disagreement"
        );
        assert_eq!(partitions["hint"][1], 0, "HINT reference disagreement");
        println!("Verified compressed contract: cases={CASE_COUNT}, shared_mismatches=0, hint_mismatches=0, policy_failures=0");
    }
}

#[test]
#[ignore = "requires freshly generated, pinned Sail compressed states"]
fn regenerated_compressed_vectors() {
    let file = super::reference_input::open_reference("SAIL_COMPRESSED_VECTORS");
    compare_compressed_vectors(std::io::BufReader::new(file), true);
}

#[test]
#[should_panic(expected = "missing reference header")]
fn empty_compressed_reference_fails() {
    compare_compressed_vectors(std::io::Cursor::new(""), true);
}

#[test]
#[should_panic(expected = "truncated compressed reference")]
fn truncated_compressed_reference_fails() {
    compare_compressed_vectors(std::io::Cursor::new("COMPRESSED 1 65536 8\n"), true);
}

#[test]
fn reserved_register_and_custom_shift_encodings_preserve_declared_policy() {
    for (encoded, variant) in [(0x0803, 0), (0x4802, 0), (0x9001, 1)] {
        let mut machine = initial_computer(encoded, variant);
        let before = *machine.processor.registers();
        assert!(matches!(
            machine.step(),
            super::StepOutcome::Trapped {
                cause: 2,
                value: 0,
                ..
            }
        ));
        assert_eq!(machine.processor.registers(), &before);
        assert_eq!(machine.processor.machine_registers.retired, 7);
    }
}

#[test]
#[ignore = "requires freshly generated, pinned Sail compressed states"]
#[cfg(feature = "verification")]
fn regenerated_compressed_contract() {
    let file = super::reference_input::open_reference("SAIL_COMPRESSED_VECTORS");
    compare_compressed_vectors(std::io::BufReader::new(file), false);
}

fn encoding_class(encoded: u32) -> &'static str {
    let quadrant = encoded & 3;
    let function = encoded >> 13;
    let destination = (encoded >> 7) & 31;
    let second = (encoded >> 2) & 31;
    let immediate = ((encoded >> 12) & 1) * 32 + second;
    let high_shift = encoded & 0x1000 != 0
        && ((quadrant == 1 && function == 4 && ((encoded >> 10) & 3) < 2)
            || (quadrant == 2 && function == 0));
    if high_shift {
        return "custom-shift";
    }
    let hint = match (quadrant, function) {
        (1, 0) => destination == 0 || immediate == 0,
        (1, 2) => destination == 0,
        (1, 3) => destination == 0 && immediate != 0,
        (1, 4) => ((encoded >> 10) & 3) < 2 && immediate == 0,
        (2, 0) => destination == 0 || second == 0,
        (2, 4) => destination == 0 && second != 0,
        _ => false,
    };
    if hint {
        return "hint";
    }
    let unavailable = match (quadrant, function) {
        (1, 0 | 2 | 3) | (2, 0 | 2 | 3) => destination >= 16,
        (2, 4) => destination >= 16 || second >= 16,
        (2, 6 | 7) => second >= 16,
        (3, _) => {
            matches!(
                encoded & 0x7f,
                0x03 | 0x13 | 0x17 | 0x33 | 0x37 | 0x5b | 0x67 | 0x6f | 0x73 | 0x7b
            ) && destination >= 16
        }
        _ => false,
    };
    if unavailable {
        "reserved-register"
    } else {
        "shared-comparison"
    }
}

fn capability_state(machine: &Computer) -> [Capability; 21] {
    let processor = &machine.processor;
    let mut values = [Capability::integer(0); 21];
    values[..16].copy_from_slice(processor.registers());
    values[16..].copy_from_slice(&[
        processor.pcc,
        processor.machine_trap,
        processor.machine_data,
        processor.machine_scratch,
        processor.machine_exception,
    ]);
    values
}

fn scalar_state(machine: &Computer) -> [u64; 18] {
    let processor = &machine.processor;
    let registers = &processor.machine_registers;
    [
        u64::from(processor.program_counter),
        u64::from(registers.machine_status),
        u64::from(registers.trap_cause),
        u64::from(registers.trap_value),
        registers.retired,
        u64::from(registers.counter_inhibit),
        u64::from(processor.stack_high_water),
        u64::from(processor.stack_base),
        registers.cycle,
        registers.time,
        u64::from(registers.exception_program_counter),
        u64::from(registers.interrupt_enable),
        u64::from(registers.interrupt_pending),
        u64::from(registers.isa),
        u64::from(registers.scratch),
        u64::from(registers.trap_vector),
        u64::from(registers.instruction_alignment),
        u64::from(processor.trigger_select),
    ]
}

fn memory_state(machine: &Computer) -> Vec<Capability> {
    machine
        .bus
        .memory()
        .iter()
        .flat_map(|region| {
            (0..region.size()).step_by(8).map(|offset| {
                region
                    .inspect_capability(region.base + offset as u32)
                    .unwrap()
            })
        })
        .collect()
}

fn policy_matches(
    machine: &Computer,
    outcome: StepOutcome,
    class: &str,
    mut capabilities: [Capability; 21],
    mut scalars: [u64; 18],
    memory: &[Capability],
) -> bool {
    if !matches!(class, "hint" | "reserved-register" | "custom-shift") {
        return false;
    }
    let expected_outcome = if class == "hint" {
        scalars[0] = 0x8000_0002;
        scalars[4] = if scalars[5] & 4 == 0 { 8 } else { 7 };
        matches!(
            outcome,
            StepOutcome::Retired {
                address: 0x8000_0000,
                ..
            }
        )
    } else {
        capabilities[16] = capabilities[17];
        capabilities[20] = Capability::from_memory(0x5e3e_0000_8000_0000, true);
        scalars[0] = 0;
        scalars[1] = 0x1880;
        scalars[2] = 2;
        scalars[3] = 0;
        scalars[10] = 0x8000_0000;
        matches!(
            outcome,
            StepOutcome::Trapped {
                address: 0x8000_0000,
                cause: 2,
                value: 0
            }
        )
    };
    scalars[8] = scalars[8].wrapping_add(1);
    scalars[9] = scalars[9].wrapping_add(1);
    #[cfg(feature = "verification")]
    if !machine.memory_accesses().is_empty()
        || machine
            .processor
            .register_accesses()
            .iter()
            .any(|event| class == "hint" || matches!(event, RegisterAccess::Write { .. }))
    {
        return false;
    }
    expected_outcome
        && machine.attempted_steps == 1
        && machine.fetched_instructions == 1
        && machine.last_store.is_none()
        && capability_state(machine) == capabilities
        && scalar_state(machine) == scalars
        && memory_state(machine) == memory
}

#[test]
fn normative_partition_covers_every_halfword_and_preserves_operand_roles() {
    let mut counts = BTreeMap::new();
    for encoded in 0..=u16::MAX {
        *counts
            .entry(encoding_class(u32::from(encoded)))
            .or_insert(0) += 1;
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("shared-comparison", 51_925),
            ("hint", 363),
            ("reserved-register", 11_712),
            ("custom-shift", 1_536)
        ])
    );
    for (encoded, expected) in [
        (0x0803, "reserved-register"),
        (0x4802, "reserved-register"),
        (0x6802, "reserved-register"),
        (0xc042, "reserved-register"),
        (0xe042, "reserved-register"),
        (0x9001, "custom-shift"),
        (0x9401, "custom-shift"),
        (0x1002, "custom-shift"),
        (0x0081, "hint"),
        (0x0082, "hint"),
        (0x8001, "hint"),
        (0x8401, "hint"),
        (0x0801, "hint"),
        (0x8042, "hint"),
        (0x3fff, "shared-comparison"),
        (0xbfff, "shared-comparison"),
        (0x07ff, "shared-comparison"),
        (0x4782, "shared-comparison"),
    ] {
        assert_eq!(encoding_class(encoded), expected, "encoding {encoded:04x}");
    }
}

#[test]
fn contract_policy_rejects_corrupted_fault_hint_and_memory_states() {
    for encoded in [0x0803, 0x4802, 0x9001, 0x0081] {
        for mutation in 0..12 {
            let mut machine = initial_computer(encoded, 1);
            let capabilities = capability_state(&machine);
            let scalars = scalar_state(&machine);
            let memory = memory_state(&machine);
            let outcome = machine.step();
            let class = encoding_class(encoded);
            assert!(policy_matches(
                &machine,
                outcome,
                class,
                capabilities,
                scalars,
                &memory
            ));
            match mutation {
                0 => machine.processor.program_counter ^= 2,
                1 => machine.processor.machine_registers.trap_cause ^= 1,
                2 => machine.processor.machine_registers.retired += 1,
                3 => machine
                    .processor
                    .write_register(3, machine.processor.read_register(3).unwrap().clear_tag())
                    .unwrap(),
                4 => machine.bus.write(0x8000_2000, 1, 0).unwrap(),
                5 => machine
                    .bus
                    .write_capability(0x8000_2008, Capability::integer(0))
                    .unwrap(),
                6 => machine.processor.machine_exception = Capability::integer(0),
                7 => machine.processor.pcc = Capability::integer(0),
                8 => machine.processor.machine_registers.machine_status ^= 0x80,
                9 => machine.processor.stack_high_water ^= 8,
                10 => machine.bus.write(0x8300_0000, 1, 1).unwrap(),
                _ => machine.last_store = Some((0x8000_2000, 4)),
            }
            assert!(
                !policy_matches(&machine, outcome, class, capabilities, scalars, &memory),
                "mutation {mutation} escaped for {encoded:04x}"
            );
        }
    }
}

#[test]
#[cfg(feature = "verification")]
fn contract_policy_rejects_forbidden_events_without_state_differences() {
    for encoded in [0x0803, 0x0081] {
        let mut machine = initial_computer(encoded, 1);
        let capabilities = capability_state(&machine);
        let scalars = scalar_state(&machine);
        let memory = memory_state(&machine);
        let outcome = machine.step();
        let class = encoding_class(encoded);
        let value = machine.processor.registers()[3];
        machine.processor.begin_observation();
        if class == "hint" {
            machine.processor.read_register(3).unwrap();
        } else {
            machine.processor.write_register(3, value).unwrap();
        }
        machine.processor.end_observation();
        assert!(!policy_matches(
            &machine,
            outcome,
            class,
            capabilities,
            scalars,
            &memory
        ));
    }
    let mut expected = initial_computer(0x0803, 1);
    let capabilities = capability_state(&expected);
    let scalars = scalar_state(&expected);
    let memory = memory_state(&expected);
    let outcome = expected.step();
    let mut machine = initial_computer(0x0803, 1);
    machine.step_injected(0x0000_a083);
    assert_eq!(machine.memory_accesses().len(), 1);
    let expected_capabilities = capability_state(&expected);
    let expected_scalars = scalar_state(&expected);
    machine.processor = expected.processor;
    assert_eq!(capability_state(&machine), expected_capabilities);
    assert_eq!(scalar_state(&machine), expected_scalars);
    assert_eq!(memory_state(&machine), memory);
    assert!(!policy_matches(
        &machine,
        outcome,
        "reserved-register",
        capabilities,
        scalars,
        &memory
    ));
}
