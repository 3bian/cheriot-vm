// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

use crate::bus::Bus;
use crate::cheriot1::Cheriot1System;
use crate::devices::ethernet::Ethernet;
use crate::devices::gpio::Gpio;
use crate::devices::i2c::I2c;
use crate::devices::interrupt_controller::InterruptController;
use crate::devices::spi::Spi;
use crate::devices::storage::Storage;
use crate::devices::timer32::Timer32;
use crate::devices::timer64::Timer64;
use crate::devices::uart::{Uart, UartLayout};
use crate::devices::Device;
use crate::execution::TrapPolicy;
use crate::processor::{MachineRegisters, ProcessorProfile};
use crate::rv32::computer::Computer;
use crate::TimingPolicy;

pub const CHECKPOINT_MAGIC: &[u8; 8] = b"CHKP0002";
pub const CHECKPOINT_OMITTED_STATE: u32 = 0x1f;

impl Computer {
    pub fn write_checkpoint(&self, output: &mut impl Write) -> io::Result<()> {
        write_header(
            output,
            false,
            self.processor.profile,
            self.timing,
            u8::from(self.trap_policy == TrapPolicy::Vector),
        )?;
        write_completion_configuration(
            output,
            self.host_interface
                .as_ref()
                .map(|interface| interface.address()),
        )?;
        write_machine(
            output,
            &self.processor.machine_registers,
            self.processor.program_counter,
            self.attempted_steps,
            self.fetched_instructions,
        )?;
        for register in self.processor.registers() {
            output.write_all(&u64::from(*register).to_le_bytes())?;
        }
        write_bus(output, &self.bus)
    }
}

pub(crate) fn write_cheriot1(system: &Cheriot1System, output: &mut impl Write) -> io::Result<()> {
    let computer = &system.computer;
    let processor = &computer.processor;
    write_header(
        output,
        true,
        ProcessorProfile {
            compressed: true,
            control_registers: true,
            embedded: true,
            multiply_divide: true,
        },
        computer.timing,
        2,
    )?;
    write_completion_configuration(
        output,
        system
            .host_interface
            .as_ref()
            .map(|interface| interface.address()),
    )?;
    write_machine(
        output,
        &processor.machine_registers,
        processor.program_counter,
        computer.attempted_steps,
        computer.fetched_instructions,
    )?;
    for word in [
        processor.stack_base,
        processor.stack_high_water,
        processor.trigger_select,
        computer.revocation_base,
        computer.revocation_memory_base,
    ] {
        output.write_all(&u64::from(word).to_le_bytes())?;
    }
    let special = [
        processor.pcc,
        processor.machine_trap,
        processor.machine_data,
        processor.machine_scratch,
        processor.machine_exception,
    ];
    for capability in processor.registers().iter().chain(special.iter()) {
        let (bits, tag) = capability.to_memory();
        output.write_all(&bits.to_le_bytes())?;
        output.write_all(&u64::from(tag).to_le_bytes())?;
    }
    write_bus(output, &computer.bus)
}

fn device_kind(device: &dyn Device) -> &'static str {
    let device = device.as_any();
    if device.is::<Ethernet>() {
        "ethernet"
    } else if device.is::<Gpio>() {
        "gpio"
    } else if device.is::<I2c>() {
        "i2c"
    } else if device.is::<InterruptController>() {
        "interrupt-controller"
    } else if device.is::<Spi>() {
        "spi"
    } else if device.is::<Storage>() {
        "storage"
    } else if device.is::<Timer32>() {
        "timer32"
    } else if device.is::<Timer64>() {
        "timer64"
    } else if let Some(uart) = device.downcast_ref::<Uart>() {
        match uart.layout() {
            UartLayout::Ns16550 => "uart-16550",
            UartLayout::Simple => "uart",
        }
    } else {
        "external"
    }
}

fn write_bus(output: &mut impl Write, bus: &Bus) -> io::Result<()> {
    output.write_all(&(bus.devices().len() as u32).to_le_bytes())?;
    for mapping in bus.devices() {
        write_string(output, &mapping.name)?;
        write_string(output, device_kind(mapping.device.as_ref()))?;
        output.write_all(&mapping.base.to_le_bytes())?;
        output.write_all(&mapping.size.to_le_bytes())?;
        output.write_all(
            &mapping
                .interrupt_source
                .map_or(256, u16::from)
                .to_le_bytes(),
        )?;
    }
    output.write_all(&(bus.memory().len() as u32).to_le_bytes())?;
    for memory in bus.memory() {
        memory.write_checkpoint(output)?;
    }
    Ok(())
}

fn write_completion_configuration(output: &mut impl Write, address: Option<u32>) -> io::Result<()> {
    output.write_all(&[u8::from(address.is_some())])?;
    output.write_all(&address.unwrap_or(0).to_le_bytes())
}

fn write_header(
    output: &mut impl Write,
    capability: bool,
    profile: ProcessorProfile,
    timing: TimingPolicy,
    trap_policy: u8,
) -> io::Result<()> {
    let profile_id = if capability {
        8
    } else {
        u8::from(!profile.embedded) * 4
            + u8::from(profile.multiply_divide)
            + u8::from(profile.compressed) * 2
    };
    let flags = u8::from(profile.embedded)
        | (u8::from(profile.multiply_divide) << 1)
        | (u8::from(profile.compressed) << 2)
        | (u8::from(profile.control_registers) << 3);
    output.write_all(CHECKPOINT_MAGIC)?;
    output.write_all(&[
        u8::from(capability),
        profile_id,
        flags,
        u8::from(timing == TimingPolicy::Cheriot1Compatible),
        profile.register_count() as u8,
        if capability { 16 } else { 8 },
        trap_policy,
        0,
    ])?;
    output.write_all(&CHECKPOINT_OMITTED_STATE.to_le_bytes())
}

fn write_machine(
    output: &mut impl Write,
    machine: &MachineRegisters,
    program_counter: u32,
    attempted_steps: u64,
    fetched_instructions: u64,
) -> io::Result<()> {
    let words = [
        attempted_steps,
        fetched_instructions,
        u64::from(program_counter),
        machine.cycle,
        machine.time,
        machine.retired,
        u64::from(machine.machine_status),
        u64::from(machine.interrupt_enable),
        u64::from(machine.interrupt_pending),
        u64::from(machine.counter_inhibit),
        u64::from(machine.exception_program_counter),
        u64::from(machine.trap_vector),
        u64::from(machine.trap_value),
        u64::from(machine.trap_cause),
        u64::from(machine.scratch),
        u64::from(machine.isa),
        u64::from(machine.instruction_alignment),
    ];
    for word in words {
        output.write_all(&word.to_le_bytes())?;
    }
    Ok(())
}

fn write_string(output: &mut impl Write, value: &str) -> io::Result<()> {
    let size = u32::try_from(value.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "checkpoint name too long"))?;
    output.write_all(&size.to_le_bytes())?;
    output.write_all(value.as_bytes())
}
