// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::Write;

use cheriot_vm::bus::Bus;
use cheriot_vm::completion::HostInterface;
use cheriot_vm::execution::{ExecutionReport, StepReport, StopReason};
use cheriot_vm::EmulatorError;

use crate::cli::output::{GuestOutput, OutputPresentation};
use crate::cli::run_options::RunOptions;

pub(super) trait RunTarget {
    type Outcome;

    const FLUSH_WHEN_TRACING: bool = false;
    const REPORT_TRAPS: bool = false;

    fn position(&self) -> (u32, u64);
    fn execution_report(&self, stop: StopReason) -> ExecutionReport;
    fn handle_step(
        &mut self,
        outcome: &Self::Outcome,
        options: &RunOptions,
        scratch: &mut Vec<u8>,
        output: &mut GuestOutput<impl Write>,
        traps: &mut u64,
    ) -> Result<Option<StopReason>, EmulatorError>;
    fn io_parts(&mut self) -> (&mut Bus, &mut Option<HostInterface>);
    fn is_waiting(&self, outcome: &Self::Outcome) -> bool;
    fn present_state(
        &self,
        options: &RunOptions,
        output: &mut GuestOutput<impl Write>,
    ) -> Result<(), EmulatorError>;
    fn step_report(&mut self) -> Result<StepReport<Self::Outcome>, EmulatorError>;

    fn handle_error(
        &mut self,
        error: EmulatorError,
        _output: &mut GuestOutput<impl Write>,
        _presentation: OutputPresentation,
    ) -> Result<StopReason, EmulatorError> {
        Err(error)
    }
}
