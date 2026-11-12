// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{Computer, HostInterface, StepOutcome};
use crate::exec::machine::bus::Bus;
use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::configuration::DefaultDevices;
use crate::exec::machine::error::EmulatorError;
use crate::exec::machine::execution::{ExecutionReport, StepReport, StopReason};
use crate::exec::machine::firmware::{load_cheriot_elf, load_raw};

pub struct CheriotSystem {
    pub computer: Computer,
    pub host_interface: Option<HostInterface>,
    pub last_outcome: Option<StepOutcome>,
}

impl CheriotSystem {
    pub fn from_elf_with_policy(
        mut bus: Bus,
        bytes: &[u8],
        defaults: DefaultDevices,
        completion: CompletionPolicy,
    ) -> Result<Self, EmulatorError> {
        defaults.install(&mut bus)?;
        let host_interface = completion.build(&bus, Some(bytes))?;
        let entry = load_cheriot_elf(&mut bus, bytes)?;
        Ok(Self {
            computer: Computer::new(bus, entry),
            host_interface,
            last_outcome: None,
        })
    }

    pub fn from_raw_with_policy(
        mut bus: Bus,
        address: u32,
        bytes: &[u8],
        defaults: DefaultDevices,
        completion: CompletionPolicy,
    ) -> Result<Self, EmulatorError> {
        defaults.install(&mut bus)?;
        crate::exec::machine::firmware::validate_raw_entry(&bus, address, bytes, 2)?;
        let host_interface = completion.build(&bus, None)?;
        load_raw(&mut bus, address, bytes)?;
        Ok(Self {
            computer: Computer::new(bus, address),
            host_interface,
            last_outcome: None,
        })
    }

    pub fn execution_report(&self, stop: StopReason) -> ExecutionReport {
        ExecutionReport {
            attempted_steps: self.computer.attempted_steps,
            fetched_instructions: self.computer.fetched_instructions,
            retired_instructions: self.computer.processor.machine_registers.retired,
            stop,
            virtual_time: self.computer.processor.machine_registers.time,
        }
    }

    pub fn step(&mut self) -> Result<Option<u64>, EmulatorError> {
        self.step_report().map(|report| report.completion)
    }

    pub fn step_report(&mut self) -> Result<StepReport<StepOutcome>, EmulatorError> {
        self.computer.bus.check_host_errors()?;
        let outcome = self.computer.step();
        self.last_outcome = Some(outcome);
        self.computer.bus.check_host_errors()?;
        let completion = if let (Some(interface), Some((address, width))) =
            (&mut self.host_interface, self.computer.last_store)
        {
            interface.observe_store(&mut self.computer.bus, address, width)?
        } else {
            None
        };
        Ok(StepReport {
            completion,
            outcome,
        })
    }

    pub fn write_checkpoint(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        crate::exec::machine::checkpoint::write_cheriot(self, output)
    }
}
