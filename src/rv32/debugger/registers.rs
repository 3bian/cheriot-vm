// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use gdbstub::arch::Registers as GdbRegisters;

use super::register_id::CSR_ADDRESSES;
use super::RegisterId;
use crate::rv32::Processor;

#[derive(Clone, Debug, PartialEq)]
pub struct Registers {
    bytes: [Option<u8>; 236],
    pc: u32,
}

impl Registers {
    pub fn new(processor: &Processor) -> Self {
        let mut snapshot = Self {
            pc: processor.program_counter,
            ..Self::default()
        };
        for (index, value) in processor.registers().iter().enumerate() {
            snapshot.put(index * 4, *value);
        }
        snapshot.put(128, processor.program_counter);
        if processor.profile.control_registers {
            for (index, address) in CSR_ADDRESSES.iter().enumerate() {
                if let Some(value) = processor.machine_registers.read(*address) {
                    snapshot.put(132 + index * 4, value);
                }
            }
        }
        snapshot
    }

    pub fn register(&self, id: RegisterId, output: &mut [u8]) -> Option<usize> {
        crate::debugger::read_register(&self.bytes[id.byte_range()], output)
    }

    fn put(&mut self, offset: usize, value: u32) {
        for (destination, byte) in self.bytes[offset..].iter_mut().zip(value.to_le_bytes()) {
            *destination = Some(byte);
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            bytes: [None; 236],
            pc: 0,
        }
    }
}

impl GdbRegisters for Registers {
    type ProgramCounter = u32;

    fn pc(&self) -> u32 {
        self.pc
    }

    fn gdb_serialize(&self, mut write_byte: impl FnMut(Option<u8>)) {
        for byte in self.bytes {
            write_byte(byte);
        }
    }

    fn gdb_deserialize(&mut self, _: &[u8]) -> Result<(), ()> {
        Err(())
    }
}
