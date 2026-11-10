// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::completion::HostInterface;
use crate::error::{EmulatorError, ExceptionCause};
use crate::execution::{ExecutionReport, StepReport, StopReason, TrapPolicy};
use crate::instructions::{decode_compressed, decode_standard, DecodedInstruction, Instruction};
use crate::rv32::Processor;
use crate::timing_policy::{HostClock, StepClock, TimingPolicy};

use super::StepOutcome;

pub use crate::instructions::arithmetic::{
    perform_immediate_operation, perform_register_operation,
};

pub struct Computer {
    pub attempted_steps: u64,
    pub bus: Bus,
    pub completion: Option<u64>,
    pub fetched_instructions: u64,
    pub host_interface: Option<HostInterface>,
    host_clock: HostClock,
    pub last_store: Option<(u32, usize)>,
    pub processor: Processor,
    step_clock: StepClock,
    pub timing: TimingPolicy,
    pub trap_policy: TrapPolicy,
}

impl Computer {
    pub fn new(processor: Processor, bus: Bus) -> Self {
        Self {
            attempted_steps: 0,
            completion: None,
            host_interface: None,
            host_clock: HostClock::default(),
            last_store: None,
            processor,
            bus,
            fetched_instructions: 0,
            step_clock: StepClock::default(),
            timing: TimingPolicy::default(),
            trap_policy: TrapPolicy::Vector,
        }
    }

    pub fn drain_output(&mut self, output: &mut Vec<u8>) {
        self.bus.drain_output(output);
        if let Some(interface) = &mut self.host_interface {
            interface.drain_transmitted(output);
        }
    }

    pub fn execution_report(&self, stop: StopReason) -> ExecutionReport {
        ExecutionReport {
            attempted_steps: self.attempted_steps,
            fetched_instructions: self.fetched_instructions,
            retired_instructions: self.processor.machine_registers.retired,
            stop,
            virtual_time: self.processor.machine_registers.time,
        }
    }

    pub fn set_steps_per_tick(&mut self, steps: u32) -> Result<(), crate::EmulatorError> {
        self.step_clock.set_period(self.timing, steps)
    }

    pub fn step_phase(&self) -> u32 {
        self.step_clock.phase()
    }

    pub fn steps_per_tick(&self) -> u32 {
        self.step_clock.period()
    }

    pub fn timer_hz(&self) -> u32 {
        self.host_clock.frequency()
    }

    pub fn set_timer_hz(&mut self, hz: u32) -> Result<(), EmulatorError> {
        self.host_clock.set_frequency(hz)
    }

    #[cfg(feature = "verification")]
    pub fn set_monotonic_clock(
        &mut self,
        source: impl Fn() -> std::time::Instant + 'static,
    ) -> Result<(), EmulatorError> {
        self.host_clock.set_source(source)
    }

    #[cfg(feature = "gdb")]
    pub(crate) fn pause_host_clock(&mut self) {
        if self.timing == TimingPolicy::Elapsed {
            self.host_clock
                .pause(&mut self.bus, &mut self.processor.machine_registers);
        }
    }

    #[cfg(feature = "gdb")]
    pub(crate) fn resume_host_clock(&mut self) {
        if self.timing == TimingPolicy::Elapsed {
            self.host_clock.resume();
        }
    }

    pub fn step(&mut self) -> Result<StepOutcome, EmulatorError> {
        self.step_core(None)
    }

    #[cfg(feature = "verification")]
    pub fn step_injected(&mut self, encoded: u32) -> Result<StepOutcome, EmulatorError> {
        self.step_core(Some(encoded))
    }

    fn step_core(&mut self, injected: Option<u32>) -> Result<StepOutcome, EmulatorError> {
        if self.timing == TimingPolicy::Elapsed {
            self.host_clock
                .synchronize(&mut self.bus, &mut self.processor.machine_registers);
        } else {
            crate::timing_policy::synchronize_machine_time(
                &mut self.bus,
                &mut self.processor.machine_registers,
            );
            self.host_clock.reset();
        }
        self.attempted_steps = self.attempted_steps.saturating_add(1);
        self.last_store = None;
        let inhibit = self.processor.machine_registers.counter_inhibit;
        let mut fetched = false;
        let result = self.step_inner(&mut fetched, injected);
        let written_csr = match &result {
            Ok(StepOutcome::Executed { decoded, .. }) => decoded.instruction.csr_write_address(),
            _ => None,
        };

        if inhibit & 4 == 0
            && matches!(result, Ok(StepOutcome::Executed { .. }))
            && !matches!(written_csr, Some(0xb02 | 0xb82))
        {
            self.processor.machine_registers.retired =
                self.processor.machine_registers.retired.wrapping_add(1);
        }

        let writes_cycle = matches!(written_csr, Some(0xb00 | 0xb80));
        self.fetched_instructions = self.fetched_instructions.wrapping_add(u64::from(fetched));
        self.timing.advance(
            &mut self.bus,
            &mut self.processor.machine_registers,
            &mut self.step_clock,
            inhibit,
            writes_cycle,
        );

        if let (Some(interface), Some((address, width))) =
            (&mut self.host_interface, self.last_store)
        {
            if let Some(code) = interface.observe_store(&mut self.bus, address, width)? {
                self.completion = Some(code);
            }
        }
        result
    }

    pub fn step_report(&mut self) -> Result<StepReport<StepOutcome>, EmulatorError> {
        self.bus.check_host_errors()?;
        let prior_completion = self.completion;
        let outcome = self.step()?;
        self.bus.check_host_errors()?;
        let completion = self.completion.filter(|_| prior_completion.is_none());
        Ok(StepReport {
            completion,
            outcome,
        })
    }

    fn checked_target(&self, address: u32) -> Result<u32, EmulatorError> {
        if !address.is_multiple_of(self.processor.profile.minimum_alignment()) {
            return Err(EmulatorError::Trap {
                cause: ExceptionCause::InstructionAddressMisaligned,
                address: self.processor.program_counter,
                value: address,
            });
        }

        Ok(address)
    }

    fn execute(&mut self, decoded: DecodedInstruction) -> Result<u32, EmulatorError> {
        use Instruction::*;
        let address = self.processor.program_counter;
        let following = address.wrapping_add(decoded.length);
        let read = |number| self.processor.read_register_unchecked(number);

        match decoded.instruction {
            Register {
                operation,
                destination,
                first,
                second,
            } => {
                let first_value = read(first);
                let second_value = read(second);
                let result = perform_register_operation(operation, first_value, second_value);
                self.processor.write_register_unchecked(destination, result);
                Ok(following)
            }
            Immediate {
                operation,
                destination,
                source,
                immediate,
            } => {
                let first_value = read(source);
                let result = perform_immediate_operation(operation, first_value, immediate);
                self.processor.write_register_unchecked(destination, result);
                Ok(following)
            }
            Load {
                kind,
                destination,
                base,
                offset,
            } => {
                let load_address = read(base).wrapping_add(offset as u32);
                let width = kind.width();
                if load_address % width as u32 != 0 {
                    return Err(EmulatorError::Trap {
                        cause: ExceptionCause::LoadAddressMisaligned,
                        address,
                        value: load_address,
                    });
                }
                let value = self.bus.read(load_address, width)?;
                let result = kind.extend(value);
                self.processor.write_register_unchecked(destination, result);
                Ok(following)
            }
            Store {
                kind,
                source,
                base,
                offset,
            } => {
                let store_address = read(base).wrapping_add(offset as u32);
                let width = kind.width();
                if store_address % width as u32 != 0 {
                    return Err(EmulatorError::Trap {
                        cause: ExceptionCause::StoreAddressMisaligned,
                        address,
                        value: store_address,
                    });
                }
                let value = read(source);
                self.bus.write(store_address, width, value)?;
                self.last_store = Some((store_address, width));
                Ok(following)
            }
            Branch {
                condition,
                first,
                second,
                offset,
            } => {
                let first_value = read(first);
                let second_value = read(second);
                let taken = condition.test(first_value, second_value);
                if taken {
                    self.checked_target(address.wrapping_add(offset as u32))
                } else {
                    Ok(following)
                }
            }
            JumpAndLink {
                destination,
                offset,
            } => {
                let target = self.checked_target(address.wrapping_add(offset as u32))?;
                self.processor
                    .write_register_unchecked(destination, following);
                Ok(target)
            }
            JumpAndLinkRegister {
                destination,
                base,
                offset,
            } => {
                let target = self.checked_target(read(base).wrapping_add(offset as u32) & !1)?;
                self.processor
                    .write_register_unchecked(destination, following);
                Ok(target)
            }
            LoadUpperImmediate { destination, value } => {
                self.processor.write_register_unchecked(destination, value);
                Ok(following)
            }
            AddUpperImmediateToProgramCounter { destination, value } => {
                self.processor
                    .write_register_unchecked(destination, address.wrapping_add(value));
                Ok(following)
            }
            Fence | FenceInstruction | Hint => Ok(following),
            WaitForInterrupt => {
                self.timing.wait_for_interrupt(
                    &mut self.bus,
                    &mut self.processor.machine_registers,
                    &mut self.host_clock,
                );
                Ok(following)
            }
            EnvironmentCall => Err(EmulatorError::Trap {
                cause: ExceptionCause::EnvironmentCall,
                address,
                value: 0,
            }),
            Breakpoint => Err(EmulatorError::Trap {
                cause: ExceptionCause::Breakpoint,
                address,
                value: 0,
            }),
            MachineReturn => Ok(self.processor.machine_registers.machine_return()),
            Csr {
                operation,
                destination,
                source,
                address: csr_address,
                immediate,
            } => {
                let previous = self
                    .processor
                    .machine_registers
                    .read(csr_address)
                    .ok_or_else(|| {
                        EmulatorError::Configuration(format!(
                            "CSR 0x{csr_address:03x} is unsupported"
                        ))
                    })?;
                let source_value = if immediate {
                    u32::from(source)
                } else {
                    read(source)
                };
                if operation.writes(source) {
                    let new_value = operation.apply(previous, source_value);
                    self.processor
                        .machine_registers
                        .write(csr_address, new_value)?;
                }
                self.processor
                    .write_register_unchecked(destination, previous);
                Ok(following)
            }
        }
    }

    fn step_inner(
        &mut self,
        fetched: &mut bool,
        injected: Option<u32>,
    ) -> Result<StepOutcome, EmulatorError> {
        let address = self.processor.program_counter;
        self.bus.refresh_interrupts();
        let registers = &mut self.processor.machine_registers;
        registers.interrupt_pending = self.bus.machine_interrupt_bits();
        let enabled = registers.interrupt_pending & registers.interrupt_enable;
        if registers.machine_status & (1 << 3) != 0 && enabled != 0 {
            let cause = if enabled & (1 << 11) != 0 {
                11
            } else if enabled & (1 << 3) != 0 {
                3
            } else {
                7
            };
            return self.trap_interrupt(address, cause);
        }

        if !address.is_multiple_of(self.processor.profile.minimum_alignment()) {
            return self.trap_exception(
                address,
                ExceptionCause::InstructionAddressMisaligned,
                address,
            );
        }
        let first_halfword =
            match injected.map_or_else(|| self.bus.fetch(address, 2), |word| Ok(word & 0xffff)) {
                Ok(value) => value as u16,
                Err(_) => {
                    return self.trap_exception(
                        address,
                        ExceptionCause::InstructionAccessFault,
                        address,
                    )
                }
            };
        let (decoded, raw_instruction) = if first_halfword & 3 != 3 {
            (
                if self.processor.profile.compressed {
                    decode_compressed(first_halfword)
                } else {
                    None
                },
                first_halfword as u32,
            )
        } else {
            let second_address = address.wrapping_add(2);
            let second_halfword = match injected
                .map_or_else(|| self.bus.fetch(second_address, 2), |word| Ok(word >> 16))
            {
                Ok(value) => value,
                Err(_) => {
                    return self.trap_exception(
                        address,
                        ExceptionCause::InstructionAccessFault,
                        second_address,
                    )
                }
            };
            let word = u32::from(first_halfword) | (second_halfword << 16);
            (
                decode_standard(word, self.processor.profile.multiply_divide),
                word,
            )
        };
        *fetched = true;

        let Some(decoded) = decoded else {
            return self.trap_exception(
                address,
                ExceptionCause::IllegalInstruction,
                raw_instruction,
            );
        };
        if !self.valid_registers(decoded.instruction) {
            return self.trap_exception(
                address,
                ExceptionCause::IllegalInstruction,
                decoded.encoded,
            );
        }
        if matches!(
            decoded.instruction,
            Instruction::Csr { .. } | Instruction::MachineReturn | Instruction::WaitForInterrupt
        ) && !self.processor.profile.control_registers
        {
            return self.trap_exception(
                address,
                ExceptionCause::IllegalInstruction,
                decoded.encoded,
            );
        }
        let next = self.execute(decoded);
        match next {
            Ok(next_address) => {
                self.processor.program_counter = next_address;
                Ok(StepOutcome::Executed { address, decoded })
            }
            Err(EmulatorError::Trap { cause, value, .. }) => {
                self.trap_exception(address, cause, value)
            }
            Err(EmulatorError::Bus {
                address: fault_address,
                ..
            }) => {
                let cause = match decoded.instruction {
                    Instruction::Load { .. } => ExceptionCause::LoadAccessFault,
                    Instruction::Store { .. } => ExceptionCause::StoreAccessFault,
                    _ => ExceptionCause::InstructionAccessFault,
                };
                self.trap_exception(address, cause, fault_address)
            }
            Err(EmulatorError::Configuration(_)) => {
                self.trap_exception(address, ExceptionCause::IllegalInstruction, decoded.encoded)
            }
            Err(error) => Err(error),
        }
    }

    fn trap_exception(
        &mut self,
        address: u32,
        cause: ExceptionCause,
        value: u32,
    ) -> Result<StepOutcome, EmulatorError> {
        self.processor.program_counter =
            self.processor
                .machine_registers
                .enter_trap(address, cause.code(), value, false);
        if self.trap_policy == TrapPolicy::Stop {
            return Err(EmulatorError::Trap {
                cause,
                address,
                value,
            });
        }
        Ok(StepOutcome::Trapped {
            address,
            cause: cause.code(),
            interrupt: false,
        })
    }

    fn trap_interrupt(&mut self, address: u32, cause: u32) -> Result<StepOutcome, EmulatorError> {
        self.processor.program_counter = self
            .processor
            .machine_registers
            .enter_trap(address, cause, 0, true);
        if self.trap_policy == TrapPolicy::Stop {
            return Err(EmulatorError::Interrupt { address, cause });
        }
        Ok(StepOutcome::Trapped {
            address,
            cause,
            interrupt: true,
        })
    }

    fn valid_registers(&self, instruction: Instruction) -> bool {
        let valid = |number| self.processor.validate_register(number);
        match instruction {
            Instruction::Register {
                destination,
                first,
                second,
                ..
            } => valid(destination) && valid(first) && valid(second),
            Instruction::Immediate {
                destination,
                source,
                ..
            } => valid(destination) && valid(source),
            Instruction::Load {
                destination, base, ..
            } => valid(destination) && valid(base),
            Instruction::Store { source, base, .. } => valid(source) && valid(base),
            Instruction::Branch { first, second, .. } => valid(first) && valid(second),
            Instruction::JumpAndLink { destination, .. }
            | Instruction::LoadUpperImmediate { destination, .. }
            | Instruction::AddUpperImmediateToProgramCounter { destination, .. } => {
                valid(destination)
            }
            Instruction::JumpAndLinkRegister {
                destination, base, ..
            } => valid(destination) && valid(base),
            Instruction::Csr {
                destination,
                source,
                immediate,
                ..
            } => valid(destination) && (immediate || valid(source)),
            _ => true,
        }
    }
}
