// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::time::{Duration, Instant};

use crate::exec::machine::EmulatorError;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::processor::MachineRegisters;
use crate::exec::machine::timing_policy::{
    DEFAULT_TIMER_HZ, refresh_interrupts, validate_timer_hz,
};

const NANOS_PER_SECOND: u128 = 1_000_000_000;
const MAX_WAIT_NANOS: u128 = 1_000_000;

pub(crate) struct HostClock {
    hz: u32,
    last: Option<Instant>,
    paused: bool,
    remainder: u128,
    started: bool,
    #[cfg(feature = "verification")]
    source: Option<Box<dyn Fn() -> Instant>>,
}

impl Default for HostClock {
    fn default() -> Self {
        Self {
            hz: DEFAULT_TIMER_HZ,
            last: None,
            paused: false,
            remainder: 0,
            started: false,
            #[cfg(feature = "verification")]
            source: None,
        }
    }
}

impl HostClock {
    pub(crate) fn frequency(&self) -> u32 {
        self.hz
    }

    pub(crate) fn set_frequency(&mut self, hz: u32) -> Result<(), EmulatorError> {
        validate_timer_hz(hz)?;
        if self.started || self.paused {
            return Err(EmulatorError::Configuration(
                "timer frequency cannot change after the clock starts".into(),
            ));
        }
        self.hz = hz;
        Ok(())
    }

    fn now(&self) -> Instant {
        #[cfg(feature = "verification")]
        if let Some(source) = &self.source {
            return source();
        }
        Instant::now()
    }

    pub(crate) fn pause(&mut self, bus: &mut Bus, registers: &mut MachineRegisters) {
        self.synchronize(bus, registers);
        self.last = None;
        self.paused = true;
    }

    pub(crate) fn reset(&mut self) {
        if self.last.is_some() || self.paused {
            self.last = None;
            self.paused = false;
            self.remainder = 0;
        }
    }

    #[cfg(feature = "gdb")]
    pub(crate) fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.started = true;
            self.last = Some(self.now());
        }
    }

    pub(crate) fn synchronize(&mut self, bus: &mut Bus, registers: &mut MachineRegisters) {
        super::synchronize_machine_time(bus, registers);
        if self.paused {
            return;
        }
        self.started = true;
        let now = self.now();
        let Some(last) = self.last.replace(now) else {
            return;
        };
        let scaled = now.duration_since(last).as_nanos() * u128::from(self.hz) + self.remainder;
        let elapsed_ticks = scaled / NANOS_PER_SECOND;
        self.remainder = scaled % NANOS_PER_SECOND;
        if elapsed_ticks != 0 {
            if elapsed_ticks > u128::from(u64::MAX) {
                bus.tick_devices(u64::MAX);
                bus.tick_devices(1);
            }
            let ticks = elapsed_ticks as u64;
            registers.time = registers.time.wrapping_add(ticks);
            if ticks != 0 {
                bus.tick_devices(ticks);
            }
            refresh_interrupts(bus, registers);
        }
    }

    pub(crate) fn wait_quantum(&self) -> Duration {
        let nanos = (NANOS_PER_SECOND - self.remainder)
            .div_ceil(u128::from(self.hz))
            .min(MAX_WAIT_NANOS);
        Duration::from_nanos(nanos as u64)
    }
}
