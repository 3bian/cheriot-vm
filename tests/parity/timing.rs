// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::Cell;
use std::io;
use std::rc::Rc;

use cheriot_vm::bus::Bus;
use cheriot_vm::cheriot::{CheriotSystem, StepOutcome as CapabilityOutcome};
use cheriot_vm::devices::gpio::{Gpio, LineTransport};
use cheriot_vm::devices::InterruptLine;
use cheriot_vm::processor::MachineRegisters;
use cheriot_vm::rv32::StepOutcome;
use cheriot_vm::{Computer, MachineConfiguration, TimingPolicy};

const ENTRY: u32 = 0x8000_0000;
const NOP: u32 = 0x0000_0013;
const PROCESSORS: [&str; 2] = ["rv32imc", "cheriot"];
const HOST_PROCESSORS: [&str; 9] = [
    "rv32e", "rv32em", "rv32ec", "rv32emc", "rv32i", "rv32im", "rv32ic", "rv32imc", "cheriot",
];

enum TestComputer {
    Capability(Box<CheriotSystem>),
    Integer(Box<Computer>),
}

impl TestComputer {
    fn new(processor: &str, timing: TimingPolicy, words: &[u32], timer: &str) -> Self {
        let text = configuration(processor, timer);
        let mut configuration = MachineConfiguration::parse(&text).unwrap();
        configuration.timing = timing;
        configuration.steps_per_tick = 1;
        let bytes: Vec<_> = crate::firmware_words::firmware_from_words(words);

        if configuration.is_cheriot() {
            Self::Capability(Box::new(
                configuration.build_cheriot_raw(ENTRY, &bytes).unwrap(),
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

    fn set_steps_per_tick(&mut self, steps: u32) -> Result<(), cheriot_vm::EmulatorError> {
        match self {
            Self::Capability(system) => system.computer.set_steps_per_tick(steps),
            Self::Integer(computer) => computer.set_steps_per_tick(steps),
        }
    }

    fn step_phase(&self) -> u32 {
        match self {
            Self::Capability(system) => system.computer.step_phase(),
            Self::Integer(computer) => computer.step_phase(),
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

    #[cfg(feature = "verification")]
    fn set_timer_hz(&mut self, hz: u32) {
        match self {
            Self::Capability(system) => system.computer.set_timer_hz(hz).unwrap(),
            Self::Integer(computer) => computer.set_timer_hz(hz).unwrap(),
        }
    }

    #[cfg(feature = "verification")]
    fn use_clock(&mut self) -> Rc<Cell<std::time::Instant>> {
        let now = Rc::new(Cell::new(std::time::Instant::now()));
        let source = Rc::clone(&now);
        match self {
            Self::Capability(system) => system
                .computer
                .set_monotonic_clock(move || source.get())
                .unwrap(),
            Self::Integer(computer) => computer.set_monotonic_clock(move || source.get()).unwrap(),
        }
        now
    }

    #[cfg(feature = "verification")]
    fn step_nop(&mut self) {
        self.set_program_counter(ENTRY);
        assert_eq!(self.step(), None);
    }
}

#[cfg(feature = "verification")]
fn advance(clock: &Cell<std::time::Instant>, duration: std::time::Duration) {
    clock.set(clock.get() + duration);
}

#[cfg(feature = "verification")]
fn elapsed_computer(
    processor: &str,
    hz: u32,
    words: &[u32],
) -> (TestComputer, Rc<Cell<std::time::Instant>>) {
    let mut computer = TestComputer::new(processor, TimingPolicy::Elapsed, words, "timer64");
    computer.set_timer_hz(hz);
    let clock = computer.use_clock();
    computer.bus().write(0x0200_4000, 4, u32::MAX).unwrap();
    computer.bus().write(0x0200_4004, 4, u32::MAX).unwrap();
    (computer, clock)
}

#[cfg(feature = "verification")]
#[test]
fn elapsed_time_is_independent_of_instruction_count_on_both_engines() {
    use std::time::Duration;
    for processor in PROCESSORS {
        let (mut fast, fast_clock) = elapsed_computer(processor, 2_000, &[NOP]);
        let (mut slow, slow_clock) = elapsed_computer(processor, 2_000, &[NOP]);
        fast.step_nop();
        slow.step_nop();
        for _ in 0..99 {
            fast.step_nop();
        }
        advance(&fast_clock, Duration::from_micros(3_500));
        advance(&slow_clock, Duration::from_micros(3_500));
        fast.step_nop();
        slow.step_nop();
        assert_eq!(fast.registers().time, 7, "{processor}");
        assert_eq!(slow.registers().time, 7, "{processor}");
        assert_ne!(fast.registers().retired, slow.registers().retired);
        assert_eq!(fast.bus().read(0x0200_bff8, 4).unwrap(), 7);
        assert_eq!(fast.registers().read(0xc01), Some(7));
        assert_eq!(fast.registers().read(0xc81), Some(0));

        let (mut first, first_clock) = elapsed_computer(processor, 2_000, &[NOP]);
        let (mut second, second_clock) = elapsed_computer(processor, 2_000, &[NOP]);
        first.step_nop();
        second.step_nop();
        advance(&first_clock, Duration::from_millis(2));
        advance(&second_clock, Duration::from_millis(5));
        first.step_nop();
        second.step_nop();
        assert_eq!((first.registers().time, second.registers().time), (4, 10));
        assert_eq!(first.registers().retired, second.registers().retired);
    }
}

#[cfg(feature = "verification")]
#[test]
fn guest_time_csr_views_match_timer_where_supported() {
    use std::time::Duration;
    const READ_TIME: u32 = 0xc010_2173; // csrrs x2, time, x0
    const READ_TIMEH: u32 = 0xc810_21f3; // csrrs x3, timeh, x0

    for processor in ["rv32e", "rv32imc"] {
        let (mut computer, clock) = elapsed_computer(processor, 1, &[NOP, READ_TIME, READ_TIMEH]);
        assert_eq!(computer.step(), None);
        advance(&clock, Duration::from_secs(4_294_967_297));
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        let TestComputer::Integer(computer) = &mut computer else {
            unreachable!()
        };
        assert_eq!(computer.processor.read_register(2).unwrap(), 1);
        assert_eq!(computer.processor.read_register(3).unwrap(), 1);
        assert_eq!(computer.bus.read(0x0200_bff8, 4).unwrap(), 1);
        assert_eq!(computer.bus.read(0x0200_bffc, 4).unwrap(), 1);
    }

    // The CHERIoT CSR model currently rejects these views; its firmware
    // reads the same platform timer through MMIO instead.
    for instruction in [READ_TIME, READ_TIMEH] {
        let (mut computer, clock) = elapsed_computer("cheriot", 1, &[NOP, instruction]);
        assert_eq!(computer.step(), None);
        advance(&clock, Duration::from_secs(1));
        assert_eq!(computer.step(), Some(2));
        assert_eq!(computer.bus().read(0x0200_bff8, 4).unwrap(), 1);
    }
}

#[cfg(feature = "verification")]
#[test]
fn fractional_samples_and_frequency_scale_without_drift() {
    use std::time::Duration;
    for processor in PROCESSORS {
        let (mut fragmented, fragmented_clock) = elapsed_computer(processor, 3, &[NOP]);
        let (mut single, single_clock) = elapsed_computer(processor, 3, &[NOP]);
        fragmented.step_nop();
        single.step_nop();
        for _ in 0..1_000 {
            advance(&fragmented_clock, Duration::from_millis(1));
            fragmented.step_nop();
        }
        advance(&single_clock, Duration::from_secs(1));
        single.step_nop();
        assert_eq!(
            (fragmented.registers().time, single.registers().time),
            (3, 3)
        );

        let (mut computer, clock) = elapsed_computer(processor, 12_345, &[NOP]);
        computer.step_nop();
        advance(&clock, Duration::from_millis(2_500));
        computer.step_nop();
        assert_eq!(computer.registers().time, 30_862);
        advance(&clock, Duration::from_millis(500));
        computer.step_nop();
        assert_eq!(computer.registers().time, 37_035);

        let (mut computer, clock) = elapsed_computer(processor, 1_000_000_000, &[NOP]);
        computer.step_nop();
        advance(&clock, Duration::from_secs(20_000_000_000));
        computer.step_nop();
        let expected = (20_000_000_000_u128 * 1_000_000_000) as u64;
        assert_eq!(computer.registers().time, expected);
        assert_eq!(
            computer.bus().read(0x0200_bff8, 4).unwrap(),
            expected as u32
        );
        assert_eq!(
            computer.bus().read(0x0200_bffc, 4).unwrap(),
            (expected >> 32) as u32
        );
        assert_eq!(computer.registers().read(0xc01), Some(expected as u32));
        assert_eq!(
            computer.registers().read(0xc81),
            Some((expected >> 32) as u32)
        );
    }
}

#[cfg(feature = "verification")]
#[test]
fn timer_comparator_updates_pending_independently_of_interrupt_enable() {
    use std::time::Duration;
    for processor in PROCESSORS {
        let (mut computer, clock) = elapsed_computer(processor, 2_000, &[NOP]);
        computer.bus().write(0x0200_4004, 4, 0).unwrap();
        computer.bus().write(0x0200_4000, 4, 3).unwrap();
        computer.step_nop();
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        advance(&clock, Duration::from_micros(1_500));
        computer.step_nop();
        assert_eq!(computer.registers().time, 3);
        assert_ne!(computer.registers().interrupt_pending & 0x80, 0);
        assert_eq!(computer.registers().interrupt_enable & 0x80, 0);

        computer.bus().write(0x0200_4000, 4, 5).unwrap();
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        computer.step_nop();
        assert_eq!(computer.registers().interrupt_pending & 0x80, 0);
        computer.bus().write(0x0200_4000, 4, 2).unwrap();
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        computer.registers().machine_status |= 8;
        computer.step_nop();
        assert_ne!(computer.registers().interrupt_pending & 0x80, 0);
        computer.registers().interrupt_enable |= 0x80;
        computer.set_program_counter(ENTRY);
        assert_eq!(computer.step(), Some(0x8000_0007));
    }
}

#[cfg(feature = "verification")]
#[test]
fn timer_comparator_uses_both_unsigned_halves_and_timeh() {
    use std::time::Duration;
    for processor in PROCESSORS {
        let (mut computer, clock) = elapsed_computer(processor, 1_000_000_000, &[NOP]);
        computer.bus().write(0x0200_4000, 4, 4).unwrap();
        computer.bus().write(0x0200_4004, 4, 1).unwrap();
        computer.step_nop();
        advance(&clock, Duration::from_nanos(0x1_0000_0003));
        computer.step_nop();
        assert_eq!(computer.registers().read(0xc81), Some(1));
        assert_eq!(computer.registers().read(0xc01), Some(3));
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        advance(&clock, Duration::from_nanos(1));
        computer.step_nop();
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        computer.bus().write(0x0200_4004, 4, 2).unwrap();
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        computer.step_nop();
        assert_eq!(computer.registers().interrupt_pending & 0x80, 0);
    }
}

#[cfg(feature = "verification")]
#[test]
fn host_clock_wait_is_bounded_even_at_one_hertz() {
    use std::time::{Duration, Instant};
    for processor in PROCESSORS {
        let (mut computer, clock) = elapsed_computer(processor, 1, &[0x1050_0073, NOP]);
        computer.bus().write(0x0200_4000, 4, 100).unwrap();
        computer.bus().write(0x0200_4004, 4, 0).unwrap();
        let started = Instant::now();
        assert_eq!(computer.step(), None);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "{processor}"
        );
        assert_eq!(computer.registers().time, 0);
        advance(&clock, Duration::from_secs(1));
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 1);
    }
}

#[cfg(feature = "verification")]
#[test]
fn wfi_polls_external_input_without_advancing_the_timer() {
    struct InputAfterWait(Rc<Cell<usize>>);
    impl LineTransport for InputAfterWait {
        fn read(&mut self) -> io::Result<u32> {
            self.0.set(self.0.get() + 1);
            Ok(u32::from(self.0.get() >= 3))
        }

        fn write(&mut self, _direction: u32, _outputs: u32) -> io::Result<()> {
            Ok(())
        }
    }

    for processor in PROCESSORS {
        let (mut computer, _) = elapsed_computer(processor, 1, &[0x1050_0073]);
        let reads = Rc::new(Cell::new(0));
        let gpio = Gpio::with_host(Box::new(InputAfterWait(Rc::clone(&reads)))).unwrap();
        *computer.bus().device_mut::<Gpio>("gpio0").unwrap() = gpio;
        computer.bus().write(0x1000_300c, 4, 1).unwrap();
        assert_eq!(computer.step(), None);
        assert!(reads.get() >= 3, "{processor}");
        assert_eq!(computer.registers().time, 0);
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineExternal));
        assert_ne!(computer.registers().interrupt_pending & (1 << 11), 0);
    }
}

#[cfg(feature = "verification")]
#[test]
fn host_time_advances_while_cpu_counters_are_inhibited_or_written() {
    use std::time::Duration;
    let words = [0x3200_9073, NOP, 0xb000_9073, 0xb020_9073, 0x3200_9073, NOP];
    for processor in PROCESSORS {
        let (mut computer, clock) = elapsed_computer(processor, 2_000, &words);
        computer.write_integer(5);
        assert_eq!(computer.step(), None);
        assert_eq!(
            (computer.registers().cycle, computer.registers().retired),
            (1, 1)
        );
        advance(&clock, Duration::from_millis(1));
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().retired,
                computer.registers().time
            ),
            (1, 1, 2)
        );
        computer.write_integer(42);
        advance(&clock, Duration::from_millis(1));
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().retired,
                computer.registers().time
            ),
            (42, 1, 4)
        );
        advance(&clock, Duration::from_millis(1));
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().retired,
                computer.registers().time
            ),
            (42, 42, 6)
        );
        computer.write_integer(0);
        advance(&clock, Duration::from_millis(1));
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().retired,
                computer.registers().time
            ),
            (42, 42, 8)
        );
        advance(&clock, Duration::from_millis(1));
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().retired,
                computer.registers().time
            ),
            (43, 43, 10)
        );
    }
}

#[cfg(all(feature = "verification", feature = "gdb"))]
#[test]
fn debugger_pause_excludes_halted_time_and_keeps_fractional_progress() {
    use gdbstub::target::ext::base::singlethread::SingleThreadResume;
    use std::time::Duration;

    macro_rules! exercise {
        ($target:expr, $clock:expr, $time:expr) => {{
            let mut target = $target;
            let time = $time;
            advance(&$clock, Duration::from_secs(5));
            assert_eq!(time(&target), 0);
            target.resume(None).unwrap();
            advance(&$clock, Duration::from_micros(250));
            assert_eq!(target.advance().unwrap(), None);
            target.halt(cheriot_vm::debugger::StopReason::Interrupted);
            assert_eq!(time(&target), 0);
            advance(&$clock, Duration::from_secs(10));
            assert_eq!(time(&target), 0);
            target.resume(None).unwrap();
            advance(&$clock, Duration::from_micros(250));
            assert_eq!(target.advance().unwrap(), None);
            assert_eq!(time(&target), 1);
        }};
    }

    for processor in PROCESSORS {
        let (computer, clock) = elapsed_computer(processor, 2_000, &[NOP, NOP]);
        match computer {
            TestComputer::Capability(system) => exercise!(
                cheriot_vm::cheriot::debugger::Target::new(*system, None),
                clock,
                |target: &cheriot_vm::cheriot::debugger::Target| target
                    .system()
                    .computer
                    .processor
                    .machine_registers
                    .time
            ),
            TestComputer::Integer(computer) => exercise!(
                cheriot_vm::rv32::debugger::Target::new(*computer, None),
                clock,
                |target: &cheriot_vm::rv32::debugger::Target| target
                    .computer()
                    .processor
                    .machine_registers
                    .time
            ),
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
fn processor_timing_defaults_can_be_overridden_by_embedding() {
    for processor in PROCESSORS {
        for timing in [TimingPolicy::PerStep, TimingPolicy::Elapsed] {
            let computer = TestComputer::new(processor, timing, &[NOP], "timer32");
            let actual = match computer {
                TestComputer::Capability(system) => system.computer.timing,
                TestComputer::Integer(computer) => computer.timing,
            };
            assert_eq!(actual, timing);
        }

        let text = configuration(processor, "timer32");
        let default = MachineConfiguration::parse(&text).unwrap().timing;
        assert_eq!(default, TimingPolicy::Elapsed);
    }

    for timing in [TimingPolicy::PerStep, TimingPolicy::Elapsed] {
        let profile = include_str!("../../profiles/cheriot.toml");
        let mut configuration = MachineConfiguration::parse(profile).unwrap();
        configuration.timing = timing;
        configuration.steps_per_tick = 1;
        let system = configuration
            .build_cheriot(include_bytes!("../../examples/hello_world/hello_world.elf"))
            .unwrap();
        assert_eq!(system.computer.timing, timing);
    }
}

#[test]
fn host_clock_advances_timer_and_interrupts_independently_of_instructions() {
    for processor in HOST_PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::Elapsed, &[NOP; 2], "timer64");
        computer.bus().write(0x0200_4000, 4, 20).unwrap();
        assert_eq!(computer.step(), None);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(computer.step(), None);
        let time = computer.registers().time;
        assert!(time >= 40, "{processor}: only {time} ticks after 20 ms");
        assert_eq!(computer.bus().read(0x0200_bff8, 4).unwrap(), time as u32);
        assert_eq!(computer.registers().read(0xc01), Some(time as u32));
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        assert_ne!(computer.registers().interrupt_pending & (1 << 7), 0);
        assert_eq!(computer.registers().cycle, 2);
        assert_eq!(computer.registers().retired, 2);
        computer.registers().interrupt_enable = 0x80;
        computer.registers().machine_status |= 8;
        assert_eq!(computer.step(), Some(0x8000_0007));
        assert_eq!(computer.registers().retired, 2);
        assert_eq!(computer.registers().cycle, 3);
    }
}

#[test]
fn host_clock_wait_does_not_fast_forward_to_a_distant_deadline() {
    for processor in HOST_PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::Elapsed, &[0x1050_0073], "timer64");
        computer.bus().write(0x0200_4000, 4, 2_000_000).unwrap();
        let start = std::time::Instant::now();
        assert_eq!(computer.step(), None);
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert!(computer.registers().time < 20_000);
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        assert_eq!(computer.registers().cycle, 1);
    }
}

#[test]
fn host_clock_has_an_explicit_checkpoint_identity() {
    for processor in HOST_PROCESSORS {
        let computer = TestComputer::new(processor, TimingPolicy::Elapsed, &[NOP], "timer64");
        let mut checkpoint = Vec::new();
        match computer {
            TestComputer::Capability(system) => {
                system.write_checkpoint(&mut checkpoint).unwrap();
                let mut legacy = Vec::new();
                assert!(system.write_checkpoint_v1(&mut legacy).is_err());
                assert!(legacy.is_empty());
            }
            TestComputer::Integer(computer) => computer.write_checkpoint(&mut checkpoint).unwrap(),
        }
        assert_eq!(&checkpoint[..8], b"CHKP0004");
        assert_eq!(checkpoint[11], 2);
    }
}

#[test]
fn decoded_faults_count_as_fetched_but_fetch_faults_do_not() {
    for processor in PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::PerStep, &[0xffff_ffff], "timer32");

        for _ in 0..100 {
            computer.set_program_counter(ENTRY);
            assert_eq!(computer.step(), Some(2));
        }

        assert_eq!(computer.fetched_instructions(), 100);
        assert_eq!(computer.registers().retired, 0);
        assert_eq!(computer.registers().time, 100);

        for _ in 0..3 {
            computer.set_program_counter(0x9000_0000);
            assert_eq!(computer.step(), Some(1));
        }

        assert_eq!(computer.fetched_instructions(), 100);
        assert_eq!(computer.registers().time, 103);
    }
}

#[test]
fn generic_timer_wait_deadlines_respect_masks_pending_state_and_counter_wrap() {
    for processor in PROCESSORS {
        for control in [0, 1] {
            let mut computer =
                TestComputer::new(processor, TimingPolicy::PerStep, &[0x1050_0073], "timer32");
            computer.bus().write(0x0200_0004, 4, 200).unwrap();
            computer.bus().write(0x0200_0008, 4, control).unwrap();
            assert_eq!(computer.step(), None);
            assert_eq!(computer.registers().time, 1);
        }

        let mut computer = TestComputer::new(
            processor,
            TimingPolicy::PerStep,
            &[0x1050_0073; 3],
            "timer32",
        );
        computer.bus().write(0x0200_0000, 4, u32::MAX - 1).unwrap();
        computer.bus().write(0x0200_0004, 4, 1).unwrap();
        computer.bus().write(0x0200_0008, 4, 3).unwrap();
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 1);
        assert_eq!(computer.bus().read(0x0200_0000, 4).unwrap(), u32::MAX);
        assert_eq!(computer.bus().read(0x0200_000c, 4).unwrap(), 0);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().time, 3);
        assert_eq!(computer.bus().read(0x0200_0000, 4).unwrap(), 1);
        assert_eq!(computer.bus().read(0x0200_000c, 4).unwrap(), 1);
    }
}

#[test]
fn per_step_policy_honours_counter_writes_and_preinstruction_inhibit() {
    for processor in PROCESSORS {
        let words = [0xb000_9073, 0x3200_9073, NOP, 0x3200_9073, NOP];
        let mut computer = TestComputer::new(processor, TimingPolicy::PerStep, &words, "timer32");
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
fn per_step_controls_timer_and_gpio_ticks_on_both_engines() {
    for processor in PROCESSORS {
        let (before, after) = (99, 100);
        let mut computer =
            TestComputer::new(processor, TimingPolicy::PerStep, &[NOP; 100], "timer32");
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

#[test]
fn software_interrupt_precedes_timer_under_per_step() {
    for processor in PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::PerStep, &[NOP; 100], "timer64");

        for _ in 0..100 {
            assert_eq!(computer.step(), None);
        }

        computer.bus().write(0x0200_0000, 4, 1).unwrap();
        computer.registers().write(0x304, 0x88).unwrap();
        computer.registers().machine_status |= 8;
        assert_eq!(computer.step(), Some(0x8000_0003));
        assert_eq!(computer.fetched_instructions(), 100);
        assert_eq!(computer.registers().retired, 100);
        assert_eq!(computer.registers().time, 101);
    }
}

#[test]
fn time_and_cycle_wrap_without_changing_tick_phase_or_instruction_retirement() {
    for processor in PROCESSORS {
        let mut computer = TestComputer::new(processor, TimingPolicy::PerStep, &[NOP], "timer32");
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

#[test]
fn per_step_wfi_does_not_jump_the_machine_timer() {
    let mut words = [NOP; 100];
    words[0] = 0x1050_0073;

    for processor in PROCESSORS {
        let mut computer = TestComputer::new(processor, TimingPolicy::PerStep, &words, "timer64");
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
    }
}

fn configuration(processor: &str, timer: &str) -> String {
    format!(
        "[machine]
name = 'timing-test'
[processor]
type = '{processor}'
[[cheriot-memory]]
name = 'ram'
type = 'ram'
base = 0x80000000
size = 4096
[[cheriot-memory]]
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

#[test]
fn step_quantum_accumulates_exactly_on_every_processor_and_preserves_retirement() {
    for processor in HOST_PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::PerStep, &[NOP; 100], "timer64");
        computer.set_steps_per_tick(100).unwrap();
        computer.bus().write(0x0200_4000, 4, 1).unwrap();
        for _ in 0..99 {
            assert_eq!(computer.step(), None);
        }
        assert_eq!(computer.step_phase(), 99);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().time,
                computer.registers().retired
            ),
            (0, 0, 99)
        );
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        assert!(computer.set_steps_per_tick(1).is_err());
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step_phase(), 0);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().time,
                computer.registers().retired
            ),
            (1, 1, 100)
        );
        assert_eq!(computer.bus().read(0x0200_bff8, 4).unwrap(), 1);
        assert!(computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
        computer.registers().machine_status |= 8;
        computer.registers().interrupt_enable |= 128;
        assert_eq!(computer.step(), Some(0x8000_0007));
        assert_eq!(computer.step_phase(), 1);
        assert_eq!(computer.registers().retired, 100);
        assert_eq!(computer.registers().time, 1);
    }
}

#[test]
fn step_quantum_keeps_counter_writes_and_inhibition_independent_of_time() {
    for processor in HOST_PROCESSORS {
        let words = [
            NOP,
            0xb000_9073,
            NOP,
            0x3200_9073,
            NOP,
            0x3200_9073,
            NOP,
            NOP,
        ];
        let mut computer = TestComputer::new(processor, TimingPolicy::PerStep, &words, "timer64");
        computer.set_steps_per_tick(2).unwrap();
        computer.write_integer(42);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        assert_eq!(
            (computer.registers().cycle, computer.registers().time),
            (42, 1)
        );
        computer.write_integer(5);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().time,
                computer.registers().retired
            ),
            (43, 2, 4)
        );
        computer.write_integer(0);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().time,
                computer.registers().retired
            ),
            (43, 3, 4)
        );
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step(), None);
        assert_eq!(
            (
                computer.registers().cycle,
                computer.registers().time,
                computer.registers().retired
            ),
            (44, 4, 6)
        );
    }
}

#[test]
fn step_quantum_rejects_zero_and_keeps_its_fraction_across_counter_wrap() {
    for processor in HOST_PROCESSORS {
        let mut computer =
            TestComputer::new(processor, TimingPolicy::PerStep, &[NOP; 6], "timer64");
        assert!(computer.set_steps_per_tick(0).is_err());
        computer.set_steps_per_tick(3).unwrap();
        computer.registers().cycle = u64::MAX;
        for _ in 0..2 {
            assert_eq!(computer.step(), None);
        }
        assert_eq!(computer.registers().cycle, u64::MAX);
        assert_eq!(computer.step_phase(), 2);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.registers().cycle, 0);
        assert_eq!(computer.step_phase(), 0);
        for _ in 0..3 {
            assert_eq!(computer.step(), None);
        }
        assert_eq!(computer.registers().cycle, 1);
        assert_eq!(computer.registers().time, 2);
        assert_eq!(computer.registers().retired, 6);
        let mut elapsed = TestComputer::new(processor, TimingPolicy::Elapsed, &[NOP], "timer64");
        assert!(elapsed.set_steps_per_tick(100).is_err());
    }
}

#[test]
fn deterministic_wfi_preserves_fraction_and_counter_offset_without_jumping() {
    for processor in HOST_PROCESSORS {
        let words = [0x1050_0073, NOP, NOP];
        let mut computer = TestComputer::new(processor, TimingPolicy::PerStep, &words, "timer64");
        computer.set_steps_per_tick(3).unwrap();
        computer.registers().cycle = 1000;
        computer.bus().write(0x0200_4000, 4, 100).unwrap();
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step_phase(), 1);
        assert_eq!(computer.registers().time, 0);
        assert_eq!(computer.registers().cycle, 1000);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step_phase(), 2);
        assert_eq!(computer.step(), None);
        assert_eq!(computer.step_phase(), 0);
        assert_eq!(computer.registers().time, 1);
        assert_eq!(computer.registers().cycle, 1001);
        assert!(!computer
            .bus()
            .interrupt_pending(InterruptLine::MachineTimer));
    }
}
