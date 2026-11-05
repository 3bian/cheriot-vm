// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::processor::{MachineRegisters, ProcessorProfile};
use crate::EmulatorError;

pub struct Processor {
    pub machine_registers: MachineRegisters,
    pub profile: ProcessorProfile,
    pub program_counter: u32,
    registers: [u32; 32],
}

impl Processor {
    pub fn new(profile: ProcessorProfile, entry: u32) -> Self {
        let machine_registers = MachineRegisters {
            machine_status: 3 << 11,
            instruction_alignment: profile.minimum_alignment(),
            isa: (1 << 30)
                | (if profile.embedded { 1 << 4 } else { 1 << 8 })
                | (u32::from(profile.multiply_divide) << 12)
                | (u32::from(profile.compressed) << 2),
            ..MachineRegisters::default()
        };
        Self {
            registers: [0; 32],
            program_counter: entry,
            profile,
            machine_registers,
        }
    }

    pub fn read_register(&self, index: u8) -> Result<u32, EmulatorError> {
        self.registers()
            .get(usize::from(index))
            .copied()
            .ok_or_else(|| self.register_error(index))
    }

    pub fn registers(&self) -> &[u32] {
        &self.registers[..self.profile.register_count()]
    }

    pub fn validate_register(&self, index: u8) -> bool {
        (index as usize) < self.profile.register_count()
    }

    pub fn write_register(&mut self, index: u8, value: u32) -> Result<(), EmulatorError> {
        if !self.validate_register(index) {
            return Err(self.register_error(index));
        }

        self.write_register_unchecked(index, value);
        Ok(())
    }

    pub(crate) fn read_register_unchecked(&self, index: u8) -> u32 {
        if index == 0 {
            0
        } else {
            self.registers[index as usize]
        }
    }

    fn register_error(&self, index: u8) -> EmulatorError {
        EmulatorError::Configuration(format!(
            "register x{index} is unavailable on this {}-register processor",
            self.profile.register_count()
        ))
    }

    pub(crate) fn write_register_unchecked(&mut self, index: u8, value: u32) {
        if index != 0 {
            self.registers[index as usize] = value;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/rv32/processor.rs"]
mod tests;
