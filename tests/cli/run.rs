// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

use cheriot_vm::bus::Bus;
use cheriot_vm::completion::HostInterface;
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::execution::{ExecutionReport, StepReport, StopReason, TrapPolicy};
use cheriot_vm::EmulatorError;

use super::{run_with_options, stop_before_step, GuestOutput, RunOptions, RunTarget};
use crate::cli::output::OutputPresentation;

fn options() -> RunOptions {
    RunOptions {
        breakpoint: None,
        memory_dump: None,
        presentation: OutputPresentation::Raw,
        registers: None,
        steps: Some(1),
        trace: false,
        trap_policy: TrapPolicy::Vector,
        uart: Default::default(),
    }
}

#[test]
fn breakpoint_precedes_the_budget_without_advancing_the_target() {
    let mut options = options();
    options.breakpoint = Some(0x8000_0004);
    let mut bytes = Vec::new();
    let mut output = GuestOutput::new(&mut bytes);
    let stop = stop_before_step(0x8000_0004, 1, &options, Some(1), &mut output).unwrap();
    assert_eq!(
        stop,
        Some(StopReason::Breakpoint {
            address: 0x8000_0004
        })
    );
    output.flush().unwrap();
}

#[test]
fn output_is_drained_and_the_original_failure_takes_precedence() {
    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut bus = Bus::new();
    bus.add_device("uart0".into(), 0x1000, 256, Box::new(Uart::new()))
        .unwrap();
    bus.write(0x1000, 4, u32::from(b'X')).unwrap();
    let mut output = GuestOutput::new(BrokenWriter);
    let failure = EmulatorError::Execution("original failure".into());
    let result = super::runtime_io::finish_output(
        Err(failure),
        &mut bus,
        &mut None,
        &mut Vec::new(),
        &mut output,
        OutputPresentation::Raw,
    );
    assert!(result.unwrap_err().to_string().contains("original failure"));
    assert!(bus
        .device::<Uart>("uart0")
        .unwrap()
        .transmitted()
        .is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn waiting_step_polls_uart_before_reporting_completion() {
    use cheriot_vm::host::uart::input::UartInput;
    use std::os::fd::AsRawFd;

    struct WaitingTarget {
        bus: Bus,
        host: Option<HostInterface>,
        steps: u64,
        writer: std::fs::File,
    }

    impl RunTarget for WaitingTarget {
        type Outcome = bool;

        fn position(&self) -> (u32, u64) {
            (0x8000_0000, self.steps)
        }
        fn execution_report(&self, stop: StopReason) -> ExecutionReport {
            ExecutionReport {
                attempted_steps: self.steps,
                fetched_instructions: self.steps,
                retired_instructions: self.steps,
                stop,
                virtual_time: 0,
            }
        }
        fn handle_step(
            &mut self,
            _: &bool,
            _: &RunOptions,
            _: &mut Vec<u8>,
            _: &mut GuestOutput<impl Write>,
            _: &mut u64,
        ) -> Result<Option<StopReason>, EmulatorError> {
            Ok(None)
        }
        fn io_parts(&mut self) -> (&mut Bus, &mut Option<HostInterface>) {
            (&mut self.bus, &mut self.host)
        }
        fn is_waiting(&self, outcome: &bool) -> bool {
            *outcome
        }
        fn present_state(
            &self,
            _: &RunOptions,
            _: &mut GuestOutput<impl Write>,
        ) -> Result<(), EmulatorError> {
            Ok(())
        }
        fn step_report(&mut self) -> Result<StepReport<bool>, EmulatorError> {
            self.steps += 1;
            self.writer.write_all(b"Q")?;
            Ok(StepReport {
                outcome: true,
                completion: Some(0),
            })
        }
    }

    let (reader, writer) = nix::unistd::pipe().unwrap();
    let mut bus = Bus::new();
    bus.add_device("uart0".into(), 0x1000, 256, Box::new(Uart::new()))
        .unwrap();
    let path = format!("/proc/self/fd/{}", reader.as_raw_fd());
    let input = UartInput::open(&bus, "uart0", std::path::Path::new(&path)).unwrap();
    let target = WaitingTarget {
        bus,
        host: None,
        steps: 0,
        writer: writer.into(),
    };
    let mut options = options();
    options.uart.inputs.push(input);
    let (mut target, report) = run_with_options(target, options).unwrap();
    assert_eq!(report.stop, StopReason::GuestCompletion { code: 0 });
    assert_eq!(target.bus.read(0x1000, 4).unwrap(), u32::from(b'Q'));
}
