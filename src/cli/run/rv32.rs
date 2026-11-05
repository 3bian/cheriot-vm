// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;

use cheriot_vm::execution::{ExecutionReport, StopReason};
use cheriot_vm::rv32::StepOutcome;
use cheriot_vm::{Computer, EmulatorError, ExceptionCause};

use crate::cli::output::GuestOutput;
use crate::cli::register::Register;
use crate::cli::run_options::RunOptions;
use crate::cli::runtime_io;

pub(crate) fn run_with_options(
    mut computer: Computer,
    mut options: RunOptions,
) -> Result<(Computer, ExecutionReport), EmulatorError> {
    let RunOptions {
        steps,
        breakpoint,
        trace,
        registers,
        memory_dump,
        presentation,
        trap_policy,
        ..
    } = options;
    computer.trap_policy = trap_policy;
    let maximum_steps = steps.filter(|limit| *limit != 0);
    let mut scratch = Vec::new();
    let mut output = GuestOutput::new(io::stdout().lock())
        .with_uart_outputs(std::mem::take(&mut options.uart.outputs));
    let result = (|| -> Result<StopReason, EmulatorError> {
        loop {
            let address = computer.processor.program_counter;
            if breakpoint == Some(address) {
                output.diagnostic(&format!("Breakpoint at 0x{address:08x}\n"), presentation)?;
                return Ok(StopReason::Breakpoint { address });
            }
            if maximum_steps.is_some_and(|limit| computer.attempted_steps >= limit) {
                return Ok(StopReason::BudgetExhausted);
            }
            for input in &mut options.uart.inputs {
                if computer.attempted_steps.is_multiple_of(1000) {
                    input.poll(&mut computer.bus)?;
                }
            }
            let report = match computer.step_report() {
                Err(EmulatorError::Trap {
                    cause: ExceptionCause::Breakpoint,
                    address,
                    ..
                }) => {
                    output
                        .diagnostic(&format!("Guest EBREAK at 0x{address:08x}\n"), presentation)?;
                    return Ok(StopReason::GuestEbreak { address });
                }
                result => result?,
            };
            match report.outcome {
                StepOutcome::Executed { address, decoded } if trace => {
                    output.diagnostic(
                        &format!(
                            "0x{address:08x} [{:04x}/{}]: {:?}\n",
                            decoded.encoded, decoded.length, decoded.instruction
                        ),
                        presentation,
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
                            if interrupt { "interrupt" } else { "exception" }
                        ),
                        presentation,
                    )?;
                }
                _ => {}
            }
            if computer.attempted_steps.is_multiple_of(1000) || report.completion.is_some() {
                runtime_io::drain_output(
                    &mut computer.bus,
                    &mut computer.host_interface,
                    &mut scratch,
                    &mut output,
                    presentation,
                )?;
            }
            if let Some(code) = report.completion {
                return Ok(StopReason::GuestCompletion { code });
            }
        }
    })();
    let stopped = runtime_io::finish_output(
        result,
        &mut computer.bus,
        &mut computer.host_interface,
        &mut scratch,
        &mut output,
        presentation,
    );
    let restored = options.uart.restore();
    let stop = stopped?;
    restored?;
    output.diagnostic(
        &format!(
            "PC=0x{:08x} retired={} cycles={} virtual_ticks={}\n",
            computer.processor.program_counter,
            computer.processor.machine_registers.retired,
            computer.processor.machine_registers.cycle,
            computer.processor.machine_registers.time
        ),
        presentation,
    )?;
    if let Some(selection) = registers {
        for (index, value) in computer.processor.registers().iter().enumerate() {
            if !selection.includes(Register::Integer(index as u8)) {
                continue;
            }
            output.diagnostic(
                &format!("x{index:02}: 0x{value:08x} ({})\n", *value as i32),
                presentation,
            )?;
        }
    }
    if let Some((start, count)) = memory_dump {
        runtime_io::print_memory(&computer.bus, start, count, &mut output, presentation)?;
    }
    output.flush()?;
    let report = computer.execution_report(stop);
    runtime_io::finish_report(&report, computer.processor.program_counter, None)?;
    Ok((computer, report))
}
