// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[cfg(feature = "verification")]
use super::MemoryAccess;
use super::{
    decode_compressed, decode_standard, DecodedInstruction, Fault, Operation, Processor,
    StepOutcome,
};
use crate::bus::Bus;
use crate::cheriot1::{permissions, Capability, CapabilityFaultCause};
use crate::devices::InterruptLine;
use crate::instructions::arithmetic::{perform_immediate_operation, perform_register_operation};
use crate::instructions::{CsrOperation, Instruction};
use crate::timing_policy::TimingPolicy;

pub struct Computer {
    pub attempted_steps: u64,
    pub bus: Bus,
    pub fetched_instructions: u64,
    pub last_store: Option<(u32, usize)>,
    #[cfg(feature = "verification")]
    memory_accesses: Vec<MemoryAccess>,
    pub processor: Processor,
    pub revocation_base: u32,
    pub revocation_memory_base: u32,
    pub timing: TimingPolicy,
}

impl Computer {
    pub fn new(bus: Bus, entry: u32) -> Self {
        Self {
            bus,
            fetched_instructions: 0,
            last_store: None,
            #[cfg(feature = "verification")]
            memory_accesses: Vec::with_capacity(2),
            processor: Processor::new(entry),
            revocation_base: 0x8300_0000,
            revocation_memory_base: 0x8000_0000,
            attempted_steps: 0,
            timing: TimingPolicy::Cheriot1Compatible,
        }
    }

    pub fn step(&mut self) -> StepOutcome {
        self.step_inner(None)
    }

    #[cfg(feature = "verification")]
    pub fn memory_accesses(&self) -> &[MemoryAccess] {
        &self.memory_accesses
    }

    #[cfg(feature = "verification")]
    pub fn step_injected(&mut self, encoded: u32) -> StepOutcome {
        self.step_inner(Some(encoded))
    }

    fn step_inner(&mut self, injected: Option<u32>) -> StepOutcome {
        #[cfg(feature = "verification")]
        {
            self.processor.begin_observation();
            self.memory_accesses.clear();
        }
        let address = self.processor.program_counter;
        let inhibit = self.processor.machine_registers.counter_inhibit;
        let increment_retired = inhibit & 4 == 0;
        let mut fetched = false;

        self.last_store = None;
        self.bus.refresh_interrupts();
        let pending = (self.processor.machine_registers.interrupt_pending & 0x222)
            | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineExternal)) << 11)
            | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineSoftware)) << 3)
            | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineTimer)) << 7);
        self.processor.machine_registers.interrupt_pending = pending;
        let enabled = pending & self.processor.machine_registers.interrupt_enable;

        let result = if self.processor.machine_registers.machine_status & 8 != 0 && enabled != 0 {
            Err(Fault {
                cause: 0x8000_0000
                    | if enabled & (1 << 11) != 0 {
                        11
                    } else if enabled & 8 != 0 {
                        3
                    } else if enabled & (1 << 7) != 0 {
                        7
                    } else if enabled & (1 << 9) != 0 {
                        9
                    } else if enabled & (1 << 1) != 0 {
                        1
                    } else {
                        5
                    },
                value: 0,
            })
        } else {
            self.fetch(&mut fetched, injected)
                .and_then(|decoded| self.execute(decoded).map(|()| decoded))
        };

        let writes_cycle = match &result {
            Ok(decoded) => matches!(decoded.operation,
                Operation::Integer(Instruction::Csr {
                    address: 0xb00 | 0xb80, operation, source, ..
                }) if operation.writes(source)),
            Err(_) => false,
        };

        self.attempted_steps = self.attempted_steps.wrapping_add(1);
        let outcome = match result {
            Ok(decoded) => {
                let writes_retired = matches!(decoded.operation,
                    Operation::Integer(Instruction::Csr {
                        address: 0xb02 | 0xb82, operation, source, ..
                    }) if operation.writes(source));

                if increment_retired && !writes_retired {
                    self.processor.machine_registers.retired =
                        self.processor.machine_registers.retired.wrapping_add(1);
                }

                StepOutcome::Retired {
                    address,
                    encoded: decoded.encoded,
                }
            }
            Err(fault) => {
                self.processor.machine_exception = self.processor.pcc.set_address(address);
                self.processor.machine_registers.enter_trap(
                    address,
                    fault.cause & 0x7fff_ffff,
                    fault.value,
                    fault.cause >> 31 != 0,
                );
                self.processor.pcc = self.processor.machine_trap;
                self.processor.program_counter = self.processor.machine_trap.address();
                StepOutcome::Trapped {
                    address,
                    cause: fault.cause,
                    value: fault.value,
                }
            }
        };
        self.fetched_instructions = self.fetched_instructions.wrapping_add(u64::from(fetched));

        self.timing.advance(
            &mut self.bus,
            &mut self.processor.machine_registers,
            fetched,
            self.fetched_instructions,
            inhibit,
            writes_cycle,
        );

        self.processor.machine_registers.interrupt_pending =
            (self.processor.machine_registers.interrupt_pending & 0x222)
                | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineExternal)) << 11)
                | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineSoftware)) << 3)
                | (u32::from(self.bus.interrupt_pending(InterruptLine::MachineTimer)) << 7);

        #[cfg(feature = "verification")]
        self.processor.end_observation();
        outcome
    }

    fn check_csr_access(address: u16, writing: bool) -> Result<(), Fault> {
        let defined = matches!(
            address,
            0x300 | 0x301 | 0x304 | 0x305 | 0x310 | 0x320 | 0x340..=0x344
                | 0x7a0 | 0xb00 | 0xb02 | 0xb80 | 0xb82 | 0xbc0..=0xbc2 | 0xf11..=0xf14
        );
        if !defined || (writing && address & 0xc00 == 0xc00) {
            return Err(Fault::illegal());
        }
        Ok(())
    }

    fn check_fetch_bounds(&mut self, address: u32, (base, top): (u32, u64)) -> Result<(), Fault> {
        if address < base || u64::from(address) + 2 > top {
            self.processor.pcc = self.processor.pcc.clear_tag();
            return Err(Fault::capability_cause(CapabilityFaultCause::Bounds, 32));
        }
        Ok(())
    }

    fn check_memory_access(
        &self,
        register: u8,
        address: u32,
        width: usize,
        writing: bool,
        stores_valid_capability: bool,
    ) -> Result<u16, Fault> {
        let authority = self.processor.read_register(register)?;
        let permission = if writing {
            permissions::STORE
        } else {
            permissions::LOAD
        };
        let cause = if writing {
            CapabilityFaultCause::StorePermission
        } else {
            CapabilityFaultCause::LoadPermission
        };

        if !authority.tag() {
            return Err(Fault::capability_cause(CapabilityFaultCause::Tag, register));
        }

        if authority.sealed() {
            return Err(Fault::capability_cause(
                CapabilityFaultCause::Seal,
                register,
            ));
        }

        let authority_permissions = authority.permissions();

        if authority_permissions & permission == 0 {
            return Err(Fault::capability_cause(cause, register));
        }

        if writing
            && stores_valid_capability
            && authority_permissions & permissions::LOAD_STORE_CAPABILITY == 0
        {
            return Err(Fault::capability_cause(
                CapabilityFaultCause::StoreCapabilityPermission,
                register,
            ));
        }

        if !authority.contains(address, width as u64) {
            return Err(Fault::capability_cause(
                CapabilityFaultCause::Bounds,
                register,
            ));
        }

        if width == 8 && !address.is_multiple_of(8) {
            return Err(Fault {
                cause: if writing { 6 } else { 4 },
                value: address,
            });
        }

        Ok(authority_permissions)
    }

    fn execute(&mut self, decoded: DecodedInstruction) -> Result<(), Fault> {
        let next = self.processor.program_counter.wrapping_add(decoded.length);

        if let Operation::Integer(instruction) = decoded.operation {
            return self.execute_integer(instruction, next);
        }

        let first = self.processor.read_register(decoded.first)?;
        let second = if decoded.operation == Operation::SpecialReadWrite {
            Capability::integer(0)
        } else {
            self.processor.read_register(decoded.second)?
        };
        let value = second.address();
        let immediate = decoded.immediate as u32;
        let destination = decoded.destination;
        let capability = match decoded.operation {
            Operation::AddressFromGlobal => {
                first.set_address(first.address().wrapping_add(immediate))
            }
            Operation::AddressFromPc => self
                .processor
                .pcc
                .set_address(self.processor.program_counter.wrapping_add(immediate)),
            Operation::AndPermissions => first.restrict_permissions(value as u16),
            Operation::ClearTag => first.clear_tag(),
            Operation::GetAddress => Capability::integer(first.address()),
            Operation::GetBase => Capability::integer(first.bounds().0),
            Operation::GetHigh => Capability::integer((first.to_memory().0 >> 32) as u32),
            Operation::GetLength => Capability::integer(first.length()),
            Operation::GetPermissions => Capability::integer(u32::from(first.permissions())),
            Operation::GetTag => Capability::integer(u32::from(first.tag())),
            Operation::GetTop => {
                Capability::integer(first.bounds().1.min(u64::from(u32::MAX)) as u32)
            }
            Operation::GetType => Capability::integer(u32::from(first.object_type())),
            Operation::IncrementAddress => first.set_address(first.address().wrapping_add(value)),
            Operation::IncrementAddressImmediate => {
                first.set_address(first.address().wrapping_add(immediate))
            }
            Operation::Integer(_) => return Err(Fault::illegal()),
            Operation::Jump => {
                self.link(destination, next)?;
                self.processor.program_counter =
                    self.processor.program_counter.wrapping_add(immediate);
                return Ok(());
            }
            Operation::JumpRegister => {
                let object_type = first.object_type();

                if !first.tag() {
                    return Err(Fault::capability_cause(
                        CapabilityFaultCause::Tag,
                        decoded.first,
                    ));
                }

                let legal =
                    (destination == 0 && decoded.first == 1 && matches!(object_type, 4 | 5))
                        || (destination == 0 && decoded.first != 1 && matches!(object_type, 0 | 1))
                        || (destination != 0 && matches!(object_type, 0 | 1))
                        || (destination == 1 && matches!(object_type, 0..=3));

                if !legal || (object_type != 0 && immediate != 0) {
                    return Err(Fault::capability_cause(
                        CapabilityFaultCause::Seal,
                        decoded.first,
                    ));
                }

                if first.permissions() & permissions::EXECUTE == 0 {
                    return Err(Fault::capability_cause(
                        CapabilityFaultCause::ExecutePermission,
                        decoded.first,
                    ));
                }

                self.link(destination, next)?;
                self.processor.pcc =
                    Capability::from_memory(first.to_memory().0 & !(7u64 << 54), first.tag());

                if matches!(object_type, 2 | 4) {
                    self.processor.machine_registers.machine_status &= !8;
                }

                if matches!(object_type, 3 | 5) {
                    self.processor.machine_registers.machine_status |= 8;
                }

                self.processor.program_counter = first.address().wrapping_add(immediate) & !1;
                return Ok(());
            }
            Operation::Load => {
                let address = first.address().wrapping_add(immediate);
                let authority_permissions =
                    self.check_memory_access(decoded.first, address, 8, false, false)?;
                let mut loaded = self.bus.read_capability(address).map_err(|_| Fault {
                    cause: 5,
                    value: address,
                })?;
                #[cfg(feature = "verification")]
                self.memory_accesses.push(MemoryAccess::Read {
                    address,
                    width: 8,
                    value: loaded.to_memory().0,
                    tag: Some(loaded.tag()),
                });

                if authority_permissions & permissions::LOAD_STORE_CAPABILITY == 0 {
                    loaded = loaded.clear_tag();
                }

                if loaded.tag() {
                    let mut mask = 0xfff;
                    let sealed = loaded.sealed();

                    if authority_permissions & permissions::LOAD_GLOBAL == 0 {
                        mask &= !permissions::GLOBAL;

                        if !sealed {
                            mask &= !permissions::LOAD_GLOBAL;
                        }
                    }

                    if !sealed && authority_permissions & permissions::LOAD_MUTABLE == 0 {
                        mask &= !(permissions::STORE | permissions::LOAD_MUTABLE);
                    }

                    loaded = loaded.restrict_permissions(mask);
                    let sealing = permissions::SEAL | permissions::UNSEAL | permissions::USER0;
                    let base = loaded.bounds().0;

                    if loaded.permissions() & sealing == 0 && base >= self.revocation_memory_base {
                        let bitmap_address = self
                            .revocation_base
                            .checked_add((base - self.revocation_memory_base) >> 6)
                            .ok_or(Fault {
                                cause: 5,
                                value: address,
                            })?;
                        let byte = self.bus.inspect(bitmap_address, 1).map_err(|_| Fault {
                            cause: 5,
                            value: address,
                        })?;
                        #[cfg(feature = "verification")]
                        self.memory_accesses.push(MemoryAccess::RevocationRead {
                            address: bitmap_address,
                            value: byte as u8,
                        });
                        loaded = loaded.filter_revoked(byte & (1 << ((base >> 3) & 7)) != 0);
                    }
                }

                loaded
            }
            Operation::Move => first,
            Operation::RepresentableAlignment => {
                Capability::integer(Capability::alignment_mask(first.address()))
            }
            Operation::RepresentableLength => {
                Capability::integer(Capability::rounded_length(first.address()))
            }
            Operation::Seal => first.seal(second),
            Operation::SetAddress => first.set_address(value),
            Operation::SetBounds => first.set_bounds(value, false).0,
            Operation::SetBoundsExact => first.set_bounds(value, true).0,
            Operation::SetBoundsImmediate => first.set_bounds(immediate, false).0,
            Operation::SetBoundsRoundDown => first.set_bounds_round_down(value),
            Operation::SetHigh => Capability::from_memory(
                (u64::from(value) << 32) | u64::from(first.address()),
                false,
            ),
            Operation::SpecialReadWrite => self.special(decoded.second, decoded.first, first)?,
            Operation::Store => {
                let address = first.address().wrapping_add(immediate);
                let authority_permissions =
                    self.check_memory_access(decoded.first, address, 8, true, second.tag())?;
                let stored = if authority_permissions & permissions::STORE_LOCAL == 0
                    && second.permissions() & permissions::GLOBAL == 0
                {
                    second.clear_tag()
                } else {
                    second
                };
                self.bus
                    .write_capability(address, stored)
                    .map_err(|_| Fault {
                        cause: 7,
                        value: address,
                    })?;
                self.record_store(address, 8);
                #[cfg(feature = "verification")]
                self.memory_accesses.push(MemoryAccess::Write {
                    address,
                    width: 8,
                    value: stored.to_memory().0,
                    tag: Some(stored.tag()),
                });
                self.processor.program_counter = next;
                return Ok(());
            }
            Operation::Subtract => Capability::integer(first.address().wrapping_sub(value)),
            Operation::TestEqual => Capability::integer(u32::from(first == second)),
            Operation::TestSubset => Capability::integer(u32::from(first.subset_contains(second))),
            Operation::Unseal => first.unseal(second),
        };
        self.processor.write_register(destination, capability)?;
        self.processor.program_counter = next;

        Ok(())
    }

    fn execute_integer(&mut self, instruction: Instruction, next: u32) -> Result<(), Fault> {
        let mut following = next;
        match instruction {
            Instruction::Register {
                operation,
                destination,
                first,
                second,
            } => {
                let value = perform_register_operation(
                    operation,
                    self.processor.read_register(first)?.address(),
                    self.processor.read_register(second)?.address(),
                );
                self.processor.write_integer_register(destination, value)?;
            }
            Instruction::Immediate {
                operation,
                destination,
                source,
                immediate,
            } => {
                let value = perform_immediate_operation(
                    operation,
                    self.processor.read_register(source)?.address(),
                    immediate,
                );
                self.processor.write_integer_register(destination, value)?;
            }
            Instruction::LoadUpperImmediate { destination, value } => {
                self.processor.write_integer_register(destination, value)?
            }
            Instruction::AddUpperImmediateToProgramCounter { .. }
            | Instruction::JumpAndLink { .. }
            | Instruction::JumpAndLinkRegister { .. } => return Err(Fault::illegal()),
            Instruction::Branch {
                condition,
                first,
                second,
                offset,
            } => {
                let first = self.processor.read_register(first)?.address();
                let second = self.processor.read_register(second)?.address();
                let taken = condition.test(first, second);
                if taken {
                    following = self.processor.program_counter.wrapping_add(offset as u32);
                }
            }
            Instruction::Load {
                kind,
                destination,
                base,
                offset,
            } => {
                if destination >= 16 {
                    return Err(Fault::illegal());
                }
                let address = self
                    .processor
                    .read_register(base)?
                    .address()
                    .wrapping_add(offset as u32);
                let width = kind.width();
                self.check_memory_access(base, address, width, false, false)?;
                let bits = self.bus.read(address, width).map_err(|_| Fault {
                    cause: 5,
                    value: address,
                })?;
                #[cfg(feature = "verification")]
                self.memory_accesses.push(MemoryAccess::Read {
                    address,
                    width,
                    value: u64::from(bits),
                    tag: None,
                });
                let value = kind.extend(bits);
                self.processor.write_integer_register(destination, value)?;
            }
            Instruction::Store {
                kind,
                source,
                base,
                offset,
            } => {
                let value = self.processor.read_register(source)?.address();
                let address = self
                    .processor
                    .read_register(base)?
                    .address()
                    .wrapping_add(offset as u32);
                let width = kind.width();
                self.check_memory_access(base, address, width, true, false)?;
                self.bus.write(address, width, value).map_err(|_| Fault {
                    cause: 7,
                    value: address,
                })?;
                self.record_store(address, width);
                #[cfg(feature = "verification")]
                self.memory_accesses.push(MemoryAccess::Write {
                    address,
                    width,
                    value: u64::from(value) & ((1_u64 << (width * 8)) - 1),
                    tag: None,
                });
            }
            Instruction::Breakpoint => {
                return Err(Fault {
                    cause: 3,
                    value: self.processor.program_counter,
                })
            }
            Instruction::EnvironmentCall => {
                return Err(Fault {
                    cause: 11,
                    value: 0,
                })
            }
            Instruction::MachineReturn => {
                self.require_system_registers(32)?;
                if self.processor.machine_registers.machine_status & (3 << 11) != 3 << 11 {
                    return Err(Fault::illegal());
                }
                self.processor.machine_registers.machine_return();
                self.processor.pcc = self.processor.machine_exception;
                following = self.processor.machine_exception.address();
            }
            Instruction::Fence | Instruction::FenceInstruction | Instruction::Hint => (),
            Instruction::WaitForInterrupt => {
                self.timing
                    .wait_for_interrupt(&mut self.bus, &mut self.processor.machine_registers);
            }
            Instruction::Csr {
                operation,
                destination,
                source,
                address,
                immediate,
            } => {
                if destination >= 16 {
                    return Err(Fault::illegal());
                }
                let value = if immediate {
                    u32::from(source)
                } else {
                    self.processor.read_register(source)?.address()
                };
                let writing = operation.writes(source);
                Self::check_csr_access(address, writing)?;
                let counter_read = !writing && matches!(address, 0xb00 | 0xb02 | 0xb80 | 0xb82);
                if !counter_read {
                    self.require_system_registers(32)?;
                }
                let previous = match address {
                    0x7a0 => !self.processor.trigger_select,
                    0xbc0 => 3,
                    0xbc1 => self.processor.stack_high_water,
                    0xbc2 => self.processor.stack_base,
                    0x305 => self.processor.machine_trap.address(),
                    0x341 => self.processor.machine_exception.address(),
                    _ => self
                        .processor
                        .machine_registers
                        .read(address)
                        .ok_or_else(Fault::illegal)?,
                };
                if writing {
                    let written = match operation {
                        CsrOperation::ReadWrite => value,
                        CsrOperation::ReadSet => previous | value,
                        CsrOperation::ReadClear => previous & !value,
                    };
                    match address {
                        0x304 => {
                            self.processor.machine_registers.interrupt_enable = written & 0xaaa
                        }
                        0x344 => {
                            self.processor.machine_registers.interrupt_pending =
                                (self.processor.machine_registers.interrupt_pending & !0x222)
                                    | (written & 0x222);
                        }
                        0x300 => {
                            let mask = 0x7f_ffff & !(0x44 | (3 << 15) | (1 << 17) | 0x11);
                            let mut status = written & mask;
                            if status & (3 << 13) == 3 << 13 || status & (3 << 9) == 3 << 9 {
                                status |= 1 << 31;
                            }
                            self.processor.machine_registers.machine_status = status;
                        }
                        0xbc0 => (),
                        0xbc1 => self.processor.stack_high_water = written & !15,
                        0xbc2 => self.processor.stack_base = written & !15,
                        0x7a0 => self.processor.trigger_select = written,
                        0x305 | 0x341 => return Err(Fault::illegal()),
                        _ => self
                            .processor
                            .machine_registers
                            .write(address, written)
                            .map_err(|_| Fault::illegal())?,
                    }
                }
                self.processor
                    .write_integer_register(destination, previous)?;
            }
        }
        self.processor.program_counter = following;

        Ok(())
    }

    fn fetch(
        &mut self,
        fetched: &mut bool,
        injected: Option<u32>,
    ) -> Result<DecodedInstruction, Fault> {
        let address = self.processor.program_counter;
        if !self.processor.pcc.tag() {
            return Err(Fault::capability_cause(CapabilityFaultCause::Tag, 32));
        }
        if self.processor.pcc.sealed() {
            return Err(Fault::capability_cause(CapabilityFaultCause::Seal, 32));
        }
        if self.processor.pcc.permissions() & permissions::EXECUTE == 0 {
            return Err(Fault::capability_cause(
                CapabilityFaultCause::ExecutePermission,
                32,
            ));
        }

        let bounds = self.processor.pcc.bounds();
        self.check_fetch_bounds(address, bounds)?;

        if address & 1 != 0 {
            return Err(Fault {
                cause: 0,
                value: address,
            });
        }
        let first = match injected {
            Some(encoded) => encoded & 0xffff,
            None => self.bus.fetch(address, 2).map_err(|_| Fault {
                cause: 1,
                value: address,
            })?,
        };
        if first & 3 != 3 {
            *fetched = true;
            return decode_compressed(first as u16).ok_or_else(Fault::illegal);
        }
        let second_address = address.wrapping_add(2);
        self.check_fetch_bounds(second_address, bounds)?;
        let second = match injected {
            Some(encoded) => encoded >> 16,
            None => self.bus.fetch(second_address, 2).map_err(|_| Fault {
                cause: 1,
                value: second_address,
            })?,
        };
        *fetched = true;
        decode_standard(first | second << 16).ok_or_else(Fault::illegal)
    }

    fn link(&mut self, destination: u8, address: u32) -> Result<(), Fault> {
        let mut link = self.processor.pcc.set_address(address);
        if destination == 1 {
            let object_type = if self.processor.machine_registers.machine_status & 8 != 0 {
                5
            } else {
                4
            };
            link = Capability::from_memory(
                (link.to_memory().0 & !(7u64 << 54)) | (object_type << 54),
                link.tag(),
            );
        }
        self.processor.write_register(destination, link)
    }

    fn record_store(&mut self, address: u32, width: usize) {
        self.last_store = Some((address, width));
        if address >= self.processor.stack_base && address < self.processor.stack_high_water {
            self.processor.stack_high_water = address & !15;
        }
    }

    fn require_system_registers(&self, register: u8) -> Result<(), Fault> {
        if self.processor.pcc.permissions() & permissions::SYSTEM_REGISTERS == 0 {
            return Err(Fault::capability_cause(
                CapabilityFaultCause::SystemRegisterPermission,
                register,
            ));
        }
        Ok(())
    }

    fn special(
        &mut self,
        index: u8,
        source: u8,
        mut value: Capability,
    ) -> Result<Capability, Fault> {
        if !(28..=31).contains(&index) {
            return Err(Fault::illegal());
        }
        self.require_system_registers(32 | index)?;
        let old = match index {
            28 => self.processor.machine_trap,
            29 => self.processor.machine_data,
            30 => self.processor.machine_scratch,
            _ => self.processor.machine_exception,
        };
        if source != 0 {
            if index == 28 || index == 31 {
                let mask = if index == 28 { 3 } else { 1 };
                let invalid = value.address() & mask != 0
                    || value.sealed()
                    || value.permissions() & permissions::EXECUTE == 0;
                let bits = value.to_memory().0 & !u64::from(mask);
                value = Capability::from_memory(bits, value.tag() && !invalid);
            }
            match index {
                28 => self.processor.machine_trap = value,
                29 => self.processor.machine_data = value,
                30 => self.processor.machine_scratch = value,
                _ => self.processor.machine_exception = value,
            }
        }

        Ok(old)
    }
}
