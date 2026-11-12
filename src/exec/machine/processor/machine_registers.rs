// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::error::EmulatorError;

#[derive(Default, Clone, Debug)]
pub struct MachineRegisters {
    pub counter_inhibit: u32,
    pub cycle: u64,
    pub exception_program_counter: u32,
    pub instruction_alignment: u32,
    pub interrupt_enable: u32,
    pub interrupt_pending: u32,
    pub isa: u32,
    pub machine_status: u32,
    pub retired: u64,
    pub scratch: u32,
    pub time: u64,
    pub trap_cause: u32,
    pub trap_value: u32,
    pub trap_vector: u32,
}

impl MachineRegisters {
    pub fn enter_trap(&mut self, address: u32, cause: u32, value: u32, interrupt: bool) -> u32 {
        self.exception_program_counter = address;
        self.trap_cause = cause | if interrupt { 1 << 31 } else { 0 };
        self.trap_value = value;
        let enabled = self.machine_status & (1 << 3) != 0;

        if enabled {
            self.machine_status |= 1 << 7;
        } else {
            self.machine_status &= !(1 << 7);
        }

        self.machine_status &= !(1 << 3);
        self.machine_status = (self.machine_status & !(3 << 11)) | (3 << 11);
        let base = self.trap_vector & !3;

        if interrupt && self.trap_vector & 3 == 1 {
            base.wrapping_add(4u32.wrapping_mul(cause & 0x7fff_ffff))
        } else {
            base
        }
    }

    pub fn machine_return(&mut self) -> u32 {
        if self.machine_status & (1 << 7) != 0 {
            self.machine_status |= 1 << 3;
        } else {
            self.machine_status &= !(1 << 3);
        }

        self.machine_status |= 1 << 7;
        self.machine_status |= 3 << 11;
        self.exception_program_counter & !(self.instruction_alignment.max(2) - 1)
    }

    pub fn read(&self, address: u16) -> Option<u32> {
        match address {
            0x300 => Some(self.machine_status),
            0x301 => Some(self.isa),
            0x310 => Some(0),
            0x320 => Some(self.counter_inhibit),
            0x304 => Some(self.interrupt_enable),
            0x305 => Some(self.trap_vector),
            0x340 => Some(self.scratch),
            0x341 => {
                Some(self.exception_program_counter & !(self.instruction_alignment.max(2) - 1))
            }
            0x342 => Some(self.trap_cause),
            0x343 => Some(self.trap_value),
            0x344 => Some(self.interrupt_pending),
            0xb00 | 0xc00 => Some(self.cycle as u32),
            0xb80 | 0xc80 => Some((self.cycle >> 32) as u32),
            0xc01 => Some(self.time as u32),
            0xc81 => Some((self.time >> 32) as u32),
            0xb02 | 0xc02 => Some(self.retired as u32),
            0xb82 | 0xc82 => Some((self.retired >> 32) as u32),
            0xf11..=0xf15 => Some(0),
            _ => None,
        }
    }

    pub fn write(&mut self, address: u16, value: u32) -> Result<(), EmulatorError> {
        let error = || {
            EmulatorError::Configuration(format!(
                "CSR 0x{address:03x} is not writable or implemented"
            ))
        };

        match address {
            0x300 => {
                self.machine_status = (self.machine_status & !(1 << 3 | 1 << 7))
                    | (value & (1 << 3 | 1 << 7))
                    | (3 << 11)
            }
            0x304 => self.interrupt_enable = value & ((1 << 3) | (1 << 7) | (1 << 11)),

            0x305 => self.trap_vector = (value & !3) | u32::from(value & 3 == 1),
            0x301 | 0x310 => {}
            0x320 => self.counter_inhibit = value & 5,
            0x340 => self.scratch = value,
            0x341 => self.exception_program_counter = value & !1,
            0x342 => self.trap_cause = value,
            0x343 => self.trap_value = value,
            0x344 => {}
            0xb00 => self.cycle = (self.cycle & !0xffff_ffff) | u64::from(value),
            0xb80 => self.cycle = (self.cycle & 0xffff_ffff) | (u64::from(value) << 32),
            0xb02 => self.retired = (self.retired & !0xffff_ffff) | u64::from(value),
            0xb82 => self.retired = (self.retired & 0xffff_ffff) | (u64::from(value) << 32),
            _ => return Err(error()),
        }

        Ok(())
    }
}
