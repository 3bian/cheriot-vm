// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{Computer, HostInterface, StepOutcome};
use crate::bus::Bus;
use crate::completion::CompletionPolicy;
use crate::configuration::DefaultDevices;
use crate::error::EmulatorError;
use crate::execution::{ExecutionReport, StepReport, StopReason};
use crate::firmware::{load_cheriot1_elf, load_raw};
use crate::MachineConfiguration;

pub struct Cheriot1System {
    pub computer: Computer,
    pub host_interface: Option<HostInterface>,
    pub last_outcome: Option<StepOutcome>,
}

impl Cheriot1System {
    pub fn drain_output(&mut self, output: &mut Vec<u8>) {
        self.computer.bus.drain_output(output);

        if let Some(interface) = &mut self.host_interface {
            interface.drain_transmitted(output);
        }
    }

    pub fn from_elf(bytes: &[u8]) -> Result<Self, EmulatorError> {
        MachineConfiguration::parse(include_str!("../../profiles/cheriot1.toml"))?
            .build_cheriot1(bytes)
    }

    pub fn from_elf_on_bus(bus: Bus, bytes: &[u8]) -> Result<Self, EmulatorError> {
        Self::from_elf_with_policy(
            bus,
            bytes,
            DefaultDevices::Cheriot1Compatibility,
            CompletionPolicy::ElfSymbol,
        )
    }

    pub fn from_elf_with_policy(
        mut bus: Bus,
        bytes: &[u8],
        defaults: DefaultDevices,
        completion: CompletionPolicy,
    ) -> Result<Self, EmulatorError> {
        defaults.install(&mut bus)?;
        let host_interface = completion.build(&bus, Some(bytes))?;
        let entry = load_cheriot1_elf(&mut bus, bytes)?;
        Ok(Self {
            computer: Computer::new(bus, entry),
            host_interface,
            last_outcome: None,
        })
    }

    pub fn from_raw_on_bus(bus: Bus, address: u32, bytes: &[u8]) -> Result<Self, EmulatorError> {
        Self::from_raw_with_policy(
            bus,
            address,
            bytes,
            DefaultDevices::Cheriot1Compatibility,
            CompletionPolicy::Disabled,
        )
    }

    pub fn from_raw_with_policy(
        mut bus: Bus,
        address: u32,
        bytes: &[u8],
        defaults: DefaultDevices,
        completion: CompletionPolicy,
    ) -> Result<Self, EmulatorError> {
        defaults.install(&mut bus)?;
        crate::firmware::validate_raw_entry(&bus, address, bytes, 2)?;
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

    pub fn take_output(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.drain_output(&mut bytes);
        bytes
    }

    pub fn write_checkpoint(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        crate::checkpoint::write_cheriot1(self, output)
    }

    pub fn write_checkpoint_v1(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        output.write_all(b"CHKP0001")?;
        self.write_state(output)?;
        let computer = &self.computer;
        let processor = &computer.processor;
        let machine = &processor.machine_registers;
        let words = [
            computer.fetched_instructions,
            u64::from(machine.counter_inhibit),
            u64::from(machine.exception_program_counter),
            u64::from(machine.instruction_alignment),
            u64::from(machine.isa),
            u64::from(machine.scratch),
            u64::from(machine.trap_vector),
            u64::from(processor.trigger_select),
            u64::from(computer.revocation_base),
            u64::from(computer.revocation_memory_base),
            u64::from(computer.timing == crate::TimingPolicy::Cheriot1Compatible),
        ];
        for word in words {
            output.write_all(&word.to_le_bytes())?;
        }
        output.write_all(&(computer.bus.memory.len() as u32).to_le_bytes())?;
        for memory in &computer.bus.memory {
            memory.write_checkpoint(output)?;
        }
        Ok(())
    }

    pub fn write_state(&self, output: &mut impl std::io::Write) -> std::io::Result<()> {
        let processor = &self.computer.processor;
        let machine = &processor.machine_registers;
        let words = [
            self.computer.attempted_steps,
            u64::from(processor.program_counter),
            u64::from(machine.machine_status),
            u64::from(machine.trap_cause),
            u64::from(machine.trap_value),
            machine.retired,
            u64::from(processor.stack_high_water),
            u64::from(processor.stack_base),
            machine.cycle,
            machine.time,
            u64::from(machine.interrupt_enable),
            u64::from(machine.interrupt_pending),
        ];

        for word in words {
            output.write_all(&word.to_le_bytes())?;
        }

        let specials = [
            processor.pcc,
            processor.machine_trap,
            processor.machine_data,
            processor.machine_scratch,
            processor.machine_exception,
        ];

        for capability in processor.registers().iter().chain(specials.iter()) {
            let (bits, tag) = capability.to_memory();
            output.write_all(&bits.to_le_bytes())?;
            output.write_all(&u64::from(tag).to_le_bytes())?;
        }

        Ok(())
    }
}
