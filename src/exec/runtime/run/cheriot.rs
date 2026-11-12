// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::Write;

use crate::exec::machine::EmulatorError;
use crate::exec::machine::TimingPolicy;
use crate::exec::machine::bus::Bus;
use crate::exec::machine::cheriot::{Capability, CheriotSystem, StepOutcome};
use crate::exec::machine::completion::HostInterface;
use crate::exec::machine::execution::{ExecutionReport, StepReport, StopReason, TrapPolicy};

use super::run_target::RunTarget;
use crate::exec::runtime::output::{GuestOutput, OutputPresentation};
use crate::exec::runtime::register::Register;
use crate::exec::runtime::run_options::RunOptions;
use crate::exec::runtime::runtime_io;

pub(crate) fn run_with_options(
    system: CheriotSystem,
    options: RunOptions,
) -> Result<(CheriotSystem, ExecutionReport), EmulatorError> {
    super::run_with_options(system, options)
}

impl RunTarget for CheriotSystem {
    type Outcome = StepOutcome;

    const FLUSH_WHEN_TRACING: bool = true;
    const REPORT_TRAPS: bool = true;

    fn stack_pointer(&self) -> u32 {
        self.computer
            .processor
            .read_register(2)
            .map_or(0, |capability| capability.address())
    }

    fn position(&self) -> (u32, u64) {
        (
            self.computer.processor.program_counter,
            self.computer.attempted_steps,
        )
    }

    fn execution_report(&self, stop: StopReason) -> ExecutionReport {
        CheriotSystem::execution_report(self, stop)
    }

    fn handle_step(
        &mut self,
        outcome: &StepOutcome,
        options: &RunOptions,
        scratch: &mut Vec<u8>,
        output: &mut GuestOutput<impl Write>,
        traps: &mut u64,
    ) -> Result<Option<StopReason>, EmulatorError> {
        match *outcome {
            StepOutcome::Retired { address, encoded } if options.trace => {
                output.diagnostic(
                    &format!("0x{address:08x} [0x{encoded:08x}]\n"),
                    options.presentation,
                )?;
            }
            StepOutcome::Trapped {
                address,
                cause,
                value,
            } => {
                *traps = traps.saturating_add(1);
                if options.trace {
                    output.diagnostic(
                        &format!(
                            "Trap at 0x{address:08x}: cause 0x{cause:08x}, value 0x{value:08x}\n"
                        ),
                        options.presentation,
                    )?;
                }
                if options.trap_policy == TrapPolicy::Stop {
                    if cause == 3 {
                        runtime_io::drain_output(
                            &mut self.computer.bus,
                            &mut self.host_interface,
                            scratch,
                            output,
                            options.presentation,
                        )?;
                        output.flush()?;
                        output.diagnostic(
                            &format!("Guest EBREAK at 0x{address:08x}\n"),
                            options.presentation,
                        )?;
                        return Ok(Some(StopReason::GuestEbreak { address }));
                    }
                    return Err(EmulatorError::CheriotTrap {
                        address,
                        cause,
                        value,
                    });
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn io_parts(&mut self) -> (&mut Bus, &mut Option<HostInterface>) {
        (&mut self.computer.bus, &mut self.host_interface)
    }

    fn is_waiting(&self, outcome: &StepOutcome) -> bool {
        self.computer.timing == TimingPolicy::Elapsed
            && matches!(
                outcome,
                StepOutcome::Retired {
                    encoded: 0x1050_0073,
                    ..
                }
            )
    }

    fn present_state(
        &self,
        options: &RunOptions,
        output: &mut GuestOutput<impl Write>,
    ) -> Result<(), EmulatorError> {
        if let Some(selection) = &options.registers {
            let processor = &self.computer.processor;
            if selection.includes(Register::Pc) {
                output.diagnostic(
                    &format!("Program counter: 0x{:08x}\n", processor.program_counter),
                    options.presentation,
                )?;
            }
            for (index, capability) in processor.registers().iter().enumerate() {
                if selection.includes(Register::Capability(index as u8)) {
                    print_capability(
                        &format!("c{index:02}"),
                        *capability,
                        output,
                        options.presentation,
                    )?;
                }
            }
            for (register, name, capability) in [
                (Register::Pcc, "PCC", processor.pcc),
                (Register::Mtcc, "MTCC", processor.machine_trap),
                (Register::Mtdc, "MTDC", processor.machine_data),
                (Register::Mscratchc, "MScratchC", processor.machine_scratch),
                (Register::Mepcc, "MEPCC", processor.machine_exception),
            ] {
                if selection.includes(register) {
                    print_capability(name, capability, output, options.presentation)?;
                }
            }
        }
        Ok(())
    }

    fn step_report(&mut self) -> Result<StepReport<StepOutcome>, EmulatorError> {
        CheriotSystem::step_report(self)
    }
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
