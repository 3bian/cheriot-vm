// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;
use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use gdbstub::arch::Arch;

use crate::debug::gdb::SessionTarget;
use crate::exec::machine::EmulatorError;
use crate::exec::machine::host::debugger::output::DebugOutput;

use super::failure;
use crate::exec::runtime::uart::connections::UartConnections;

pub(super) struct SessionOutput {
    bytes: Vec<u8>,
    diagnostics: DebugOutput,
    raw: bool,
    stdout: DebugOutput,
    uart: Vec<(String, DebugOutput)>,
}

impl SessionOutput {
    pub(super) const CAPACITY: usize = DebugOutput::CAPACITY;

    pub(super) fn new(uart: &UartConnections, raw: bool) -> Result<Self, EmulatorError> {
        let stdout = DebugOutput::new(io::stdout())?;
        let uart = uart
            .outputs
            .iter()
            .map(|binding| {
                DebugOutput::new(&binding.file).map(|output| (binding.device.clone(), output))
            })
            .collect::<Result<_, _>>()?;
        let diagnostics = DebugOutput::new(io::stderr())?;

        Ok(Self {
            bytes: Vec::new(),
            diagnostics,
            raw,
            stdout,
            uart,
        })
    }

    pub(super) fn collect<T: SessionTarget>(&mut self, target: &mut T) -> Result<(), EmulatorError>
    where
        T::Arch: Arch<Usize = u32>,
    {
        let raw = self.raw;
        let stdout = &mut self.stdout;
        let uart = &mut self.uart;

        target.drain_output_each(&mut self.bytes, &mut |kind, name, bytes| {
            let uart_index = uart
                .iter()
                .position(|(device, _)| kind == "UART" && device == name);
            let raw = raw || uart_index.is_some();
            let length = bytes
                .iter()
                .map(|byte| {
                    if raw {
                        1
                    } else {
                        crate::exec::machine::output::debugger_escape(*byte).1
                    }
                })
                .sum::<usize>();
            if length > Self::CAPACITY - pending_output(stdout, uart) {
                return Err(failure("debugger host output budget exhausted"));
            }
            let destination = match uart_index {
                Some(index) => &mut uart[index].1,
                None => &mut *stdout,
            };
            destination.enqueue(bytes, raw).map_err(EmulatorError::from)
        })
    }

    pub(super) fn flush(&mut self, timeout: Duration) -> Result<(), EmulatorError> {
        let deadline = Instant::now() + timeout;
        while self.pending() != 0 || self.diagnostics.pending() != 0 {
            self.pump()?;
            if Instant::now() >= deadline {
                return Err(failure(
                    "debugger output drain timed out; guest success not reported",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    pub(super) fn pending(&self) -> usize {
        pending_output(&self.stdout, &self.uart)
    }

    pub(super) fn pump(&mut self) -> Result<(), EmulatorError> {
        self.stdout.pump()?;
        for (_, channel) in &mut self.uart {
            channel.pump()?;
        }
        self.diagnostics.pump()?;
        Ok(())
    }

    pub(super) fn ready(&mut self, address: SocketAddr) -> Result<(), EmulatorError> {
        self.diagnostics.enqueue(
            format!("CHERIOT_GDB_READY {address} (halted; single session)\n").as_bytes(),
            true,
        )?;
        self.diagnostics.pump()?;
        Ok(())
    }
}

fn pending_output(stdout: &DebugOutput, uart: &[(String, DebugOutput)]) -> usize {
    stdout.pending()
        + uart
            .iter()
            .map(|(_, output)| output.pending())
            .sum::<usize>()
}
