// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fmt::Write;

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

use super::{Architecture, Capability, CheriotSystem, Fault, RegisterId, Registers, StepOutcome};
use crate::debugger::{
    inspect_memory, BreakpointType, RunControl, RunState, SessionTarget, StopReason,
};
use crate::EmulatorError;

pub struct Target {
    control: RunControl,
    fault: Option<Fault>,
    pub reason: StopReason,
    pub state: RunState,
    system: CheriotSystem,
}

impl Target {
    pub fn new(mut system: CheriotSystem, maximum_steps: Option<u64>) -> Self {
        system.computer.pause_host_clock();
        Self {
            control: RunControl::new(maximum_steps),
            fault: None,
            reason: StopReason::Startup,
            state: RunState::Halted,
            system,
        }
    }

    pub fn advance(&mut self) -> Result<Option<StopReason>, EmulatorError> {
        if self.state == RunState::Halted {
            return Ok(None);
        }
        let address = self.system.computer.processor.program_counter;
        if let Some(reason) = self
            .control
            .before_step(address, self.system.computer.attempted_steps)
        {
            return Ok(Some(self.halt(reason)));
        }
        let processor = &self.system.computer.processor;
        let previous_pcc = processor.pcc;
        let exit = self.system.step()?;
        if let Some(StepOutcome::Trapped {
            address,
            cause,
            value,
        }) = self.system.last_outcome
        {
            self.fault = Some(Fault {
                address,
                cause,
                value,
                previous_pcc,
                resulting_pc: self.system.computer.processor.program_counter,
            });
            if let Some(reason) = StopReason::from_fault(address, cause) {
                return Ok(Some(self.halt(reason)));
            }
        }
        if let Some(code) = exit {
            return Ok(Some(self.halt(StopReason::Completed(code))));
        }
        if self.state == RunState::SingleStep {
            return Ok(Some(self.halt(StopReason::Step)));
        }
        Ok(None)
    }

    pub fn drain_output(&mut self, bytes: &mut Vec<u8>) {
        self.system.drain_output(bytes);
    }

    pub fn halt(&mut self, reason: StopReason) -> StopReason {
        self.system.computer.pause_host_clock();
        self.state = RunState::Halted;
        self.reason = reason;
        reason
    }

    pub fn inspect(&self, address: u32, data: &mut [u8]) -> Result<(), EmulatorError> {
        inspect_memory(&self.system.computer.bus, address, data)
    }

    pub fn into_system(mut self) -> CheriotSystem {
        self.system.computer.resume_host_clock();
        self.system
    }

    pub fn monitor(&self, command: &[u8]) -> Result<String, &'static str> {
        let words = crate::debugger::monitor_words(command)?;
        match words.as_slice() {
            ["help"] => Ok("help | capability c0..c15|pcc|mtcc|mtdc|mscratchc|mepcc | tags ADDRESS COUNT | fault | status\nTags are stored tags, in aligned eight-byte granules; no revocation filtering.\n".into()),
            ["capability", name] => {
                let capability = self.capability(name).ok_or("unknown capability register")?;
                let (payload, tag) = capability.to_memory();
                let (base, top) = capability.bounds();
                Ok(format!("{name}: payload=0x{payload:016x} tag={} cursor=0x{:08x} base=0x{base:08x} top=0x{top:09x} permissions=0x{:03x} sealed={} type={}\n", u8::from(tag), capability.address(), capability.permissions(), capability.sealed(), capability.object_type()))
            }
            ["fault"] => Ok(match self.fault {
                Some(fault) => format!("fault address=0x{:08x} cause=0x{:08x} value=0x{:08x} pre_pcc=0x{:016x}/{} resulting_pc=0x{:08x}\n", fault.address, fault.cause, fault.value, fault.previous_pcc.to_memory().0, u8::from(fault.previous_pcc.to_memory().1), fault.resulting_pc),
                None => "no trap observed\n".into(),
            }),
            ["status"] => Ok(format!("{:?}; steps={} retired={} virtual_time={}\n", self.reason, self.system.computer.attempted_steps, self.system.computer.processor.machine_registers.retired, self.system.computer.processor.machine_registers.time)),
            ["tags", address, count] => {
                let number = |s: &str| u32::from_str_radix(s.strip_prefix("0x").unwrap_or(s), if s.starts_with("0x") {16} else {10}).map_err(|_| "invalid number");
                let address = number(address)?;
                let count = number(count)?;
                if !address.is_multiple_of(8) || !(1..=64).contains(&count) || u64::from(address) + u64::from(count) * 8 > 1_u64 << 32 {
                    return Err("tags requires eight-byte alignment and 1..64 granules within address space");
                }
                let mut output = String::from("stored tags (without revocation filtering):\n");
                for index in 0..count {
                    let location = address + index * 8;
                    let memory = self.system.computer.bus.memory().iter().find(|region| region.contains(location, 8)).ok_or("tag query outside memory")?;
                    let tag = memory.tag_at(location).ok_or("memory is not tagged")?;
                    let _ = writeln!(output, "0x{location:08x}: {}", u8::from(tag));
                }
                Ok(output)
            }
            _ => Err("unknown monitor command; use monitor help"),
        }
    }

    pub fn system(&self) -> &CheriotSystem {
        &self.system
    }

    fn capability(&self, name: &str) -> Option<Capability> {
        let processor = &self.system.computer.processor;
        Some(match name {
            "pcc" => processor.pcc,
            "mtcc" => processor.machine_trap,
            "mtdc" => processor.machine_data,
            "mscratchc" => processor.machine_scratch,
            "mepcc" => processor.machine_exception,
            _ => *processor
                .registers()
                .get(name.strip_prefix('c')?.parse::<usize>().ok()?)?,
        })
    }

    fn insert_breakpoint(
        &mut self,
        address: u32,
        kind: usize,
        breakpoint: BreakpointType,
    ) -> TargetResult<bool, Self> {
        if self
            .control
            .insert_breakpoint(&self.system.computer.bus, breakpoint, address, kind, 2)
        {
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
        if self.control.remove_breakpoint(breakpoint, address, kind, 2) {
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
        self.system.computer.resume_host_clock();
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
        *registers = Registers::new(&self.system.computer.processor);
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
        Registers::new(&self.system.computer.processor)
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
        input.poll(&mut self.system.computer.bus)
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
            &mut self.system.computer.bus,
            &mut self.system.host_interface,
            scratch,
            consume,
        )
    }

    fn halt(&mut self, reason: StopReason) -> StopReason {
        Self::halt(self, reason)
    }
}
