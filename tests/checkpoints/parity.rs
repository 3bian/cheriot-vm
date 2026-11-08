// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::Cell;
use std::io::{self, Write};
use std::rc::Rc;

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{Capability, CheriotSystem};
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::devices::Device;
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::processor::ProcessorProfile;
use cheriot_vm::rv32::Processor;
use cheriot_vm::{Computer, EmulatorError, MachineConfiguration, TimingPolicy};

const ENTRY: u32 = 0x8000_0000;
const PROFILES: [&str; 9] = [
    "rv32e", "rv32em", "rv32ec", "rv32emc", "rv32i", "rv32im", "rv32ic", "rv32imc", "cheriot",
];

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ObservedDevice(Rc<Cell<usize>>);

impl Device for ObservedDevice {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn read(&mut self, _offset: u32, _width: usize) -> Result<u32, EmulatorError> {
        self.0.set(self.0.get() + 1);
        Ok(0)
    }

    fn tick(&mut self, _ticks: u64) {
        self.0.set(self.0.get() + 1);
    }

    fn write(&mut self, _offset: u32, _width: usize, _value: u32) -> Result<(), EmulatorError> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

#[test]
fn all_profiles_and_timing_policies_record_real_registers_counters_and_layout() {
    for (profile_id, name) in PROFILES.iter().enumerate() {
        for (timing_id, timing) in [(0, TimingPolicy::PerStep), (2, TimingPolicy::Elapsed)] {
            let memory = if *name == "cheriot" {
                "cheriot-memory"
            } else {
                "memory"
            };
            let mut text = format!(
                "[machine]\nname='snapshot'\n\
                 [processor]\ntype='{name}'\n\
                 [[{memory}]]\nname='ram0'\ntype='ram'\nbase={ENTRY}\nsize=128\n"
            );
            if *name == "cheriot" {
                text.push_str(
                    "[[cheriot-memory]]\nname='bitmap'\ntype='ram'\nbase=0x83000000\nsize=0x100000\n",
                );
            }
            let mut configuration = MachineConfiguration::parse(&text).unwrap();
            configuration.timing = timing;
            configuration.steps_per_tick = 1;
            let mut bytes = Vec::new();
            if *name == "cheriot" {
                let mut system = configuration
                    .build_cheriot_raw(ENTRY, &0x0010_8093_u32.to_le_bytes())
                    .unwrap();
                system
                    .computer
                    .processor
                    .write_integer_register(1, 41)
                    .unwrap();
                system.step().unwrap();
                system.write_checkpoint(&mut bytes).unwrap();
                let register_start = 37 + 17 * 8 + 5 * 8;
                assert_eq!(word(&bytes, register_start + 16), 42);
                assert_eq!(word(&bytes, register_start + 24), 0);
                assert_eq!(bytes[13], 16);
            } else {
                let mut computer = configuration.build(ENTRY).unwrap();
                computer
                    .bus
                    .install_firmware(ENTRY, &0x0010_8093_u32.to_le_bytes())
                    .unwrap();
                computer.processor.write_register(1, 41).unwrap();
                computer.step().unwrap();
                computer.write_checkpoint(&mut bytes).unwrap();
                assert_eq!(word(&bytes, 37 + 17 * 8 + 8), 42);
                assert_eq!(bytes[13], 8);
            }
            assert_eq!(&bytes[..8], b"CHKP0004");
            assert_eq!(bytes[8], u8::from(*name == "cheriot"));
            assert_eq!(bytes[9], profile_id as u8);
            assert_eq!(bytes[11], timing_id as u8);
            assert_eq!(
                bytes[12],
                if (4..8).contains(&profile_id) { 32 } else { 16 }
            );
            assert_eq!(&bytes[16..20], &31_u32.to_le_bytes());
            assert_eq!(&bytes[20..24], &2_000_u32.to_le_bytes());
            assert_eq!(word(&bytes, 37), 1);
            assert_eq!(word(&bytes, 45), 1);
            assert_eq!(word(&bytes, 53), u64::from(ENTRY + 4));
            assert_eq!(word(&bytes, 61), 1);
            assert_eq!(word(&bytes, 69), u64::from(timing_id == 0));
            assert_eq!(word(&bytes, 77), 1);
        }
    }
}

#[test]
fn capability_snapshot_preserves_payload_tags_and_legacy_trace_bytes() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), ENTRY, 128, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
    )
    .unwrap();
    let mut system =
        CheriotSystem::from_raw_on_bus(bus, ENTRY, &0x0000_0013_u32.to_le_bytes()).unwrap();
    system.computer.timing = TimingPolicy::PerStep;
    let payload = 0x7e3e_0000_8000_0040;
    system
        .computer
        .processor
        .write_register(15, Capability::from_memory(payload, true))
        .unwrap();
    let mut snapshot = Vec::new();
    let mut legacy = Vec::new();
    let mut state = Vec::new();
    system.write_checkpoint(&mut snapshot).unwrap();
    system.write_checkpoint_v1(&mut legacy).unwrap();
    system.write_state(&mut state).unwrap();
    assert_eq!(word(&snapshot, 37 + 17 * 8 + 5 * 8 + 15 * 16), payload);
    assert_eq!(word(&snapshot, 37 + 17 * 8 + 5 * 8 + 15 * 16 + 8), 1);
    assert_eq!(&legacy[..8], b"CHKP0001");
    assert_eq!(&legacy[8..8 + state.len()], state);
}

#[test]
fn legacy_checkpoint_rejects_unrepresentable_frequency() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), ENTRY, 128, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
    )
    .unwrap();
    let mut system =
        CheriotSystem::from_raw_on_bus(bus, ENTRY, &0x0000_0013_u32.to_le_bytes()).unwrap();
    system.computer.timing = TimingPolicy::PerStep;
    system.computer.set_timer_hz(12_345).unwrap();
    let mut legacy = Vec::new();
    assert!(system.write_checkpoint_v1(&mut legacy).is_err());
    assert!(legacy.is_empty());
    let mut current = Vec::new();
    system.write_checkpoint(&mut current).unwrap();
    assert_eq!(&current[..8], b"CHKP0004");
    assert_eq!(&current[20..24], &12_345_u32.to_le_bytes());
}

#[test]
fn integer_snapshot_records_explicit_tag_storage_without_capability_registers() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), ENTRY, 8, MemoryKind::Ram, Tagging::Cheriot).unwrap(),
    )
    .unwrap();
    let computer = Computer::new(
        Processor::new(
            ProcessorProfile {
                compressed: false,
                control_registers: false,
                embedded: true,
                multiply_divide: false,
            },
            ENTRY,
        ),
        bus,
    );
    let mut snapshot = Vec::new();
    computer.write_checkpoint(&mut snapshot).unwrap();
    assert_eq!(&snapshot[8..11], &[0, 0, 1]);
    assert_eq!(snapshot[13], 8);
    let mapping = 37 + 17 * 8 + 16 * 8 + 8;
    assert_eq!(snapshot[mapping + 4 + 3 + 14], 1);
    assert_eq!(snapshot.last(), Some(&0));
}

#[test]
fn snapshot_observation_does_not_poll_devices_consume_input_or_swallow_write_errors() {
    let mut bus = Bus::new();
    let calls = Rc::new(Cell::new(0));
    bus.add_device(
        "observed".into(),
        0x1000,
        256,
        Box::new(ObservedDevice(calls.clone())),
    )
    .unwrap();
    let mut uart = Uart::new();
    uart.inject_receive(b"Q").unwrap();
    bus.add_device("uart".into(), 0x2000, 256, Box::new(uart))
        .unwrap();
    let computer = Computer::new(
        Processor::new(
            ProcessorProfile {
                compressed: false,
                control_registers: true,
                embedded: false,
                multiply_divide: false,
            },
            ENTRY,
        ),
        bus,
    );
    let mut computer = computer;
    let mut first = Vec::new();
    let mut second = Vec::new();
    computer.write_checkpoint(&mut first).unwrap();
    computer.write_checkpoint(&mut second).unwrap();
    assert_eq!(first, second);
    assert_eq!(calls.get(), 0);
    assert_eq!(computer.bus.read(0x2000, 4).unwrap(), u32::from(b'Q'));
    assert_eq!(
        computer
            .write_checkpoint(&mut FailingWriter)
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}

fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[test]
fn deterministic_quantum_and_fraction_are_part_of_both_snapshot_layouts() {
    for profile in ["rv32e", "rv32imc", "cheriot"] {
        let text = include_str!("../../profiles/cheriot.toml")
            .replace("type = \"cheriot\"", &format!("type = \"{profile}\""));
        let configuration = MachineConfiguration::parse(&text).unwrap();
        let mut bytes = Vec::new();
        if configuration.is_cheriot() {
            let mut system = configuration
                .build_cheriot_raw(ENTRY, &0x00000013_u32.to_le_bytes())
                .unwrap();
            system.step().unwrap();
            system.write_checkpoint(&mut bytes).unwrap();
            let mut legacy = Vec::new();
            assert!(system.write_checkpoint_v1(&mut legacy).is_err());
            assert!(legacy.is_empty());
        } else {
            let mut computer = configuration
                .build_raw(ENTRY, &0x00000013_u32.to_le_bytes())
                .unwrap();
            computer.step().unwrap();
            computer.write_checkpoint(&mut bytes).unwrap();
        }
        assert_eq!(&bytes[..8], b"CHKP0004");
        assert_eq!(&bytes[24..28], &100_u32.to_le_bytes());
        assert_eq!(&bytes[28..32], &1_u32.to_le_bytes());
        assert_eq!(word(&bytes, 37), 1);
        assert_eq!(word(&bytes, 61), 0);
        assert_eq!(word(&bytes, 69), 0);
    }
}
