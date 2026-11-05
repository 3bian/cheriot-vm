// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub mod arithmetic;
mod branch_condition;
mod compressed;
mod csr_operation;
mod decoded_instruction;
mod immediate_operation;
mod instruction;
mod load_kind;
mod register_operation;
mod store_kind;

pub use branch_condition::BranchCondition;
pub use compressed::decode_compressed;
pub use csr_operation::CsrOperation;
pub use decoded_instruction::DecodedInstruction;
pub use immediate_operation::ImmediateOperation;
pub use instruction::Instruction;
pub use load_kind::LoadKind;
pub use register_operation::RegisterOperation;
pub use store_kind::StoreKind;

pub fn decode_standard(word: u32, multiply_divide: bool) -> Option<DecodedInstruction> {
    if word & 3 != 3 {
        return None;
    }

    let opcode = word & 0x7f;
    let destination = ((word >> 7) & 31) as u8;
    let function_code = (word >> 12) & 7;
    let first = ((word >> 15) & 31) as u8;
    let second = ((word >> 20) & 31) as u8;
    let extended_function_code = (word >> 25) & 0x7f;
    let signed_immediate = sign_extend(word >> 20, 12);
    let instruction = match opcode {
        0x37 => Instruction::LoadUpperImmediate {
            destination,
            value: word & 0xfffff000,
        },
        0x17 => Instruction::AddUpperImmediateToProgramCounter {
            destination,
            value: word & 0xfffff000,
        },
        0x6f => {
            let immediate = ((word >> 31) << 20)
                | (((word >> 12) & 0xff) << 12)
                | (((word >> 20) & 1) << 11)
                | (((word >> 21) & 0x3ff) << 1);
            Instruction::JumpAndLink {
                destination,
                offset: sign_extend(immediate, 21),
            }
        }
        0x67 if function_code == 0 => Instruction::JumpAndLinkRegister {
            destination,
            base: first,
            offset: signed_immediate,
        },
        0x63 => {
            let condition = match function_code {
                0 => BranchCondition::Equal,
                1 => BranchCondition::NotEqual,
                4 => BranchCondition::LessThan,
                5 => BranchCondition::GreaterOrEqual,
                6 => BranchCondition::LessThanUnsigned,
                7 => BranchCondition::GreaterOrEqualUnsigned,
                _ => return None,
            };
            let immediate = ((word >> 31) << 12)
                | (((word >> 7) & 1) << 11)
                | (((word >> 25) & 0x3f) << 5)
                | (((word >> 8) & 15) << 1);
            Instruction::Branch {
                condition,
                first,
                second,
                offset: sign_extend(immediate, 13),
            }
        }
        0x03 => {
            let kind = match function_code {
                0 => LoadKind::SignedByte,
                1 => LoadKind::SignedHalfword,
                2 => LoadKind::Word,
                4 => LoadKind::UnsignedByte,
                5 => LoadKind::UnsignedHalfword,
                _ => return None,
            };
            Instruction::Load {
                kind,
                destination,
                base: first,
                offset: signed_immediate,
            }
        }
        0x23 => {
            let kind = match function_code {
                0 => StoreKind::Byte,
                1 => StoreKind::Halfword,
                2 => StoreKind::Word,
                _ => return None,
            };
            let immediate = ((word >> 25) << 5) | ((word >> 7) & 31);
            Instruction::Store {
                kind,
                source: second,
                base: first,
                offset: sign_extend(immediate, 12),
            }
        }
        0x13 => {
            let operation = match function_code {
                0 => ImmediateOperation::Add,
                2 => ImmediateOperation::SetLessThan,
                3 => ImmediateOperation::SetLessThanUnsigned,
                4 => ImmediateOperation::Xor,
                6 => ImmediateOperation::Or,
                7 => ImmediateOperation::And,
                1 if extended_function_code == 0 => ImmediateOperation::ShiftLeft,
                5 if extended_function_code == 0 => ImmediateOperation::ShiftRightLogical,
                5 if extended_function_code == 0x20 => ImmediateOperation::ShiftRightArithmetic,
                _ => return None,
            };
            Instruction::Immediate {
                operation,
                destination,
                source: first,
                immediate: if matches!(
                    operation,
                    ImmediateOperation::ShiftLeft
                        | ImmediateOperation::ShiftRightLogical
                        | ImmediateOperation::ShiftRightArithmetic
                ) {
                    (second as i32) & 31
                } else {
                    signed_immediate
                },
            }
        }
        0x33 => {
            use RegisterOperation::*;
            let operation = match (extended_function_code, function_code) {
                (0, 0) => Add,
                (0x20, 0) => Sub,
                (0, 1) => ShiftLeft,
                (0, 2) => SetLessThan,
                (0, 3) => SetLessThanUnsigned,
                (0, 4) => Xor,
                (0, 5) => ShiftRightLogical,
                (0x20, 5) => ShiftRightArithmetic,
                (0, 6) => Or,
                (0, 7) => And,
                (1, 0) if multiply_divide => Mul,
                (1, 1) if multiply_divide => MulHighSigned,
                (1, 2) if multiply_divide => MulHighSignedUnsigned,
                (1, 3) if multiply_divide => MulHighUnsigned,
                (1, 4) if multiply_divide => DivSigned,
                (1, 5) if multiply_divide => DivUnsigned,
                (1, 6) if multiply_divide => RemainderSigned,
                (1, 7) if multiply_divide => RemainderUnsigned,
                _ => return None,
            };
            Instruction::Register {
                operation,
                destination,
                first,
                second,
            }
        }
        0x0f => match function_code {
            0 => Instruction::Fence,
            1 => Instruction::FenceInstruction,
            _ => return None,
        },
        0x73 => match function_code {
            0 => {
                if destination != 0 || first != 0 {
                    return None;
                }

                match word >> 20 {
                    0 => Instruction::EnvironmentCall,
                    1 => Instruction::Breakpoint,
                    0x302 => Instruction::MachineReturn,
                    0x105 => Instruction::WaitForInterrupt,
                    _ => return None,
                }
            }
            1..=3 | 5..=7 => Instruction::Csr {
                operation: match function_code % 4 {
                    1 => CsrOperation::ReadWrite,
                    2 => CsrOperation::ReadSet,
                    _ => CsrOperation::ReadClear,
                },
                destination,
                source: first,
                address: (word >> 20) as u16,
                immediate: function_code >= 5,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(DecodedInstruction {
        instruction,
        length: 4,
        encoded: word,
    })
}

pub fn sign_extend(value: u32, bits: u32) -> i32 {
    ((value << (32 - bits)) as i32) >> (32 - bits)
}

#[cfg(test)]
#[path = "../tests/instructions/tests.rs"]
mod tests;
