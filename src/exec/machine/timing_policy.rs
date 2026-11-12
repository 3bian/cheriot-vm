// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use crate::exec::machine::bus::Bus;
use crate::exec::machine::processor::MachineRegisters;

mod host_clock;
mod step_clock;

pub(crate) use host_clock::HostClock;
pub(crate) use step_clock::StepClock;

pub(crate) const DEFAULT_STEPS_PER_TICK: u32 = 1;
pub const DEFAULT_TIMER_HZ: u32 = 2_000;
pub const MAX_TIMER_HZ: u32 = 1_000_000_000;

pub(crate) fn validate_step_period(
    timing: TimingPolicy,
    period: u32,
) -> Result<(), crate::exec::machine::EmulatorError> {
    if period == 0 || (timing == TimingPolicy::Elapsed && period != 1) {
        return Err(crate::exec::machine::EmulatorError::Configuration(
            "steps_per_tick must be positive and requires per-step timing when greater than 1"
                .into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_timer_hz(hz: u32) -> Result<(), crate::exec::machine::EmulatorError> {
    if !(1..=MAX_TIMER_HZ).contains(&hz) {
        return Err(crate::exec::machine::EmulatorError::Configuration(format!(
            "timer_hz must be between 1 and {MAX_TIMER_HZ}"
        )));
    }
    Ok(())
}

fn refresh_interrupts(bus: &mut Bus, registers: &mut MachineRegisters) {
    bus.refresh_interrupts();
    registers.interrupt_pending =
        (registers.interrupt_pending & 0x222) | bus.machine_interrupt_bits();
}

#[inline]
pub(crate) fn synchronize_machine_time(bus: &mut Bus, registers: &mut MachineRegisters) {
    if let Some(time) = bus.take_machine_time() {
        registers.time = time;
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TimingPolicy {
    #[default]
    Elapsed,
    PerStep,
}

impl TimingPolicy {
    pub(crate) fn checkpoint_id(self) -> u8 {
        match self {
            Self::PerStep => 0,
            Self::Elapsed => 2,
        }
    }

    #[inline]
    pub(crate) fn advance(
        self,
        bus: &mut Bus,
        registers: &mut MachineRegisters,
        clock: &mut StepClock,
        inhibit_before: u32,
        writes_cycle: bool,
    ) {
        synchronize_machine_time(bus, registers);
        let tick = clock.advance();
        if self == Self::PerStep && !tick {
            return;
        }
        if inhibit_before & 1 == 0 && !writes_cycle {
            registers.cycle = registers.cycle.wrapping_add(1);
        }
        if self == Self::PerStep {
            registers.time = registers.time.wrapping_add(1);
            bus.tick_devices(1);
        }
    }

    pub(crate) fn wait_for_interrupt(
        self,
        bus: &mut Bus,
        registers: &mut MachineRegisters,
        clock: &mut HostClock,
    ) {
        match self {
            Self::PerStep => (),
            Self::Elapsed => {
                bus.poll_host_inputs();
                refresh_interrupts(bus, registers);
                if ![
                    crate::exec::machine::devices::InterruptLine::MachineExternal,
                    crate::exec::machine::devices::InterruptLine::MachineSoftware,
                    crate::exec::machine::devices::InterruptLine::MachineTimer,
                ]
                .into_iter()
                .any(|line| bus.interrupt_pending(line))
                {
                    std::thread::sleep(clock.wait_quantum());
                }
                clock.synchronize(bus, registers);
                bus.poll_host_inputs();
                refresh_interrupts(bus, registers);
            }
        }
    }
}
