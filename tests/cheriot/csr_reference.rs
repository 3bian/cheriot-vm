// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{permissions, Capability, Computer, StepOutcome};
use crate::bus::Bus;
use crate::memory::{MemoryKind, MemoryRegion, Tagging};

fn compare_csr_vectors(vectors: &str) {
    let mut cases = 0;
    for line in vectors.lines() {
        let mut fields = line.split_whitespace();
        assert_eq!(fields.next(), Some("CSR"));
        let values: Vec<u64> = fields.map(|value| value.parse().unwrap()).collect();
        assert_eq!(values.len(), 24);
        let word = values[0] as u32;
        let mut bus = Bus::new();
        bus.add_memory(
            MemoryRegion::new(
                "ram".into(),
                0x8000_0000,
                0x1000,
                MemoryKind::Ram,
                Tagging::Cheriot,
            )
            .unwrap(),
        )
        .unwrap();
        bus.install_firmware(0x8000_0000, &word.to_le_bytes())
            .unwrap();
        let mut computer = Computer::new(bus, 0x8000_0000);
        computer.timing = crate::TimingPolicy::PerStep;
        let processor = &mut computer.processor;
        processor.machine_registers.cycle = 0x1_0000_0002;
        processor.machine_registers.retired = 0x3_0000_0004;
        processor.machine_registers.scratch = 0x1234_5678;
        processor.stack_high_water = 0x8000_3000;
        processor.stack_base = 0x8000_2000;
        processor.trigger_select = values[3] as u32;
        processor.machine_registers.interrupt_pending = values[4] as u32;
        if values[4] != 0 {
            processor.machine_registers.interrupt_enable = 0x222;
            processor.machine_registers.machine_status = 0x1808;
        }
        if values[2] == 0 {
            processor.pcc = processor
                .pcc
                .restrict_permissions(0xfff & !permissions::SYSTEM_REGISTERS);
        }
        processor
            .write_integer_register(1, values[1] as u32)
            .unwrap();
        processor
            .write_integer_register(15, values[1] as u32)
            .unwrap();
        processor
            .write_register(3, Capability::from_memory(0x7e3e_0000_0000_0000, true))
            .unwrap();
        let outcome = computer.step();
        let processor = &computer.processor;
        let machine = &processor.machine_registers;
        let destination = processor
            .read_register(((word >> 7) & 31) as u8)
            .unwrap()
            .to_memory();
        let pcc = processor.pcc.to_memory();
        let mepcc = processor.machine_exception.to_memory();
        let actual = [
            u64::from(processor.program_counter),
            u64::from(machine.machine_status),
            u64::from(machine.trap_cause),
            u64::from(machine.trap_value),
            machine.retired,
            machine.cycle,
            u64::from(machine.counter_inhibit),
            u64::from(machine.interrupt_enable),
            u64::from(machine.interrupt_pending),
            u64::from(machine.scratch),
            u64::from(processor.stack_high_water),
            u64::from(processor.stack_base),
            u64::from(processor.trigger_select),
            destination.0,
            u64::from(destination.1),
            pcc.0,
            u64::from(pcc.1),
            mepcc.0,
            u64::from(mepcc.1),
        ];
        // The reference used its old instruction-based clock. Keep every CSR
        // comparison, accounting separately for this VM's per-step cycle rule.
        let mut expected = values[5..].to_vec();
        let function = (word >> 12) & 7;
        let source = (word >> 15) & 31;
        let writes =
            matches!(function, 1 | 5) || (matches!(function, 2 | 3 | 6 | 7) && source != 0);
        let writes_cycle = matches!(outcome, StepOutcome::Retired { .. })
            && matches!((word >> 20) & 0xfff, 0xb00 | 0xb80)
            && writes;
        if !writes_cycle {
            expected[5] = expected[5].wrapping_add(1);
        }
        assert_eq!(&actual[..], expected, "case {cases}, word={word:#010x}, source={}, authority={}, initial tselect={:#x}, {outcome:?}", values[1], values[2], values[3]);
        cases += 1;
    }
    assert_eq!(cases, 8479);
    println!("Compared {cases} independently generated CSR cases and 19 post-step fields each");
}

#[test]
fn pinned_csr_vectors() {
    compare_csr_vectors(include_str!("../vectors/csr-vectors.txt"));
}

#[test]
#[ignore = "requires fresh CSR vectors from the pinned Sail reference"]
fn regenerated_csr_vectors() {
    compare_csr_vectors(&super::reference_input::read_reference_text(
        "SAIL_CSR_VECTORS",
    ));
}
