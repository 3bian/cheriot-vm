// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{Capability, CheriotSystem};
use cheriot_vm::completion::{CompletionPolicy, HostInterface};
use cheriot_vm::configuration::DefaultDevices;
use cheriot_vm::execution::TrapPolicy;
use cheriot_vm::execution::{ExecutionReport, StopReason};
use cheriot_vm::memory::{MemoryKind, MemoryRegion, Tagging};
use cheriot_vm::{Computer, MachineConfiguration, TimingPolicy};

const BASE: u32 = 0x8000_0000;
const TOHOST: u32 = BASE + 0x1000;
const PROFILES: [&str; 9] = [
    "rv32e", "rv32em", "rv32ec", "rv32emc", "rv32i", "rv32im", "rv32ic", "rv32imc", "cheriot",
];
const TIMINGS: [TimingPolicy; 2] = [TimingPolicy::PerStep, TimingPolicy::Elapsed];

enum Engine {
    Cheriot(Box<CheriotSystem>),
    Integer(Box<Computer>),
}

impl Engine {
    fn bus(&mut self) -> &mut Bus {
        match self {
            Self::Cheriot(system) => &mut system.computer.bus,
            Self::Integer(computer) => &mut computer.bus,
        }
    }

    fn read(&self, index: u8) -> u32 {
        match self {
            Self::Cheriot(system) => system
                .computer
                .processor
                .read_register(index)
                .unwrap()
                .address(),
            Self::Integer(computer) => computer.processor.read_register(index).unwrap(),
        }
    }

    fn report(&self) -> ExecutionReport {
        match self {
            Self::Cheriot(system) => system.execution_report(StopReason::BudgetExhausted),
            Self::Integer(computer) => computer.execution_report(StopReason::BudgetExhausted),
        }
    }

    fn set_register(&mut self, index: u8, address: u32, authority: bool) {
        match self {
            Self::Cheriot(system) => {
                let value = if authority {
                    Capability::from_memory(0x7e3e_0000_0000_0000 | u64::from(address), true)
                } else {
                    Capability::integer(address)
                };
                system
                    .computer
                    .processor
                    .write_register(index, value)
                    .unwrap();
            }
            Self::Integer(computer) => computer.processor.write_register(index, address).unwrap(),
        }
    }

    fn step(&mut self) -> (bool, Option<u64>) {
        match self {
            Self::Cheriot(system) => {
                let report = system.step_report().unwrap();
                (
                    matches!(
                        report.outcome,
                        cheriot_vm::cheriot::StepOutcome::Trapped { .. }
                    ),
                    report.completion,
                )
            }
            Self::Integer(computer) => {
                let report = computer.step_report().unwrap();
                (
                    matches!(
                        report.outcome,
                        cheriot_vm::rv32::StepOutcome::Trapped { .. }
                    ),
                    report.completion,
                )
            }
        }
    }
}

fn machine(profile: &str, timing: TimingPolicy, words: &[u32], completion: bool) -> Engine {
    let memory = if profile == "cheriot" {
        "tagged-memory"
    } else {
        "memory"
    };
    let text = format!(
        r#"
[machine]
name = "matrix"
[processor]
type = "{profile}"
[[{memory}]]
name = "code"
type = "ram"
base = {BASE}
size = 4096
[[{memory}]]
name = "data"
type = "ram"
base = {TOHOST}
size = 4096
[[{memory}]]
name = "shadow"
type = "ram"
base = 0x83000000
size = 0x100000
[[timer64]]
name = "timer"
base = 0x02000000
[[uart]]
name = "serial"
base = 0x10000000
"#
    );
    let mut configuration = MachineConfiguration::parse(&text).unwrap();
    configuration.timing = timing;
    configuration.steps_per_tick = 1;
    configuration.default_devices = DefaultDevices::Configured;
    configuration.completion = Some(if completion {
        CompletionPolicy::Address(TOHOST)
    } else {
        CompletionPolicy::Disabled
    });
    let bytes: Vec<u8> = crate::firmware_words::firmware_from_words(words);
    if profile == "cheriot" {
        Engine::Cheriot(Box::new(
            configuration.build_cheriot_raw(BASE, &bytes).unwrap(),
        ))
    } else {
        let mut computer = configuration.build_raw(BASE, &bytes).unwrap();
        computer.trap_policy = TrapPolicy::Vector;
        Engine::Integer(Box::new(computer))
    }
}

#[test]
fn csr_forms_preserve_value_and_encoded_write_intent_on_both_engines() {
    for profile in ["rv32imc", "cheriot"] {
        for (function, source, register_value, expected) in [
            (1, 1, 0x3c, 0x3c),
            (2, 1, 0x3c, 0xbd),
            (3, 1, 0x3c, 0x81),
            (5, 1, 0, 1),
            (6, 1, 0, 0xa5),
            (7, 1, 0, 0xa4),
            (1, 0, 0, 0),
            (2, 0, 0, 0xa5),
            (3, 0, 0, 0xa5),
            (5, 0, 0, 0),
            (6, 0, 0, 0xa5),
            (7, 0, 0, 0xa5),
            (2, 1, 0, 0xa5),
            (3, 1, 0, 0xa5),
        ] {
            let word = (0x340 << 20) | (source << 15) | (function << 12) | (2 << 7) | 0x73;
            let mut machine = machine(profile, TimingPolicy::PerStep, &[word], false);
            match &mut machine {
                Engine::Cheriot(system) => {
                    system.computer.processor.machine_registers.scratch = 0xa5
                }
                Engine::Integer(computer) => computer.processor.machine_registers.scratch = 0xa5,
            }
            machine.set_register(1, register_value, false);
            assert_eq!(machine.step(), (false, None), "{profile}/{word:#010x}");
            assert_eq!(machine.read(2), 0xa5, "{profile}/{word:#010x}");
            let scratch = match &machine {
                Engine::Cheriot(system) => system.computer.processor.machine_registers.scratch,
                Engine::Integer(computer) => computer.processor.machine_registers.scratch,
            };
            assert_eq!(scratch, expected, "{profile}/{word:#010x}");
        }

        for (function, source, traps) in [
            (1, 0, true),
            (2, 0, false),
            (3, 0, false),
            (5, 0, true),
            (6, 0, false),
            (7, 0, false),
            (2, 1, true),
            (3, 1, true),
        ] {
            let word = (0xf11 << 20) | (source << 15) | (function << 12) | (2 << 7) | 0x73;
            let mut machine = machine(profile, TimingPolicy::PerStep, &[word], false);
            machine.set_register(1, 0, false);
            assert_eq!(machine.step(), (traps, None), "{profile}/{word:#010x}");
        }
    }
}

#[test]
fn faulting_counter_writes_do_not_suppress_cycle_increment() {
    use cheriot_vm::cheriot::permissions;

    for profile in ["rv32e", "cheriot"] {
        let source = if profile == "rv32e" { 16 } else { 1 };
        let word = (0xb00 << 20) | (source << 15) | (1 << 12) | (2 << 7) | 0x73;
        let mut machine = machine(profile, TimingPolicy::PerStep, &[word], false);
        if let Engine::Cheriot(system) = &mut machine {
            system.computer.processor.pcc = system
                .computer
                .processor
                .pcc
                .restrict_permissions(0xfff & !permissions::SYSTEM_REGISTERS);
        }
        assert_eq!(machine.step(), (true, None), "{profile}");
        let (cycle, retired) = match &machine {
            Engine::Cheriot(system) => (
                system.computer.processor.machine_registers.cycle,
                system.computer.processor.machine_registers.retired,
            ),
            Engine::Integer(computer) => (
                computer.processor.machine_registers.cycle,
                computer.processor.machine_registers.retired,
            ),
        };
        assert_eq!((cycle, retired), (1, 0), "{profile}");
    }
}

#[test]
fn all_profiles_and_timing_policies_count_attempts_fetches_retirements_and_ticks() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(profile, timing, &[0x0010_8093; 100], false);
            for _ in 0..100 {
                assert_eq!(machine.step(), (false, None), "{profile}/{timing:?}");
            }
            assert_eq!(machine.read(1), 100, "{profile}/{timing:?}");
            let report = machine.report();
            assert_eq!(
                (
                    report.attempted_steps,
                    report.fetched_instructions,
                    report.retired_instructions
                ),
                (100, 100, 100)
            );
            if timing == TimingPolicy::PerStep {
                assert_eq!(report.virtual_time, 100);
            }
            assert_eq!(
                machine.bus().read(0x0200_bff8, 4).unwrap(),
                report.virtual_time as u32
            );
            assert_eq!(machine.bus().devices().len(), 2);
        }
    }
}

#[test]
fn all_profiles_and_timing_policies_observe_only_successful_split_completion_stores() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(
                profile,
                timing,
                &[0x0020_a023, 0x0000_a223, 0x0020_a023],
                true,
            );
            machine.set_register(1, TOHOST, true);
            machine.set_register(2, 1, false);
            assert_eq!(machine.step(), (false, None), "{profile}/{timing:?} low");
            assert_eq!(
                machine.step(),
                (false, Some(0)),
                "{profile}/{timing:?} high"
            );
            assert_eq!(
                machine.step(),
                (false, None),
                "{profile}/{timing:?} repeated completion"
            );
            let report = machine.report();
            assert_eq!(
                (
                    report.attempted_steps,
                    report.fetched_instructions,
                    report.retired_instructions
                ),
                (3, 3, 3)
            );
        }
    }
}

#[test]
fn all_profiles_and_timing_policies_leave_faulting_completion_stores_unobserved() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(profile, timing, &[0x0020_a023], true);
            machine.set_register(1, TOHOST, true);
            machine.set_register(2, 1, false);
            machine.bus().remove_memory("data").unwrap();
            machine
                .bus()
                .add_memory(
                    MemoryRegion::new(
                        "readonly".into(),
                        TOHOST,
                        4096,
                        MemoryKind::Rom,
                        Tagging::None,
                    )
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(machine.step(), (true, None), "{profile}/{timing:?}");
            assert_eq!(machine.bus().inspect(TOHOST, 4).unwrap(), 0);
            assert_eq!(machine.report().retired_instructions, 0);
            match &machine {
                Engine::Cheriot(system) => assert_eq!(system.computer.last_store, None),
                Engine::Integer(computer) => assert_eq!(computer.last_store, None),
            }
        }
    }
}

#[test]
fn all_profiles_and_timing_policies_deliver_interrupts_before_fetch() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(profile, timing, &[0x0010_8093], false);
            machine.bus().write(0x0200_0000, 4, 1).unwrap();
            let registers = match &mut machine {
                Engine::Cheriot(system) => &mut system.computer.processor.machine_registers,
                Engine::Integer(computer) => &mut computer.processor.machine_registers,
            };
            registers.machine_status |= 1 << 3;
            registers.interrupt_enable = 1 << 3;
            assert_eq!(machine.step(), (true, None), "{profile}/{timing:?}");
            let report = machine.report();
            assert_eq!(
                (
                    report.attempted_steps,
                    report.fetched_instructions,
                    report.retired_instructions
                ),
                (1, 0, 0)
            );
            assert_eq!(machine.read(1), 0);
            if timing == TimingPolicy::PerStep {
                assert_eq!(report.virtual_time, 1);
            }
        }
    }
}

#[test]
fn all_profiles_and_timing_policies_share_ordered_output_and_reusable_drain_storage() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(profile, timing, &[0x0020_a023], false);
            machine.set_register(1, 0x1000_0000, true);
            machine.set_register(2, u32::from(b'A'), false);
            assert_eq!(machine.step(), (false, None));
            let mut scratch = Vec::with_capacity(65536);
            let capacity = scratch.capacity();
            let mut output = Vec::new();
            machine
                .bus()
                .drain_output_each(&mut scratch, |kind, name, bytes| {
                    assert_eq!((kind, name), ("UART", "serial"));
                    output.extend_from_slice(bytes);
                    Ok(())
                })
                .unwrap();
            assert_eq!(output, b"A");
            assert_eq!(scratch.capacity(), capacity);
            output.clear();
            machine.bus().drain_output(&mut output);
            assert!(output.is_empty());
        }
    }
}

#[test]
fn explicit_generic_cheriot_elf_loading_does_not_require_symbols_or_default_devices() {
    let mut bytes = include_bytes!("../../examples/hello_world/hello_world.elf").to_vec();
    bytes[32..36].fill(0);
    bytes[46..52].fill(0);
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new(
            "ram".into(),
            BASE,
            64 * 1024 * 1024,
            MemoryKind::Ram,
            Tagging::Cheriot,
        )
        .unwrap(),
    )
    .unwrap();
    let system = CheriotSystem::from_elf_with_policy(
        bus,
        &bytes,
        DefaultDevices::Configured,
        CompletionPolicy::Disabled,
    )
    .unwrap();
    assert!(system.host_interface.is_none());
    assert!(system.computer.bus.devices().is_empty());
}

#[test]
fn htif_rejects_invalid_locations_and_commands_and_bounds_terminal_capture() {
    let mut bus = Bus::new();
    bus.add_memory(
        MemoryRegion::new("ram".into(), 0, 4096, MemoryKind::Ram, Tagging::None).unwrap(),
    )
    .unwrap();
    assert!(HostInterface::on_bus(&bus, 1).is_err());
    assert!(HostInterface::on_bus(&bus, 4096).is_err());
    let mut interface = HostInterface::on_bus(&bus, 0).unwrap();
    bus.write(0, 4, 1).unwrap();
    bus.write(4, 4, 0x0001_0000).unwrap();
    assert!(interface.observe_store(&mut bus, 0, 8).is_err());
    let mut interface = HostInterface::on_bus(&bus, 0).unwrap();
    for _ in 0..65536 {
        bus.write(0, 4, u32::from(b'X')).unwrap();
        bus.write(4, 4, 0x0101_0000).unwrap();
        assert_eq!(interface.observe_store(&mut bus, 0, 8).unwrap(), None);
        assert_eq!(bus.inspect(0, 4).unwrap(), 0);
    }
    bus.write(0, 4, u32::from(b'Y')).unwrap();
    bus.write(4, 4, 0x0101_0000).unwrap();
    assert!(matches!(
        interface.observe_store(&mut bus, 0, 8),
        Err(cheriot_vm::EmulatorError::Firmware(message))
            if message == "HTIF terminal capture is full"
    ));
    assert_eq!(bus.inspect(0, 4).unwrap(), u32::from(b'Y'));
    assert_eq!(bus.inspect(4, 4).unwrap(), 0x0101_0000);
    assert_eq!(interface.take_transmitted(), vec![b'X'; 65536]);
    assert!(interface.take_transmitted().is_empty());
    assert_eq!(interface.observe_store(&mut bus, 0, 8).unwrap(), None);
    assert_eq!(bus.inspect(0, 4).unwrap(), 0);
    assert_eq!(bus.inspect(4, 4).unwrap(), 0);
    assert_eq!(interface.take_transmitted(), b"Y");

    for _ in 0..65536 {
        bus.write(0, 4, u32::from(b'Z')).unwrap();
        bus.write(4, 4, 0x0101_0000).unwrap();
        assert_eq!(interface.observe_store(&mut bus, 0, 8).unwrap(), None);
    }
    bus.write(0, 4, u32::from(b'W')).unwrap();
    bus.write(4, 4, 0x0101_0000).unwrap();
    assert!(interface.observe_store(&mut bus, 0, 8).is_err());

    let mut drained = vec![b'P'];
    interface.drain_transmitted(&mut drained);
    assert_eq!(drained[0], b'P');
    assert_eq!(&drained[1..], vec![b'Z'; 65536]);
    assert!(interface.take_transmitted().is_empty());

    assert_eq!(interface.observe_store(&mut bus, 0, 8).unwrap(), None);
    assert_eq!(bus.inspect(0, 4).unwrap(), 0);
    assert_eq!(bus.inspect(4, 4).unwrap(), 0);
    assert_eq!(interface.take_transmitted(), b"W");
    interface.drain_transmitted(&mut drained);
    assert_eq!(drained.len(), 65537);
}

#[test]
fn integer_tag_storage_is_explicit_and_does_not_enable_capability_instructions() {
    let text = "[machine]\nname='tag-test'\n[processor]\ntype='rv32i'\n[[tagged-memory]]\nname='ram'\ntype='ram'\nbase=0\nsize=4096\n";
    let mut computer = MachineConfiguration::parse(text)
        .unwrap()
        .build_raw(0, &0x0000_005b_u32.to_le_bytes())
        .unwrap();
    assert_eq!(computer.bus.memory()[0].tagging(), Tagging::Cheriot);
    assert!(matches!(
        computer.step().unwrap(),
        cheriot_vm::rv32::StepOutcome::Trapped { cause: 2, .. }
    ));
}

#[cfg(feature = "verification")]
#[test]
fn injected_integer_execution_preserves_instruction_memory_and_uses_real_retirement() {
    let text = "[machine]\nname='injection'\n[processor]\ntype='rv32ec'\n[[tagged-memory]]\nname='ram'\ntype='ram'\nbase=0\nsize=4096\n";
    let mut computer = MachineConfiguration::parse(text)
        .unwrap()
        .build_raw(0, &[0xaa; 8])
        .unwrap();
    assert!(matches!(
        computer.step_injected(0x0070_0093).unwrap(),
        cheriot_vm::rv32::StepOutcome::Executed { .. }
    ));
    assert_eq!(computer.processor.read_register(1).unwrap(), 7);
    assert_eq!(computer.bus.inspect(0, 4).unwrap(), 0xaaaa_aaaa);
    assert_eq!(computer.attempted_steps, 1);
    assert_eq!(computer.fetched_instructions, 1);
    assert_eq!(computer.processor.machine_registers.retired, 1);
    assert_eq!(computer.bus.memory()[0].tag_at(0), Some(false));
    assert!(matches!(
        computer.step_injected(0x0200_0033).unwrap(),
        cheriot_vm::rv32::StepOutcome::Trapped { cause: 2, .. }
    ));
    assert_eq!(computer.processor.machine_registers.retired, 1);
}

#[test]
fn all_profiles_and_timing_policies_preserve_wfi_policy_semantics() {
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut machine = machine(profile, timing, &[0x1050_0073], false);
            machine.bus().write(0x0200_4000, 4, 5).unwrap();
            machine.bus().write(0x0200_4004, 4, 0).unwrap();
            assert_eq!(machine.step(), (false, None), "{profile}/{timing:?}");
            let report = machine.report();
            if timing == TimingPolicy::PerStep {
                assert_eq!(report.virtual_time, 1);
            }
            assert_eq!(
                machine.bus().read(0x0200_bff8, 4).unwrap(),
                report.virtual_time as u32
            );
            assert_eq!(
                (
                    report.attempted_steps,
                    report.fetched_instructions,
                    report.retired_instructions
                ),
                (1, 1, 1)
            );
        }
    }
}

#[test]
fn all_profiles_and_timing_policies_enforce_register_and_extension_limits() {
    for (profile, embedded, multiply, compressed) in [
        ("rv32e", true, false, false),
        ("rv32em", true, true, false),
        ("rv32ec", true, false, true),
        ("rv32emc", true, true, true),
        ("rv32i", false, false, false),
        ("rv32im", false, true, false),
        ("rv32ic", false, false, true),
        ("rv32imc", false, true, true),
        ("cheriot", true, true, true),
    ] {
        for timing in TIMINGS {
            for (word, must_trap) in [
                (0x0010_0813, embedded),
                (0x0220_81b3, !multiply),
                (0x0000_0001, !compressed),
            ] {
                let mut machine = machine(profile, timing, &[word], false);
                machine.set_register(1, 7, false);
                machine.set_register(2, 3, false);
                assert_eq!(
                    machine.step(),
                    (must_trap, None),
                    "{profile}/{timing:?}/{word:#x}"
                );
                assert_eq!(machine.report().fetched_instructions, 1);
                assert_eq!(machine.report().retired_instructions, u64::from(!must_trap));
                if word == 0x0220_81b3 && multiply {
                    assert_eq!(machine.read(3), 21);
                }
            }
            let mut machine = machine(profile, timing, &[0x0001_0001], false);
            match &mut machine {
                Engine::Cheriot(system) => system.computer.processor.program_counter = BASE + 2,
                Engine::Integer(computer) => computer.processor.program_counter = BASE + 2,
            }
            assert_eq!(
                machine.step(),
                (!compressed, None),
                "{profile}/{timing:?}/alignment"
            );
            assert_eq!(machine.report().fetched_instructions, u64::from(compressed));
        }
    }
}

#[test]
fn completion_policy_is_explicit_and_validated_for_both_firmware_forms() {
    let profile = "[machine]\nname='integer'\ncompletion='elf-symbol'\n[processor]\ntype='rv32i'\n[[memory]]\nname='ram'\ntype='ram'\nbase=0\nsize=4096\n";
    assert!(MachineConfiguration::parse(profile)
        .unwrap()
        .build_raw(0, &0x13_u32.to_le_bytes())
        .is_err());
    for value in ["'disabled'", "{ address = 256 }"] {
        let profile = profile.replace("'elf-symbol'", value);
        let computer = MachineConfiguration::parse(&profile)
            .unwrap()
            .build_raw(0, &0x13_u32.to_le_bytes())
            .unwrap();
        assert_eq!(computer.host_interface.is_some(), value != "'disabled'");
    }
    let invalid = profile.replace("'elf-symbol'", "'htif' ");
    assert!(MachineConfiguration::parse(&invalid).is_err());
}

#[test]
fn all_profiles_distinguish_breakpoint_ebreak_and_budget_stops() {
    use std::fs;
    use std::process::Command;
    let directory = std::env::temp_dir().join(format!("emu-runtime-stops-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let firmware = directory.join("stop.bin");
    fs::write(
        &firmware,
        crate::firmware_words::firmware_from_words(&[0x0000_0013_u32, 0x0010_0073]),
    )
    .unwrap();
    for profile in PROFILES {
        let timing = "elapsed";
        let memory = if profile == "cheriot" {
            "tagged-memory"
        } else {
            "memory"
        };
        let text = format!("[machine]\nname='stop'\ncompletion='disabled'\n[processor]\ntype='{profile}'\n[[{memory}]]\nname='ram'\ntype='ram'\nbase={BASE}\nsize=4096\n[[{memory}]]\nname='shadow'\ntype='ram'\nbase=0x83000000\nsize=0x100000\n");
        let path = directory.join(format!("{profile}-{timing}.toml"));
        fs::write(&path, text).unwrap();
        for (extra, success, expected) in [
            (
                vec!["--steps", "1", "--breakpoint", "0x80000004"],
                true,
                "Breakpoint at 0x80000004",
            ),
            (
                vec!["--steps", "2", "--trap-policy", "stop"],
                true,
                "Guest EBREAK at 0x80000004",
            ),
            (
                vec!["--steps", "1"],
                false,
                "step limit reached after 1 steps",
            ),
        ] {
            let result = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
                .args([
                    "--profile",
                    path.to_str().unwrap(),
                    "--raw",
                    firmware.to_str().unwrap(),
                    "--address",
                    "0x80000000",
                    "--output",
                    "raw",
                ])
                .args(extra)
                .output()
                .unwrap();
            assert_eq!(
                result.status.success(),
                success,
                "{profile}/{timing:?}: {:?}",
                result
            );
            assert!(result.stdout.is_empty(), "raw stdout contains diagnostics");
            let diagnostics = String::from_utf8(result.stderr).unwrap();
            assert!(
                diagnostics.contains(expected),
                "{profile}/{timing:?}: {diagnostics}"
            );
            assert!(!diagnostics.contains("Guest exit status:"));
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn complete_capability_stores_do_not_lose_completion_after_fifteen_zero_requests() {
    for timing in TIMINGS {
        let mut machine = machine("cheriot", timing, &[0x0020_b023; 17], true);
        machine.set_register(1, TOHOST, true);
        for _ in 0..15 {
            assert_eq!(machine.step(), (false, None));
        }
        machine.set_register(2, 1, false);
        assert_eq!(machine.step(), (false, Some(0)), "{timing:?}");
        assert_eq!(
            machine.step(),
            (false, None),
            "completion must be observed once"
        );
        assert_eq!(machine.report().retired_instructions, 17);
    }
}

#[test]
fn all_profiles_and_timing_policies_share_spi_and_i2c_host_transfers_and_precise_faults() {
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    use cheriot_vm::devices::i2c::{I2c, I2cTransport};
    use cheriot_vm::devices::spi::{Spi, SpiPeripheral};

    struct SpiHost(Rc<RefCell<Vec<u8>>>);
    impl SpiPeripheral for SpiHost {
        fn transfer(&mut self, value: u8, _: u8) -> io::Result<u8> {
            self.0.borrow_mut().push(value);
            Ok(0x5a)
        }
    }
    struct I2cHost(Rc<RefCell<Vec<u8>>>);
    impl I2cTransport for I2cHost {
        fn transfer(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
            assert_eq!(address, 0x50);
            assert_eq!(write, [0x10]);
            assert_eq!(read.len(), 2);
            self.0.borrow_mut().push(address);
            read.copy_from_slice(&[0xab, 0xcd]);
            Ok(())
        }
    }
    for profile in PROFILES {
        for timing in TIMINGS {
            let mut engine = machine(
                profile,
                timing,
                &[
                    0x0020a023, 0x0000a183, 0x0020a023, 0x0020a023, 0x0000a183, 0x0020a023,
                    0x0000a183, 0x0020a023,
                ],
                false,
            );
            let spi_calls = Rc::new(RefCell::new(Vec::new()));
            let i2c_calls = Rc::new(RefCell::new(Vec::new()));
            let mut spi = Spi::new();
            spi.attach(0, Box::new(SpiHost(spi_calls.clone()))).unwrap();
            engine
                .bus()
                .add_device("spi0".into(), 0x10001000, 256, Box::new(spi))
                .unwrap();
            engine
                .bus()
                .add_device(
                    "i2c0".into(),
                    0x10002000,
                    256,
                    Box::new(I2c::with_host(Box::new(I2cHost(i2c_calls.clone())))),
                )
                .unwrap();
            engine.bus().write(0x10001008, 4, 0).unwrap();
            engine.set_register(1, 0x10001000, true);
            engine.set_register(2, 0x42, false);
            assert!(!engine.step().0, "{profile} {timing:?}");
            assert!(!engine.step().0, "{profile} {timing:?}");
            assert_eq!(engine.read(3), 0x5a);
            assert_eq!(*spi_calls.borrow(), [0x42]);
            engine.set_register(1, 0x10002010, true);
            if let Engine::Cheriot(system) = &mut engine {
                let root = Capability::from_memory(0x7e3e_0000_1000_2000, true);
                let (bounded, exact) = root.set_bounds(24, true);
                assert!(exact);
                assert_eq!(bounded.bounds(), (0x10002000, 0x10002018));
                system
                    .computer
                    .processor
                    .write_register(1, bounded.set_address(0x10002010))
                    .unwrap();
            }
            engine.set_register(2, 1 << 16, false);
            assert!(!engine.step().0, "{profile} {timing:?}");
            engine.bus().write(0x10002000, 4, 0x50).unwrap();
            engine.bus().write(0x10002004, 4, 0x10).unwrap();
            engine.bus().write(0x10002008, 4, 1).unwrap();
            assert!(i2c_calls.borrow().is_empty());
            engine.set_register(1, 0x10002008, true);
            engine.set_register(2, 2, false);
            assert!(!engine.step().0, "{profile} {timing:?}");
            engine.set_register(1, 0x10002004, true);
            assert!(!engine.step().0, "{profile} {timing:?}");
            assert_eq!(engine.read(3), 0xab);
            assert_eq!(*i2c_calls.borrow(), [0x50]);
            engine.set_register(1, 0x10002008, true);
            assert!(!engine.step().0, "{profile} {timing:?}");
            engine.set_register(1, 0x10002004, true);
            assert!(!engine.step().0, "{profile} {timing:?}");
            assert_eq!(engine.read(3), 0xcd);
            assert_eq!(*i2c_calls.borrow(), [0x50]);
            engine.set_register(1, 0x10001001, false);
            assert!(engine.step().0, "{profile} {timing:?}");
            assert_eq!(*spi_calls.borrow(), [0x42]);
            engine.bus().check_host_errors().unwrap();
        }
    }
}
