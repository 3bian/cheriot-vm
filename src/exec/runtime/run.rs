// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

use crate::exec::machine::EmulatorError;
use crate::exec::machine::execution::{ExecutionReport, StopReason};

use crate::exec::runtime::output::GuestOutput;
use crate::exec::runtime::run::run_target::RunTarget;
use crate::exec::runtime::run_options::RunOptions;
use crate::exec::runtime::runtime_io;

pub(crate) mod cheriot;
#[cfg(all(feature = "gdb", unix))]
pub(crate) mod debugger;
mod run_target;
pub(crate) mod rv32;

fn run_with_options<T: RunTarget>(
    mut target: T,
    mut options: RunOptions,
) -> Result<(T, ExecutionReport), EmulatorError> {
    let maximum_steps = options.steps.filter(|limit| *limit != 0);
    let mut scratch = Vec::new();
    let mut traps = 0_u64;
    let mut output = GuestOutput::new(io::stdout().lock())
        .with_uart_outputs(std::mem::take(&mut options.uart.outputs));
    let presentation = options.presentation;
    let result = (|| -> Result<StopReason, EmulatorError> {
        loop {
            let (address, attempted_steps) = target.position();
            if let Some(stop) = stop_before_step(
                address,
                attempted_steps,
                &options,
                maximum_steps,
                &mut output,
            )? {
                return Ok(stop);
            }
            if attempted_steps.is_multiple_of(1000) {
                poll_uart_inputs(&mut target, &mut options)?;
            }
            if let Some(insights) = &options.insights {
                insights
                    .borrow_mut()
                    .record(address, target.stack_pointer());
            }
            let report = match target.step_report() {
                Ok(report) => report,
                Err(error) => return target.handle_error(error, &mut output, presentation),
            };
            let waiting = target.is_waiting(&report.outcome);
            if waiting {
                poll_uart_inputs(&mut target, &mut options)?;
            }
            if let Some(stop) = target.handle_step(
                &report.outcome,
                &options,
                &mut scratch,
                &mut output,
                &mut traps,
            )? {
                return Ok(stop);
            }
            if target.position().1.is_multiple_of(1000) || waiting || report.completion.is_some() {
                let (bus, host_interface) = target.io_parts();
                runtime_io::drain_output(
                    bus,
                    host_interface,
                    &mut scratch,
                    &mut output,
                    presentation,
                )?;
                if waiting || (T::FLUSH_WHEN_TRACING && options.trace) {
                    output.flush()?;
                }
            }
            if let Some(code) = report.completion {
                return Ok(StopReason::GuestCompletion { code });
            }
        }
    })();
    let (bus, host_interface) = target.io_parts();
    let stopped = runtime_io::finish_output(
        result,
        bus,
        host_interface,
        &mut scratch,
        &mut output,
        presentation,
    );
    let restored = options.uart.restore();
    let stop = stopped?;
    restored?;
    target.present_state(&options, &mut output)?;
    if let Some((start, count)) = options.memory_dump {
        runtime_io::print_memory(target.io_parts().0, start, count, &mut output, presentation)?;
    }
    output.flush()?;
    let report = target.execution_report(stop);
    runtime_io::finish_report(
        &report,
        target.position().0,
        T::REPORT_TRAPS.then_some(traps),
    )?;
    Ok((target, report))
}

fn poll_uart_inputs<T: RunTarget>(
    target: &mut T,
    options: &mut RunOptions,
) -> Result<(), EmulatorError> {
    for input in &mut options.uart.inputs {
        input.poll(target.io_parts().0)?;
    }
    Ok(())
}

#[inline(always)]
fn stop_before_step(
    address: u32,
    attempted_steps: u64,
    options: &RunOptions,
    maximum_steps: Option<u64>,
    output: &mut GuestOutput<impl Write>,
) -> Result<Option<StopReason>, EmulatorError> {
    if options.breakpoint.contains(&address) {
        output.diagnostic(
            &format!("Breakpoint at 0x{address:08x}\n"),
            options.presentation,
        )?;
        return Ok(Some(StopReason::Breakpoint { address }));
    }
    Ok(maximum_steps
        .filter(|limit| attempted_steps >= *limit)
        .map(|_| StopReason::BudgetExhausted))
}

#[cfg(test)]
#[path = "../machine/tests/cli/run.rs"]
mod tests;
