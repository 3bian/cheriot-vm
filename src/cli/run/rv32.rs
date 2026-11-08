// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::Write;

use cheriot_vm::bus::Bus;
use cheriot_vm::completion::HostInterface;
use cheriot_vm::execution::{ExecutionReport, StepReport, StopReason};
use cheriot_vm::instructions::{DecodedInstruction, Instruction};
use cheriot_vm::rv32::StepOutcome;
use cheriot_vm::{Computer, EmulatorError, ExceptionCause, TimingPolicy};

use super::run_target::RunTarget;
use crate::cli::output::{GuestOutput, OutputPresentation};
use crate::cli::register::Register;
use crate::cli::run_options::RunOptions;

pub(crate) fn run_with_options(
    mut computer: Computer,
    options: RunOptions,
) -> Result<(Computer, ExecutionReport), EmulatorError> {
    computer.trap_policy = options.trap_policy;
    super::run_with_options(computer, options)
}

impl RunTarget for Computer {
    type Outcome = StepOutcome;

    fn position(&self) -> (u32, u64) {
        (self.processor.program_counter, self.attempted_steps)
    }

    fn execution_report(&self, stop: StopReason) -> ExecutionReport {
        Computer::execution_report(self, stop)
    }

    fn handle_error(
        &mut self,
        error: EmulatorError,
        output: &mut GuestOutput<impl Write>,
        presentation: OutputPresentation,
    ) -> Result<StopReason, EmulatorError> {
        match error {
            EmulatorError::Trap {
                cause: ExceptionCause::Breakpoint,
                address,
                ..
            } => {
                output.diagnostic(&format!("Guest EBREAK at 0x{address:08x}\n"), presentation)?;
                Ok(StopReason::GuestEbreak { address })
            }
            error => Err(error),
        }
    }

    fn handle_step(
        &mut self,
        outcome: &StepOutcome,
        options: &RunOptions,
        _scratch: &mut Vec<u8>,
        output: &mut GuestOutput<impl Write>,
        _traps: &mut u64,
    ) -> Result<Option<StopReason>, EmulatorError> {
        match outcome {
            StepOutcome::Executed { address, decoded } if options.trace => {
                output.diagnostic(
                    &format!(
                        "0x{address:08x} [{:04x}/{}]: {:?}\n",
                        decoded.encoded, decoded.length, decoded.instruction
                    ),
                    options.presentation,
                )?;
            }
            StepOutcome::Trapped {
                address,
                cause,
                interrupt,
            } => {
                output.diagnostic(
                    &format!(
                        "{} trap cause {cause} at 0x{address:08x}\n",
                        if *interrupt { "interrupt" } else { "exception" }
                    ),
                    options.presentation,
                )?;
            }
            _ => {}
        }
        Ok(None)
    }

    fn io_parts(&mut self) -> (&mut Bus, &mut Option<HostInterface>) {
        (&mut self.bus, &mut self.host_interface)
    }

    fn is_waiting(&self, outcome: &StepOutcome) -> bool {
        self.timing == TimingPolicy::Elapsed
            && matches!(
                outcome,
                StepOutcome::Executed {
                    decoded: DecodedInstruction {
                        instruction: Instruction::WaitForInterrupt,
                        ..
                    },
                    ..
                }
            )
    }

    fn present_state(
        &self,
        options: &RunOptions,
        output: &mut GuestOutput<impl Write>,
    ) -> Result<(), EmulatorError> {
        output.diagnostic(
            &format!(
                "PC=0x{:08x} retired={} cycles={} virtual_ticks={}\n",
                self.processor.program_counter,
                self.processor.machine_registers.retired,
                self.processor.machine_registers.cycle,
                self.processor.machine_registers.time
            ),
            options.presentation,
        )?;
        if let Some(selection) = &options.registers {
            for (index, value) in self.processor.registers().iter().enumerate() {
                if selection.includes(Register::Integer(index as u8)) {
                    output.diagnostic(
                        &format!("x{index:02}: 0x{value:08x} ({})\n", *value as i32),
                        options.presentation,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn step_report(&mut self) -> Result<StepReport<StepOutcome>, EmulatorError> {
        Computer::step_report(self)
    }
}
