// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::Cell;
use std::io;
use std::rc::Rc;

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot1::{Cheriot1System, StepOutcome as CapabilityOutcome};
use cheriot_vm::devices::gpio::{Gpio, LineTransport};
use cheriot_vm::devices::InterruptLine;
use cheriot_vm::processor::MachineRegisters;
use cheriot_vm::rv32::StepOutcome;
use cheriot_vm::{Computer, MachineConfiguration, TimingPolicy};

const ENTRY: u32 = 0x8000_0000;
const NOP: u32 = 0x0000_0013;
const PROCESSORS: [&str; 2] = ["rv32imc", "cheriot1"];

enum TestComputer {
    Capability(Box<Cheriot1System>),
    Integer(Box<Computer>),
}

impl TestComputer {
    fn new(processor: &str, timing: &str, words: &[u32], timer: &str) -> Self {
        let text = configuration(processor, timing, timer);
        let configuration = MachineConfiguration::parse(&text).unwrap();
        let bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();

        if configuration.is_cheriot1() {
            Self::Capability(Box::new(
                configuration.build_cheriot1_raw(ENTRY, &bytes).unwrap(),
            ))
        } else {
            let mut computer = configuration.build(ENTRY).unwrap();
            computer.bus.install_firmware(ENTRY, &bytes).unwrap();
            Self::Integer(Box::new(computer))
        }
    }

    fn bus(&mut self) -> &mut Bus {
        match self {
            Self::Capability(system) => &mut system.computer.bus,
            Self::Integer(computer) => &mut computer.bus,
        }
    }

    fn fetched_instructions(&self) -> u64 {
        match self {
            Self::Capability(system) => system.computer.fetched_instructions,
            Self::Integer(computer) => computer.fetched_instructions,
        }
    }

    fn registers(&mut self) -> &mut MachineRegisters {
        match self {
            Self::Capability(system) => &mut system.computer.processor.machine_registers,
            Self::Integer(computer) => &mut computer.processor.machine_registers,
        }
    }

    fn set_instructions(&mut self, instructions: u64) {
        match self {
            Self::Capability(system) => system.computer.fetched_instructions = instructions,
            Self::Integer(computer) => computer.fetched_instructions = instructions,
        }
    }

    fn set_program_counter(&mut self, address: u32) {
        match self {
            Self::Capability(system) => system.computer.processor.program_counter = address,
            Self::Integer(computer) => computer.processor.program_counter = address,
        }
    }

    fn step(&mut self) -> Option<u32> {
        match self {
            Self::Capability(system) => {
                assert_eq!(system.step().unwrap(), None);

                match system.last_outcome.unwrap() {
                    CapabilityOutcome::Retired { .. } => None,
                    CapabilityOutcome::Trapped { cause, .. } => Some(cause),
                }
            }
            Self::Integer(computer) => match computer.step().unwrap() {
                StepOutcome::Executed { .. } => None,
                StepOutcome::Trapped {
                    cause, interrupt, ..
                } => Some(cause | (u32::from(interrupt) << 31)),
            },
        }
    }

    fn write_integer(&mut self, value: u32) {
        match self {
            Self::Capability(system) => system
                .computer
                .processor
                .write_integer_register(1, value)
                .unwrap(),
            Self::Integer(computer) => computer.processor.write_register(1, value).unwrap(),
        }
    }
}

struct InputLines(Rc<Cell<usize>>);

impl LineTransport for InputLines {
    fn read(&mut self) -> io::Result<u32> {
        self.0.set(self.0.get() + 1);
        Ok(0)
    }

    fn write(&mut self, _direction: u32, _outputs: u32) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn compatibility_counter_write_and_inhibit_rules_apply_to_both_engines() {
    for processor in PROCESSORS {
        let mut computer =
            TestComputer::new(processor, "cheriot1-compatible", &[0xb000_9073], "timer64");
        computer.set_instructions(99);
        computer.write_integer(42);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 43);
        assert_eq!(computer.registers().time, 1);

        let mut computer =
            TestComputer::new(processor, "cheriot1-compatible", &[0x3200_9073], "timer64");
        computer.set_instructions(99);
        computer.write_integer(1);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 0);
        assert_eq!(computer.registers().time, 1);
    }
}

#[test]
fn configuration_selects_timing_independently_and_rejects_unknown_values() {
    for processor in PROCESSORS {
        for (value, expected) in [
            ("per-step", TimingPolicy::PerStep),
            ("cheriot1-compatible", TimingPolicy::Cheriot1Compatible),
        ] {
            let configuration =
                MachineConfiguration::parse(&configuration(processor, value, "timer32")).unwrap();
            assert_eq!(configuration.timing, expected);
        }

        let text = configuration(processor, "per-step", "timer32");
        let omitted = text.replace("timing = 'per-step'", "");
        let default = MachineConfiguration::parse(&omitted).unwrap().timing;
        assert_eq!(
            default,
            if processor == "cheriot1" {
                TimingPolicy::Cheriot1Compatible
            } else {
                TimingPolicy::PerStep
            }
        );

        for value in ["'unknown'", "0", "true", "{}"] {
            let invalid = text.replace("'per-step'", value);
            let error = MachineConfiguration::parse(&invalid)
                .unwrap_err()
                .to_string();
            assert!(error.contains("timing"), "{error}");
        }
    }

    for value in ["per-step", "cheriot1-compatible"] {
        let profile = include_str!("../../profiles/cheriot1.toml").replace(
            "timing = \"cheriot1-compatible\"",
            &format!("timing = '{value}'"),
        );
        let configuration = MachineConfiguration::parse(&profile).unwrap();
        let timing = configuration.timing;
        let system = configuration
            .build_cheriot1(include_bytes!("../../examples/hello_world/hello_world.elf"))
            .unwrap();
        assert_eq!(system.computer.timing, timing);
    }
}

#[test]
fn decoded_faults_count_as_fetched_but_fetch_faults_do_not() {
    for processor in PROCESSORS {
        for (timing, ticks, after_fetch_faults) in
            [("per-step", 100, 103), ("cheriot1-compatible", 1, 1)]
        {
            let mut computer = TestComputer::new(processor, timing, &[0xffff_ffff], "timer32");

            for _ in 0..100 {
                computer.set_program_counter(ENTRY);
                assert_eq!(computer.step(), Some(2));
            }

            assert_eq!(computer.fetched_instructions(), 100);
            assert_eq!(computer.registers().retired, 0);
            assert_eq!(computer.registers().time, ticks);

            for _ in 0..3 {
                computer.set_program_counter(0x9000_0000);
                assert_eq!(computer.step(), Some(1));
            }

            assert_eq!(computer.fetched_instructions(), 100);
            assert_eq!(computer.registers().time, after_fetch_faults);
        }
    }
}

#[test]
fn generic_timer_wait_deadlines_respect_masks_pending_state_and_counter_wrap() {
    for processor in PROCESSORS {
        for control in [0, 1] {
            let mut computer =
                TestComputer::new(processor, "cheriot1-compatible", &[0x1050_0073], "timer32");
            computer.bus().write(0x0200_0004, 4, 200).unwrap();
            computer.bus().write(0x0200_0008, 4, control).unwrap();
            assert_eq!(computer.step(), None);
            assert_eq!(computer.registers().time, 0);
        }

        let mut computer = TestComputer::new(
            processor,
            "cheriot1-compatible",
            &[0x1050_0073; 2],
            "timer32",
        );
        computer.bus().write(0x0200_0000, 4, u32::MAX - 1).unwrap();
        computer.bus().write(0x0200_0004, 4, 1).unwrap();
        computer.bus().write(0x0200_0008, 4, 3).unwrap();
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 3);
        assert_eq!(computer.bus().read(0x0200_0000, 4).unwrap(), 1);
        assert_eq!(computer.bus().read(0x0200_000c, 4).unwrap(), 1);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 3);
    }
}

#[test]
fn per_step_policy_honours_counter_writes_and_preinstruction_inhibit() {
    for processor in PROCESSORS {
        let words = [0xb000_9073, 0x3200_9073, NOP, 0x3200_9073, NOP];
        let mut computer = TestComputer::new(processor, "per-step", &words, "timer32");
        computer.write_integer(42);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 42);

        computer.write_integer(5);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 43);
        assert_eq!(computer.registers().retired, 2);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 43);
        assert_eq!(computer.registers().retired, 2);

        computer.write_integer(0);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 43);
        assert_eq!(computer.registers().retired, 2);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 44);
        assert_eq!(computer.registers().retired, 3);
        assert_eq!(computer.registers().time, 5);
    }
}

#[test]
fn shared_policies_control_timer_and_gpio_ticks_on_both_engines() {
    for processor in PROCESSORS {
        for (timing, before, after) in [("per-step", 99, 100), ("cheriot1-compatible", 0, 1)] {
            let mut computer = TestComputer::new(processor, timing, &[NOP; 100], "timer32");
            let reads = Rc::new(Cell::new(0));
            let gpio = Gpio::with_host(Box::new(InputLines(reads.clone()))).unwrap();
            *computer.bus().device_mut::<Gpio>("gpio0").unwrap() = gpio;
            computer.bus().write(0x0200_0008, 4, 1).unwrap();

            for _ in 0..99 {
                assert_eq!(computer.step(), None);
            }

            assert_eq!(computer.registers().time, before);
            assert_eq!(computer.bus().read(0x0200_0000, 4).unwrap(), before as u32);
            assert_eq!(reads.get(), before as usize + 1);
            assert_eq!(computer.step(), None);
            assert_eq!(computer.registers().time, after);
            assert_eq!(computer.registers().cycle, after);
            assert_eq!(computer.registers().retired, 100);
            assert_eq!(computer.bus().read(0x0200_0000, 4).unwrap(), after as u32);
            assert_eq!(reads.get(), after as usize + 1);
            assert_eq!(computer.fetched_instructions(), 100);
        }
    }
}

#[test]
fn software_interrupt_precedes_timer_and_only_per_step_policy_ticks_on_delivery() {
    for processor in PROCESSORS {
        for (timing, after_interrupt) in [("per-step", 101), ("cheriot1-compatible", 1)] {
            let mut computer = TestComputer::new(processor, timing, &[NOP; 100], "timer64");

            for _ in 0..100 {
                assert_eq!(computer.step(), None);
            }

            computer.bus().write(0x0200_0000, 4, 1).unwrap();
            computer.registers().write(0x304, 0x88).unwrap();
            computer.registers().machine_status |= 8;
            assert_eq!(computer.step(), Some(0x8000_0003));
            assert_eq!(computer.fetched_instructions(), 100);
            assert_eq!(computer.registers().retired, 100);
            assert_eq!(computer.registers().time, after_interrupt);
        }
    }
}

#[test]
fn time_and_cycle_wrap_without_changing_tick_phase_or_instruction_retirement() {
    for processor in PROCESSORS {
        for timing in ["per-step", "cheriot1-compatible"] {
            let mut computer = TestComputer::new(processor, timing, &[NOP], "timer32");
            computer.set_instructions(u64::MAX);
            computer.registers().cycle = u64::MAX;
            computer.registers().time = u64::MAX;
            assert_eq!(computer.step(), None);
            assert_eq!(computer.fetched_instructions(), 0);
            assert_eq!(computer.registers().cycle, 0);
            assert_eq!(computer.registers().time, 0);
            assert_eq!(computer.registers().retired, 1);
        }
    }
}

#[test]
fn wait_for_interrupt_uses_the_selected_policy_with_the_same_clint() {
    let mut words = [NOP; 100];
    words[0] = 0x1050_0073;

    for processor in PROCESSORS {
        let mut computer = TestComputer::new(processor, "per-step", &words, "timer64");
        computer.bus().write(0x0200_4000, 4, 200).unwrap();
        computer.registers().interrupt_enable = 0x80;
        computer.registers().machine_status |= 8;

        for _ in 0..100 {
            assert_eq!(computer.step(), None);
        }

        assert_eq!(computer.registers().time, 100);
        assert_eq!(computer.bus().read(0x0200_bff8, 4).unwrap(), 100);
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));

        let mut computer = TestComputer::new(processor, "cheriot1-compatible", &words, "timer64");
        computer.bus().write(0x0200_4000, 4, 200).unwrap();
        computer.registers().interrupt_enable = 0x80;
        computer.registers().machine_status |= 8;
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 200);
        assert_eq!(computer.registers().cycle, 200);
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));

        for _ in 1..100 {
            assert_eq!(computer.step(), None);
        }

        assert_eq!(computer.registers().time, 201);
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        assert_eq!(computer.step(), Some(0x8000_0007));
        assert_eq!(computer.fetched_instructions(), 100);
        assert_eq!(computer.registers().time, 201);
    }
}

fn configuration(processor: &str, timing: &str, timer: &str) -> String {
    format!(
        "[machine]
name = 'timing-test'
timing = '{timing}'
[processor]
type = '{processor}'
[[cheriot1-memory]]
name = 'ram'
type = 'ram'
base = 0x80000000
size = 4096
[[cheriot1-memory]]
name = 'shadow'
type = 'ram'
base = 0x83000000
size = 0x100000
[[{timer}]]
name = 'timer0'
base = 0x02000000
[[gpio]]
name = 'gpio0'
base = 0x10003000
"
    )
}
