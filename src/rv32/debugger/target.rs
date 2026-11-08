// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use gdbstub::common::Signal;
use gdbstub::target::ext::base::single_register_access::{
    SingleRegisterAccess, SingleRegisterAccessOps,
};
use gdbstub::target::ext::base::singlethread::{
    SingleThreadBase, SingleThreadResume, SingleThreadResumeOps, SingleThreadSingleStep,
    SingleThreadSingleStepOps,
};
use gdbstub::target::ext::base::BaseOps;
use gdbstub::target::ext::breakpoints::{
    Breakpoints, BreakpointsOps, HwBreakpoint, HwBreakpointOps, SwBreakpoint, SwBreakpointOps,
};
use gdbstub::target::ext::monitor_cmd::{ConsoleOutput, MonitorCmd, MonitorCmdOps};
use gdbstub::target::{Target as GdbTarget, TargetError, TargetResult};

use super::{Architecture, Fault, RegisterId, Registers};
use crate::debugger::{
    inspect_memory, BreakpointType, RunControl, RunState, SessionTarget, StopReason,
};
use crate::rv32::StepOutcome;
use crate::{Computer, EmulatorError};

pub struct Target {
    control: RunControl,
    fault: Option<Fault>,
    pub reason: StopReason,
    pub state: RunState,
    computer: Computer,
}

impl Target {
    pub fn new(mut computer: Computer, maximum_steps: Option<u64>) -> Self {
        computer.pause_host_clock();
        Self {
            control: RunControl::new(maximum_steps),
            fault: None,
            reason: StopReason::Startup,
            state: RunState::Halted,
            computer,
        }
    }

    pub fn advance(&mut self) -> Result<Option<StopReason>, EmulatorError> {
        if self.state == RunState::Halted {
            return Ok(None);
        }
        let address = self.computer.processor.program_counter;
        if let Some(reason) = self
            .control
            .before_step(address, self.computer.attempted_steps)
        {
            return Ok(Some(self.halt(reason)));
        }
        self.computer.bus.check_host_errors()?;
        let outcome = self.computer.step();
        self.computer.bus.check_host_errors()?;
        let trap = match outcome {
            Ok(StepOutcome::Trapped {
                address,
                cause,
                interrupt,
            }) => Some((
                address,
                cause | if interrupt { 1 << 31 } else { 0 },
                self.computer.processor.machine_registers.trap_value,
            )),
            Err(EmulatorError::Trap {
                address,
                cause,
                value,
            }) => Some((address, cause.code(), value)),
            Err(error) => return Err(error),
            _ => None,
        };
        if let Some((address, cause, value)) = trap {
            self.fault = Some(Fault {
                address,
                cause,
                value,
                resulting_pc: self.computer.processor.program_counter,
            });
            if let Some(reason) = StopReason::from_fault(address, cause) {
                return Ok(Some(self.halt(reason)));
            }
        }
        if let Some(code) = self.computer.completion {
            return Ok(Some(self.halt(StopReason::Completed(code))));
        }
        if self.state == RunState::SingleStep {
            return Ok(Some(self.halt(StopReason::Step)));
        }
        Ok(None)
    }

    pub fn drain_output(&mut self, bytes: &mut Vec<u8>) {
        self.computer.drain_output(bytes);
    }

    pub fn halt(&mut self, reason: StopReason) -> StopReason {
        self.computer.pause_host_clock();
        self.state = RunState::Halted;
        self.reason = reason;
        reason
    }

    pub fn inspect(&self, address: u32, data: &mut [u8]) -> Result<(), EmulatorError> {
        inspect_memory(&self.computer.bus, address, data)
    }

    pub fn into_computer(mut self) -> Computer {
        self.computer.resume_host_clock();
        self.computer
    }

    pub fn monitor(&self, command: &[u8]) -> Result<String, &'static str> {
        let words = crate::debugger::monitor_words(command)?;
        match words.as_slice() {
            ["help"] => Ok("help | fault | status\nInteger registers and CSRs are read-only. Unimplemented E registers and disabled CSRs are unavailable.\n".into()),
            ["fault"] => Ok(match self.fault {
                Some(fault) => format!("fault address=0x{:08x} cause=0x{:08x} value=0x{:08x} resulting_pc=0x{:08x}\n", fault.address, fault.cause, fault.value, fault.resulting_pc),
                None => "no trap observed\n".into(),
            }),
            ["status"] => Ok(format!("{:?}; steps={} retired={} virtual_time={} registers={} compressed={}\n", self.reason, self.computer.attempted_steps, self.computer.processor.machine_registers.retired, self.computer.processor.machine_registers.time, self.computer.processor.profile.register_count(), self.computer.processor.profile.compressed)),
            _ => Err("unknown monitor command; use monitor help"),
        }
    }

    pub fn computer(&self) -> &Computer {
        &self.computer
    }

    fn insert_breakpoint(
        &mut self,
        address: u32,
        kind: usize,
        breakpoint: BreakpointType,
    ) -> TargetResult<bool, Self> {
        if self.control.insert_breakpoint(
            &self.computer.bus,
            breakpoint,
            address,
            kind,
            self.computer.processor.profile.minimum_alignment(),
        ) {
            Ok(true)
        } else {
            Err(TargetError::NonFatal)
        }
    }

    fn remove_breakpoint(
        &mut self,
        address: u32,
        kind: usize,
        breakpoint: BreakpointType,
    ) -> TargetResult<bool, Self> {
        if self.control.remove_breakpoint(
            breakpoint,
            address,
            kind,
            self.computer.processor.profile.minimum_alignment(),
        ) {
            Ok(true)
        } else {
            Err(TargetError::NonFatal)
        }
    }

    fn resume_with(
        &mut self,
        state: RunState,
        signal: Option<Signal>,
    ) -> Result<(), EmulatorError> {
        self.control
            .resume(&mut self.state, self.reason, state, signal)?;
        self.computer.resume_host_clock();
        Ok(())
    }
}

impl GdbTarget for Target {
    type Arch = Architecture;
    type Error = EmulatorError;
    fn use_rle(&self) -> bool {
        false
    }

    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }
    fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
        Some(self)
    }
    fn support_monitor_cmd(&mut self) -> Option<MonitorCmdOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadBase for Target {
    fn read_registers(&mut self, registers: &mut Registers) -> TargetResult<(), Self> {
        *registers = Registers::new(&self.computer.processor);
        Ok(())
    }
    fn write_registers(&mut self, _: &Registers) -> TargetResult<(), Self> {
        Err(TargetError::NonFatal)
    }
    fn read_addrs(&mut self, address: u32, data: &mut [u8]) -> TargetResult<usize, Self> {
        self.inspect(address, data)
            .map_err(|_| TargetError::NonFatal)?;
        Ok(data.len())
    }
    fn write_addrs(&mut self, _: u32, _: &[u8]) -> TargetResult<(), Self> {
        Err(TargetError::NonFatal)
    }
    fn support_resume(&mut self) -> Option<SingleThreadResumeOps<'_, Self>> {
        Some(self)
    }
    fn support_single_register_access(&mut self) -> Option<SingleRegisterAccessOps<'_, (), Self>> {
        Some(self)
    }
}

impl SingleRegisterAccess<()> for Target {
    fn read_register(
        &mut self,
        _: (),
        id: RegisterId,
        bytes: &mut [u8],
    ) -> TargetResult<usize, Self> {
        Registers::new(&self.computer.processor)
            .register(id, bytes)
            .ok_or(TargetError::NonFatal)
    }
    fn write_register(&mut self, _: (), _: RegisterId, _: &[u8]) -> TargetResult<(), Self> {
        Err(TargetError::NonFatal)
    }
}

impl SingleThreadResume for Target {
    fn resume(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        self.resume_with(RunState::Continuing, signal)
    }
    fn support_single_step(&mut self) -> Option<SingleThreadSingleStepOps<'_, Self>> {
        Some(self)
    }
}

impl SingleThreadSingleStep for Target {
    fn step(&mut self, signal: Option<Signal>) -> Result<(), Self::Error> {
        self.resume_with(RunState::SingleStep, signal)
    }
}

impl Breakpoints for Target {
    fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
        Some(self)
    }
    fn support_hw_breakpoint(&mut self) -> Option<HwBreakpointOps<'_, Self>> {
        Some(self)
    }
}

impl SwBreakpoint for Target {
    fn add_sw_breakpoint(&mut self, address: u32, kind: usize) -> TargetResult<bool, Self> {
        self.insert_breakpoint(address, kind, BreakpointType::Software)
    }
    fn remove_sw_breakpoint(&mut self, address: u32, kind: usize) -> TargetResult<bool, Self> {
        self.remove_breakpoint(address, kind, BreakpointType::Software)
    }
}

impl HwBreakpoint for Target {
    fn add_hw_breakpoint(&mut self, address: u32, kind: usize) -> TargetResult<bool, Self> {
        self.insert_breakpoint(address, kind, BreakpointType::Hardware)
    }
    fn remove_hw_breakpoint(&mut self, address: u32, kind: usize) -> TargetResult<bool, Self> {
        self.remove_breakpoint(address, kind, BreakpointType::Hardware)
    }
}

impl MonitorCmd for Target {
    fn handle_monitor_cmd(
        &mut self,
        command: &[u8],
        output: ConsoleOutput<'_>,
    ) -> Result<(), Self::Error> {
        crate::debugger::present_monitor(self.monitor(command), output);
        Ok(())
    }
}

impl SessionTarget for Target {
    fn poll_input(
        &mut self,
        input: &mut crate::host::uart::input::UartInput,
    ) -> Result<(), EmulatorError> {
        input.poll(&mut self.computer.bus)
    }

    fn advance(&mut self) -> Result<Option<StopReason>, EmulatorError> {
        Self::advance(self)
    }

    fn drain_output(&mut self, bytes: &mut Vec<u8>) {
        Self::drain_output(self, bytes);
    }

    fn drain_output_each(
        &mut self,
        scratch: &mut Vec<u8>,
        consume: &mut impl FnMut(&str, &str, &[u8]) -> Result<(), EmulatorError>,
    ) -> Result<(), EmulatorError> {
        crate::output::drain(
            &mut self.computer.bus,
            &mut self.computer.host_interface,
            scratch,
            consume,
        )
    }

    fn halt(&mut self, reason: StopReason) -> StopReason {
        Self::halt(self, reason)
    }
}
