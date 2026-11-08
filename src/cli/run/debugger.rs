// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use gdbstub::arch::Arch;
use gdbstub::common::Signal;
use gdbstub::stub::state_machine::GdbStubStateMachine;
use gdbstub::stub::{DisconnectReason, GdbStub, SingleThreadStopReason};

use cheriot_vm::cheriot::debugger::Target as CheriotTarget;
use cheriot_vm::cheriot::CheriotSystem;
use cheriot_vm::debugger::{BreakpointType, SessionTarget, StopReason};
use cheriot_vm::execution::TrapPolicy;
use cheriot_vm::host::debugger::connection::DebugConnection;
use cheriot_vm::host::debugger::listener;
use cheriot_vm::rv32::debugger::Target as Rv32Target;
use cheriot_vm::{Computer, EmulatorError};

use self::session_output::SessionOutput;

mod session_output;

pub(crate) fn run_cheriot(
    system: CheriotSystem,
    address: SocketAddr,
    steps: Option<u64>,
    raw: bool,
    mut uart: crate::cli::uart::connections::UartConnections,
) -> Result<(), EmulatorError> {
    let (target, reason) = run_session(CheriotTarget::new(system, steps), address, raw, &mut uart)?;
    match reason {
        DisconnectReason::Disconnect => super::cheriot::run_with_options(
            target.into_system(),
            crate::cli::run_options::RunOptions {
                uart,
                steps,
                breakpoint: None,
                trace: false,
                registers: None,
                memory_dump: None,
                presentation: crate::cli::output::OutputPresentation::debugger(raw),
                trap_policy: TrapPolicy::Vector,
            },
        )
        .map(|_| ()),
        other => {
            uart.restore()?;
            finish(other)
        }
    }
}

pub(crate) fn run_rv32(
    mut computer: Computer,
    address: SocketAddr,
    steps: Option<u64>,
    raw: bool,
    mut uart: crate::cli::uart::connections::UartConnections,
) -> Result<(), EmulatorError> {
    computer.trap_policy = TrapPolicy::Vector;
    let (target, reason) = run_session(Rv32Target::new(computer, steps), address, raw, &mut uart)?;
    match reason {
        DisconnectReason::Disconnect => super::rv32::run_with_options(
            target.into_computer(),
            crate::cli::run_options::RunOptions {
                uart,
                steps,
                breakpoint: None,
                trace: false,
                registers: None,
                memory_dump: None,
                presentation: crate::cli::output::OutputPresentation::debugger(raw),
                trap_policy: TrapPolicy::Vector,
            },
        )
        .map(|_| ()),
        other => {
            uart.restore()?;
            finish(other)
        }
    }
}

fn failure(error: impl std::fmt::Display) -> EmulatorError {
    EmulatorError::Execution(error.to_string())
}

fn finish(reason: DisconnectReason) -> Result<(), EmulatorError> {
    match reason {
        DisconnectReason::TargetExited(0) => Ok(()),
        DisconnectReason::TargetExited(code) => {
            Err(failure(format!("guest exited with status {code}")))
        }
        DisconnectReason::Kill => Err(failure("debugger killed the guest")),
        other => Err(failure(format!("debugger session ended: {other:?}"))),
    }
}

fn run_session<T>(
    mut target: T,
    address: SocketAddr,
    raw: bool,
    uart: &mut crate::cli::uart::connections::UartConnections,
) -> Result<(T, DisconnectReason), EmulatorError>
where
    T: SessionTarget,
    T::Arch: Arch<Usize = u32>,
{
    let listener = listener::listen(address)?;
    let mut output = SessionOutput::new(uart, raw)?;
    output.ready(listener.local_addr()?)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                output.pump()?;
                if Instant::now() >= deadline {
                    return Err(failure("debugger connection timeout after 30 seconds"));
                }
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error.into()),
        }
    };
    drop(listener);
    let connection = DebugConnection::new(stream)?;
    let mut packet_buffer = [0_u8; 2048];
    let stub = GdbStub::builder(connection)
        .with_packet_buffer(&mut packet_buffer)
        .build()
        .map_err(failure)?;
    let mut state = stub.run_state_machine(&mut target).map_err(failure)?;
    let mut pending_completion: Option<(u64, Instant)> = None;
    let session = (|| -> Result<DisconnectReason, EmulatorError> {
        Ok(loop {
            output.pump()?;
            if pending_completion.is_some_and(|(_, started)| {
                output.pending() != 0 && started.elapsed() >= Duration::from_secs(1)
            }) {
                return Err(failure(
                    "debugger output drain timed out; guest success not reported",
                ));
            }
            state = match state {
                GdbStubStateMachine::Idle(mut stub) => {
                    stub.borrow_conn().pump()?;
                    if let Some(byte) = stub.borrow_conn().next_byte()? {
                        match stub.incoming_data(&mut target, byte).map_err(failure)? {
                            GdbStubStateMachine::CtrlCInterrupt(stub) => stub
                                .interrupt_handled(&mut target, None::<SingleThreadStopReason<u32>>)
                                .map_err(failure)?,
                            state => state,
                        }
                    } else {
                        thread::sleep(Duration::from_millis(1));
                        stub.into()
                    }
                }
                GdbStubStateMachine::Running(mut stub) => {
                    stub.borrow_conn().pump()?;
                    if let Some(byte) = stub.borrow_conn().next_byte()? {
                        stub.incoming_data(&mut target, byte).map_err(failure)?
                    } else if let Some((code, _)) = pending_completion {
                        if output.pending() == 0 {
                            stub.report_stop(&mut target, wire_stop(StopReason::Completed(code)))
                                .map_err(failure)?
                        } else {
                            thread::sleep(Duration::from_millis(1));
                            stub.into()
                        }
                    } else {
                        let mut stop = None;
                        if output.pending() > SessionOutput::CAPACITY - 8192 {
                            stop = Some(target.halt(StopReason::OutputLimit));
                        } else {
                            for input in &mut uart.inputs {
                                target.poll_input(input)?;
                            }
                            for _ in 0..64 {
                                stop = target.advance()?;
                                if stop.is_some() {
                                    break;
                                }
                            }
                            output.collect(&mut target)?;
                        }
                        match stop {
                            Some(StopReason::Completed(code)) if output.pending() != 0 => {
                                pending_completion = Some((code, Instant::now()));
                                stub.into()
                            }
                            Some(reason) => stub
                                .report_stop(&mut target, wire_stop(reason))
                                .map_err(failure)?,
                            None => stub.into(),
                        }
                    }
                }
                GdbStubStateMachine::CtrlCInterrupt(stub) => {
                    target.halt(StopReason::Interrupted);
                    stub.interrupt_handled(
                        &mut target,
                        Some(SingleThreadStopReason::Signal(Signal::SIGINT)),
                    )
                    .map_err(failure)?
                }
                GdbStubStateMachine::Disconnected(mut stub) => {
                    let deadline = Instant::now() + Duration::from_secs(1);
                    while stub.borrow_conn().pending() != 0 {
                        stub.borrow_conn().pump()?;
                        if Instant::now() >= deadline {
                            return Err(failure("debugger final reply timed out"));
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    break stub.get_reason();
                }
            };
        })
    })();
    let killed = matches!(session, Ok(DisconnectReason::Kill));
    let session = session.and_then(|reason| {
        if matches!(reason, DisconnectReason::Kill) {
            Err(failure("debugger killed the guest"))
        } else {
            Ok(reason)
        }
    });
    let collected = output.collect(&mut target);
    let timeout = if killed {
        Duration::from_millis(100)
    } else {
        Duration::from_secs(1)
    };
    let flushed = output.flush(timeout);
    let reason = session?;
    collected?;
    flushed?;
    if matches!(reason, DisconnectReason::Disconnect) {
        if let Some((code, _)) = pending_completion {
            return if code == 0 {
                Ok((target, DisconnectReason::TargetExited(0)))
            } else {
                Err(failure(format!("guest exited with status {code}")))
            };
        }
    }
    Ok((target, reason))
}

fn wire_stop(reason: StopReason) -> SingleThreadStopReason<u32> {
    match reason {
        StopReason::Breakpoint(_, BreakpointType::Software) => SingleThreadStopReason::SwBreak(()),
        StopReason::Breakpoint(_, BreakpointType::Hardware) => SingleThreadStopReason::HwBreak(()),
        StopReason::Completed(code) => {
            SingleThreadStopReason::Exited(u8::try_from(code).unwrap_or(255))
        }
        StopReason::Ebreak(_) | StopReason::Fault(3) => {
            SingleThreadStopReason::Signal(Signal::SIGTRAP)
        }
        StopReason::Fault(_) => SingleThreadStopReason::Signal(Signal::SIGSEGV),
        StopReason::Interrupted => SingleThreadStopReason::Signal(Signal::SIGINT),
        StopReason::OutputLimit => SingleThreadStopReason::Signal(Signal::SIGXFSZ),
        StopReason::RunLimit => SingleThreadStopReason::Signal(Signal::SIGXCPU),
        StopReason::Step | StopReason::Startup => SingleThreadStopReason::DoneStep,
    }
}
