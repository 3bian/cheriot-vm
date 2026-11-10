// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{permissions, Capability};
use cheriot_vm::cheriot::{Computer, StepOutcome};
use cheriot_vm::devices::timer64::Timer64;
use cheriot_vm::devices::uart::{Uart, UartLayout};
use cheriot_vm::devices::{Device, InterruptLine};
use cheriot_vm::firmware::{load_cheriot_elf, load_elf};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::processor::ProcessorProfile;

const HELLO: &[u8] = include_bytes!("../../examples/hello_world/hello_world.elf");

#[test]
fn csr_legality_precedes_pcc_authority_and_preserves_destinations() {
    for word in [0xf110_1073, 0xfff0_1073, 0xc000_2173, 0xf110_91f3] {
        let mut machine = computer(&[word]);
        machine.processor.pcc = machine
            .processor
            .pcc
            .restrict_permissions(0xfff & !permissions::SYSTEM_REGISTERS);
        let original = memory_capability(0x8000_0100);
        machine.processor.write_register(3, original).unwrap();
        assert!(matches!(
            machine.step(),
            StepOutcome::Trapped {
                cause: 2,
                value: 0,
                ..
            }
        ));
        assert_eq!(machine.processor.read_register(3).unwrap(), original);
        assert_eq!(machine.processor.machine_registers.retired, 0);
    }
}

#[test]
fn trigger_probe_writes_retire_and_read_back_the_complement() {
    let mut machine = computer(&[0x7a00_1073, 0x7a00_21f3, 0x7a00_9073, 0x7a00_2273]);
    machine
        .processor
        .write_integer_register(1, 0x1234_5678)
        .unwrap();
    for _ in 0..4 {
        assert!(matches!(machine.step(), StepOutcome::Retired { .. }));
    }
    assert_eq!(
        machine.processor.read_register(3).unwrap().address(),
        u32::MAX
    );
    assert_eq!(
        machine.processor.read_register(4).unwrap().address(),
        0xedcb_a987
    );
    assert_eq!(machine.processor.trigger_select, 0x1234_5678);
}

#[test]
fn capability_memory_tags_survive_only_capability_stores() {
    let mut machine = computer(&[0x0020_b023, 0x0000_b183, 0x0000_8023, 0x0000_b203]);
    let value = memory_capability(0x8000_0100).set_bounds(32, true).0;
    machine
        .processor
        .write_register(1, memory_capability(0x8000_0200))
        .unwrap();
    machine.processor.write_register(2, value).unwrap();
    assert!(matches!(machine.step(), StepOutcome::Retired { .. }));
    assert_eq!(machine.bus.memory()[0].tag_at(0x8000_0200), Some(true));
    machine.step();
    assert_eq!(machine.processor.read_register(3).unwrap(), value);
    machine.step();
    assert_eq!(machine.bus.memory()[0].tag_at(0x8000_0200), Some(false));
    machine.step();
    assert!(!machine.processor.read_register(4).unwrap().tag());
}

#[test]
fn cheriot_uart_divisor_latch_and_clint_accesses_are_explicit() {
    let mut uart = Uart::with_layout(UartLayout::Ns16550);
    assert_eq!(uart.read(0x14, 4).unwrap(), 0x20);
    assert_eq!(uart.read(0, 4).unwrap(), 0);
    assert!(uart.read(0x14, 1).is_err());
    uart.write(0x0c, 4, 0x80).unwrap();
    uart.write(0, 4, b'X'.into()).unwrap();
    uart.write(0x0c, 4, 3).unwrap();
    uart.write(0, 4, b'A'.into()).unwrap();
    assert_eq!(uart.take_transmitted(), b"A");
    let mut clint = Timer64::new();
    clint.write(0x4000, 4, 5).unwrap();
    clint.write(0x4004, 4, 1).unwrap();
    clint.tick((1u64 << 32) + 4);
    assert_eq!(clint.read(0xbffc, 4).unwrap(), 1);
    assert_eq!(clint.read(0xbff8, 4).unwrap(), 4);
    assert!(!clint.interrupt_pending(InterruptLine::MachineTimer));
    clint.tick(1);
    clint.write(0, 4, 1).unwrap();
    assert!(clint.interrupt_pending(InterruptLine::MachineSoftware));
    assert!(clint.interrupt_pending(InterruptLine::MachineTimer));
    clint.write(0xbff8, 4, 0).unwrap();
    assert_eq!(clint.read(0xbff8, 4).unwrap(), 0);
    assert!(clint.read(0x4000, 2).is_err());
}

#[test]
fn uart_16550_input_is_bounded_polled_and_preserved_on_invalid_reads() {
    let mut console = Uart::with_layout(UartLayout::Ns16550);
    console.inject_receive(b"AB").unwrap();
    assert_eq!(console.read(0x14, 4).unwrap(), 0x21);
    assert!(console.inject_receive(&vec![0; 4095]).is_err());
    assert!(console.read(0, 1).is_err());
    console.write(0x0c, 4, 0x80).unwrap();
    assert!(console.read(0, 4).is_err());
    console.write(0x0c, 4, 3).unwrap();
    assert_eq!(console.read(0, 4).unwrap(), u32::from(b'A'));
    assert_eq!(console.read(0, 4).unwrap(), u32::from(b'B'));
    assert_eq!(console.read(0x14, 4).unwrap(), 0x20);
    assert_eq!(console.read(0, 4).unwrap(), 0);
}

fn computer(words: &[u32]) -> Computer {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "ram".into(),
            0x8000_0000,
            4096,
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
            4096,
            MemoryKind::Ram,
            Tagging::None,
        )
        .unwrap(),
    )
    .unwrap();
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(words);
    bus.install_firmware(0x8000_0000, &bytes).unwrap();
    let mut computer = Computer::new(bus, 0x8000_0000);
    computer.timing = cheriot_vm::TimingPolicy::PerStep;
    computer
}

#[test]
fn failed_capability_store_preserves_memory_and_registers() {
    let mut machine = computer(&[0x0020_b023]);
    let authority = memory_capability(0x8000_0200).restrict_permissions(permissions::STORE);
    machine.processor.write_register(1, authority).unwrap();
    machine
        .processor
        .write_register(2, memory_capability(0))
        .unwrap();
    machine
        .bus
        .install_firmware(0x8000_0200, &[0xa5; 8])
        .unwrap();
    assert_eq!(
        machine.step(),
        StepOutcome::Trapped {
            address: 0x8000_0000,
            cause: 28,
            value: 53
        }
    );
    assert_eq!(machine.bus.inspect(0x8000_0200, 4).unwrap(), 0xa5a5_a5a5);
    assert_eq!(machine.bus.inspect(0x8000_0204, 4).unwrap(), 0xa5a5_a5a5);
    assert_eq!(machine.processor.read_register(1).unwrap(), authority);
    assert_eq!(machine.processor.machine_exception.address(), 0x8000_0000);
}

#[test]
fn fetch_bounds_fault_precedes_alignment_and_clears_saved_authority() {
    let mut machine = computer(&[0x0000_0013]);
    machine.processor.pcc = machine
        .processor
        .pcc
        .set_address(0x8000_0000)
        .set_bounds(2, true)
        .0;
    machine.processor.program_counter = 0x8000_0003;

    assert_eq!(
        machine.step(),
        StepOutcome::Trapped {
            address: 0x8000_0003,
            cause: 28,
            value: 1025,
        }
    );
    assert_eq!(machine.processor.machine_exception.address(), 0x8000_0003);
    assert!(!machine.processor.machine_exception.tag());
    assert_eq!(machine.fetched_instructions, 0);
    assert_eq!(machine.processor.machine_registers.retired, 0);
}

#[test]
fn illegal_decoding_counts_as_fetched_but_not_retired() {
    let mut machine = computer(&[0x0001_0001; 50]);
    machine.bus.write(0x8000_00c6, 2, 0).unwrap();
    for _ in 0..99 {
        assert!(matches!(machine.step(), StepOutcome::Retired { .. }));
    }
    assert!(matches!(
        machine.step(),
        StepOutcome::Trapped { cause: 2, .. }
    ));
    assert_eq!(machine.fetched_instructions, 100);
    assert_eq!(machine.processor.machine_registers.retired, 99);
    assert_eq!(machine.processor.machine_registers.time, 100);
}

#[test]
fn integer_operations_destroy_capability_metadata() {
    let mut machine = computer(&[0x0000_8093, 0xfea1_825b]);
    machine
        .processor
        .write_register(1, memory_capability(0x1234))
        .unwrap();
    machine
        .processor
        .write_register(3, memory_capability(0x5678))
        .unwrap();
    machine.step();
    assert_eq!(
        machine.processor.read_register(1).unwrap(),
        Capability::integer(0x1234)
    );
    machine.step();
    assert_eq!(
        machine.processor.read_register(4).unwrap(),
        memory_capability(0x5678)
    );
    machine
        .processor
        .write_register(0, memory_capability(0))
        .unwrap();
    assert_eq!(
        machine.processor.read_register(0).unwrap(),
        Capability::integer(0)
    );
    assert!(machine
        .processor
        .write_register(16, Capability::integer(0))
        .is_err());
}

#[test]
fn loaders_require_their_own_abi_and_raw_firmware_cannot_create_tags() {
    let mut machine = computer(&[0x0000_b183]);
    let profile = ProcessorProfile {
        compressed: true,
        control_registers: true,
        embedded: true,
        multiply_divide: true,
    };
    assert!(load_elf(&mut machine.bus, HELLO, profile).is_err());
    let mut integer_elf = HELLO.to_vec();
    integer_elf[36..40].copy_from_slice(&9u32.to_le_bytes());
    assert!(load_cheriot_elf(&mut machine.bus, &integer_elf).is_err());
    machine
        .bus
        .install_firmware(0x8000_0200, &0x7e3e_0000_0000_0000u64.to_le_bytes())
        .unwrap();
    machine
        .processor
        .write_register(1, memory_capability(0x8000_0200))
        .unwrap();
    machine.step();
    assert!(!machine.processor.read_register(3).unwrap().tag());
}

fn memory_capability(address: u32) -> Capability {
    Capability::from_memory(0x7e3e_0000_0000_0000 | u64::from(address), true)
}

#[test]
fn revocation_uses_loaded_base_and_preserves_memory_payload_tag() {
    let mut machine = computer(&[0x0020_b023, 0x0000_b183]);
    let value = memory_capability(0x8000_0100)
        .set_bounds(32, true)
        .0
        .set_address(0x8000_0108);
    machine
        .processor
        .write_register(1, memory_capability(0x8000_0200))
        .unwrap();
    machine.processor.write_register(2, value).unwrap();
    machine.step();
    machine.bus.write(0x8300_0004, 1, 1).unwrap();
    machine.step();
    assert_eq!(
        machine.processor.read_register(3).unwrap(),
        value.clear_tag()
    );
    assert_eq!(machine.bus.memory()[0].tag_at(0x8000_0200), Some(true));
    assert_eq!(machine.bus.inspect(0x8000_0200, 4).unwrap(), 0x8000_0108);
}

#[test]
fn sentry_calls_and_returns_restore_interrupt_enable() {
    let mut machine = computer(&[
        0x0005_00e7,
        0x0000_0013,
        0x0000_0013,
        0x0000_0013,
        0x0000_8067,
    ]);
    machine.processor.machine_registers.machine_status |= 8;
    let sentry = Capability::from_memory(0x5e3e_0000_8000_0010 | (2 << 54), true);
    machine.processor.write_register(10, sentry).unwrap();
    machine.step();
    assert_eq!(machine.processor.program_counter, 0x8000_0010);
    assert_eq!(machine.processor.machine_registers.machine_status & 8, 0);
    assert_eq!(machine.processor.read_register(1).unwrap().object_type(), 5);
    assert_eq!(
        machine.processor.read_register(1).unwrap().address(),
        0x8000_0004
    );
    assert!(!machine.processor.pcc.sealed());
    machine.step();
    assert_eq!(machine.processor.program_counter, 0x8000_0004);
    assert_eq!(machine.processor.machine_registers.machine_status & 8, 8);
}

#[test]
fn software_interrupt_precedes_timer_and_does_not_retire() {
    let mut machine = computer(&[0x0000_0013]);
    machine
        .bus
        .add_device(
            "clint".into(),
            0x0200_0000,
            0xc0000,
            Box::new(Timer64::new()),
        )
        .unwrap();
    machine.bus.write(0x0200_0000, 4, 1).unwrap();
    machine.processor.machine_registers.machine_status |= 8;
    machine.processor.machine_registers.interrupt_enable = 0x88;
    assert_eq!(
        machine.step(),
        StepOutcome::Trapped {
            address: 0x8000_0000,
            cause: 0x8000_0003,
            value: 0
        }
    );
    assert_eq!(machine.fetched_instructions, 0);
    assert_eq!(machine.processor.machine_registers.retired, 0);
    assert_eq!(machine.processor.machine_registers.interrupt_pending, 0x88);
}

#[test]
fn unprivileged_capability_cannot_modify_system_registers() {
    let mut machine = computer(&[0x3400_91f3]);
    machine.processor.pcc = machine
        .processor
        .pcc
        .restrict_permissions(0xfff & !permissions::SYSTEM_REGISTERS);
    machine.processor.write_integer_register(1, 42).unwrap();
    machine.processor.write_integer_register(3, 99).unwrap();
    assert_eq!(
        machine.step(),
        StepOutcome::Trapped {
            address: 0x8000_0000,
            cause: 28,
            value: 1048
        }
    );
    assert_eq!(machine.processor.machine_registers.scratch, 0);
    assert_eq!(machine.processor.read_register(3).unwrap().address(), 99);
}

#[test]
fn wait_for_interrupt_does_not_jump_in_per_step_mode() {
    let mut words = vec![0x0001_0001; 128];
    words[0] = 0x1050_0073;
    let mut machine = computer(&words);
    machine
        .bus
        .add_device(
            "clint".into(),
            0x0200_0000,
            0xc0000,
            Box::new(Timer64::new()),
        )
        .unwrap();
    machine.bus.write(0x0200_4000, 4, 200).unwrap();
    machine.processor.machine_registers.machine_status |= 8;
    machine.processor.machine_registers.interrupt_enable = 0x80;
    machine.step();
    assert_eq!(machine.processor.machine_registers.time, 1);
    assert_eq!(machine.processor.machine_registers.cycle, 1);
    assert_eq!(machine.processor.machine_registers.interrupt_pending, 0);
    for _ in 1..200 {
        assert!(matches!(machine.step(), StepOutcome::Retired { .. }));
    }
    assert_eq!(machine.processor.machine_registers.time, 200);
    assert_eq!(machine.processor.machine_registers.interrupt_pending, 0x80);
    assert!(matches!(
        machine.step(),
        StepOutcome::Trapped {
            cause: 0x8000_0007,
            ..
        }
    ));
    assert_eq!(machine.fetched_instructions, 200);
}
