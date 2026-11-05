// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[cfg(feature = "verification")]
use std::sync::Mutex;

use super::Fault;
#[cfg(feature = "verification")]
use super::RegisterAccess;
use crate::cheriot1::Capability;
use crate::processor::MachineRegisters;

pub struct Processor {
    pub machine_data: Capability,
    pub machine_exception: Capability,
    pub machine_registers: MachineRegisters,
    pub machine_scratch: Capability,
    pub machine_trap: Capability,
    pub pcc: Capability,
    pub program_counter: u32,
    #[cfg(feature = "verification")]
    register_accesses: Mutex<(bool, Vec<RegisterAccess>)>,
    registers: [Capability; 16],
    pub stack_base: u32,
    pub stack_high_water: u32,
    pub trigger_select: u32,
}

impl Processor {
    pub fn new(entry: u32) -> Self {
        let execute = Capability::from_memory(0x5e3e_0000_0000_0000, true);
        Self {
            machine_registers: MachineRegisters {
                machine_status: 3 << 11,
                instruction_alignment: 2,
                isa: (1 << 30) | (1 << 23) | (1 << 12) | (1 << 4) | (1 << 2),
                ..Default::default()
            },
            machine_data: Capability::from_memory(0x7e3e_0000_0000_0000, true),
            machine_exception: execute,
            machine_scratch: Capability::from_memory(0x4e3e_0000_0000_0000, true),
            machine_trap: execute,
            pcc: execute,
            program_counter: entry,
            #[cfg(feature = "verification")]
            register_accesses: Mutex::new((false, Vec::with_capacity(8))),
            registers: [Capability::integer(0); 16],
            stack_base: 0,
            stack_high_water: 0,
            trigger_select: 0,
        }
    }

    pub fn read_register(&self, index: u8) -> Result<Capability, Fault> {
        let value = self
            .registers
            .get(usize::from(index))
            .copied()
            .ok_or_else(Fault::illegal)?;
        #[cfg(feature = "verification")]
        self.observe(RegisterAccess::Read { index, value });
        Ok(value)
    }

    #[cfg(feature = "verification")]
    pub fn register_accesses(&self) -> Vec<RegisterAccess> {
        self.register_accesses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .1
            .clone()
    }

    pub fn registers(&self) -> &[Capability; 16] {
        &self.registers
    }

    pub fn write_register(&mut self, index: u8, value: Capability) -> Result<(), Fault> {
        let register = self
            .registers
            .get_mut(usize::from(index))
            .ok_or_else(Fault::illegal)?;

        if index != 0 {
            *register = value;
            #[cfg(feature = "verification")]
            self.observe(RegisterAccess::Write { index, value });
        }

        Ok(())
    }

    pub fn write_integer_register(&mut self, index: u8, value: u32) -> Result<(), Fault> {
        self.write_register(index, Capability::integer(value))
    }

    #[cfg(feature = "verification")]
    pub(crate) fn begin_observation(&mut self) {
        let observation = self
            .register_accesses
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        observation.0 = true;
        observation.1.clear();
    }

    #[cfg(feature = "verification")]
    pub(crate) fn end_observation(&mut self) {
        self.register_accesses
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .0 = false;
    }

    #[cfg(feature = "verification")]
    fn observe(&self, access: RegisterAccess) {
        let mut observation = self
            .register_accesses
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if observation.0 {
            observation.1.push(access);
        }
    }
}
