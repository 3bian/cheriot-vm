// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use crate::bus::Bus;
use crate::processor::MachineRegisters;

const COMPATIBILITY_INSTRUCTIONS_PER_TICK: u64 = 100;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TimingPolicy {
    #[serde(rename = "cheriot1-compatible")]
    Cheriot1Compatible,
    PerStep,
}

impl TimingPolicy {
    pub(crate) fn advance(
        self,
        bus: &mut Bus,
        registers: &mut MachineRegisters,
        fetched: bool,
        instructions: u64,
        inhibit_before: u32,
        writes_cycle: bool,
    ) {
        let (tick, increment_cycle) = match self {
            Self::Cheriot1Compatible => (
                fetched && instructions.is_multiple_of(COMPATIBILITY_INSTRUCTIONS_PER_TICK),
                registers.counter_inhibit & 1 == 0,
            ),
            Self::PerStep => (true, inhibit_before & 1 == 0 && !writes_cycle),
        };

        if !tick {
            return;
        }

        if increment_cycle {
            registers.cycle = registers.cycle.wrapping_add(1);
        }

        registers.time = registers.time.wrapping_add(1);
        bus.tick_devices(1);
    }

    pub(crate) fn wait_for_interrupt(self, bus: &mut Bus, registers: &mut MachineRegisters) {
        if self == Self::PerStep {
            return;
        }

        if let Some(ticks) = bus.ticks_until_interrupt().filter(|ticks| *ticks > 0) {
            bus.advance_time_without_dispatch(ticks);
            registers.time = registers.time.wrapping_add(ticks);
            registers.cycle = registers.time;
        }
    }
}
