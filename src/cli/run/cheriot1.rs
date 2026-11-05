// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

use cheriot_vm::cheriot1::{Capability, Cheriot1System, StepOutcome};
use cheriot_vm::execution::TrapPolicy;
use cheriot_vm::execution::{ExecutionReport, StopReason};
use cheriot_vm::EmulatorError;

use crate::cli::output::GuestOutput;
use crate::cli::output::OutputPresentation;
use crate::cli::register::Register;
use crate::cli::run_options::RunOptions;
use crate::cli::runtime_io;

pub(crate) fn run_with_options(
    mut system: Cheriot1System,
    mut options: RunOptions,
) -> Result<(Cheriot1System, ExecutionReport), EmulatorError> {
    let maximum_steps = options.steps.filter(|limit| *limit != 0);
    let mut traps = 0_u64;
    let mut bytes = Vec::new();
    let mut output = GuestOutput::new(io::stdout().lock())
        .with_uart_outputs(std::mem::take(&mut options.uart.outputs));
    let presentation = options.presentation;
    let result = (|| -> Result<StopReason, EmulatorError> {
        loop {
            let address = system.computer.processor.program_counter;
            if options.breakpoint == Some(address) {
                output.diagnostic(&format!("Breakpoint at 0x{address:08x}\n"), presentation)?;
                return Ok(StopReason::Breakpoint { address });
            }
            if maximum_steps.is_some_and(|limit| system.computer.attempted_steps >= limit) {
                return Ok(StopReason::BudgetExhausted);
            }
            for input in &mut options.uart.inputs {
                if system.computer.attempted_steps.is_multiple_of(1000) {
                    input.poll(&mut system.computer.bus)?;
                }
            }
            let report = system.step_report()?;
            match report.outcome {
                StepOutcome::Retired { address, encoded } if options.trace => {
                    output.diagnostic(
                        &format!("0x{address:08x} [0x{encoded:08x}]\n"),
                        presentation,
                    )?;
                }
                StepOutcome::Trapped {
                    address,
                    cause,
                    value,
                } => {
                    traps = traps.saturating_add(1);
                    if options.trace {
                        output.diagnostic(&format!("Trap at 0x{address:08x}: cause 0x{cause:08x}, value 0x{value:08x}\n"), presentation)?;
                    }
                    if options.trap_policy == TrapPolicy::Stop {
                        if cause == 3 {
                            runtime_io::drain_output(
                                &mut system.computer.bus,
                                &mut system.host_interface,
                                &mut bytes,
                                &mut output,
                                presentation,
                            )?;
                            output.flush()?;
                            output.diagnostic(
                                &format!("Guest EBREAK at 0x{address:08x}\n"),
                                presentation,
                            )?;
                            return Ok(StopReason::GuestEbreak { address });
                        }
                        return Err(EmulatorError::Cheriot1Trap {
                            address,
                            cause,
                            value,
                        });
                    }
                }
                _ => {}
            }
            if system.computer.attempted_steps.is_multiple_of(1000) || report.completion.is_some() {
                runtime_io::drain_output(
                    &mut system.computer.bus,
                    &mut system.host_interface,
                    &mut bytes,
                    &mut output,
                    presentation,
                )?;
                if options.trace {
                    output.flush()?;
                }
            }
            if let Some(code) = report.completion {
                return Ok(StopReason::GuestCompletion { code });
            }
        }
    })();
    let stopped = runtime_io::finish_output(
        result,
        &mut system.computer.bus,
        &mut system.host_interface,
        &mut bytes,
        &mut output,
        presentation,
    );
    let restored = options.uart.restore();
    let stop = stopped?;
    restored?;
    if let Some(selection) = options.registers {
        let processor = &system.computer.processor;
        if selection.includes(Register::Pc) {
            output.diagnostic(
                &format!("Program counter: 0x{:08x}\n", processor.program_counter),
                presentation,
            )?;
        }
        for (index, capability) in processor.registers().iter().enumerate() {
            if !selection.includes(Register::Capability(index as u8)) {
                continue;
            }
            print_capability(
                &format!("c{index:02}"),
                *capability,
                &mut output,
                presentation,
            )?;
        }
        for (register, name, capability) in [
            (Register::Pcc, "PCC", processor.pcc),
            (Register::Mtcc, "MTCC", processor.machine_trap),
            (Register::Mtdc, "MTDC", processor.machine_data),
            (Register::Mscratchc, "MScratchC", processor.machine_scratch),
            (Register::Mepcc, "MEPCC", processor.machine_exception),
        ] {
            if !selection.includes(register) {
                continue;
            }
            print_capability(name, capability, &mut output, presentation)?;
        }
    }
    if let Some((start, count)) = options.memory_dump {
        runtime_io::print_memory(
            &system.computer.bus,
            start,
            count,
            &mut output,
            presentation,
        )?;
    }
    output.flush()?;
    let report = system.execution_report(stop);
    runtime_io::finish_report(
        &report,
        system.computer.processor.program_counter,
        Some(traps),
    )?;
    Ok((system, report))
}

fn print_capability(
    name: &str,
    capability: Capability,
    output: &mut GuestOutput<impl Write>,
    presentation: OutputPresentation,
) -> Result<(), EmulatorError> {
    let (payload, tag) = capability.to_memory();
    let (base, top) = capability.bounds();
    output.diagnostic(&format!(
        "{name}: address=0x{:08x} payload=0x{payload:016x} tag={} base=0x{base:08x} top=0x{top:09x} permissions=0x{:03x} type={}\n",
        capability.address(), u8::from(tag), capability.permissions(), capability.object_type()), presentation)?;
    Ok(())
}
