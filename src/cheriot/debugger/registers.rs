// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use gdbstub::arch::Registers as GdbRegisters;

use super::{Processor, RegisterId};

#[derive(Clone, Debug, PartialEq)]
pub struct Registers {
    bytes: [Option<u8>; 321],
    pc: u32,
}

impl Registers {
    pub fn new(processor: &Processor) -> Self {
        let mut snapshot = Self {
            pc: processor.program_counter,
            ..Self::default()
        };
        for (index, value) in processor.registers().iter().enumerate() {
            snapshot.put(index * 4, &value.address().to_le_bytes());
        }
        snapshot.put(128, &processor.program_counter.to_le_bytes());
        let special = [
            processor.pcc,
            processor.machine_trap,
            processor.machine_data,
            processor.machine_scratch,
            processor.machine_exception,
        ];
        for (index, capability) in processor
            .registers()
            .iter()
            .chain(special.iter())
            .enumerate()
        {
            let (bits, tag) = capability.to_memory();
            snapshot.put(132 + index * 8, &bits.to_le_bytes());
            snapshot.put(300 + index, &[u8::from(tag)]);
        }
        snapshot
    }

    pub fn register(&self, id: RegisterId, output: &mut [u8]) -> Option<usize> {
        crate::debugger::read_register(&self.bytes[id.byte_range()], output)
    }

    fn put(&mut self, offset: usize, bytes: &[u8]) {
        for (destination, byte) in self.bytes[offset..].iter_mut().zip(bytes) {
            *destination = Some(*byte);
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            bytes: [None; 321],
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
