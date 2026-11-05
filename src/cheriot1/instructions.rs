// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::instructions::{self, Instruction};

use super::{DecodedInstruction, Operation};

pub fn decode_standard(word: u32) -> Option<DecodedInstruction> {
    let mut decoded = DecodedInstruction {
        destination: ((word >> 7) & 31) as u8,
        encoded: word,
        first: ((word >> 15) & 31) as u8,
        immediate: (word as i32) >> 20,
        length: 4,
        operation: Operation::Move,
        second: ((word >> 20) & 31) as u8,
    };
    let function = (word >> 12) & 7;
    decoded.operation = match word & 127 {
        0x17 | 0x7b => {
            decoded.first = if word & 127 == 0x7b { 3 } else { 0 };
            decoded.second = 0;
            decoded.immediate = ((word as i32) >> 12).wrapping_shl(11);

            if word & 127 == 0x7b {
                Operation::AddressFromGlobal
            } else {
                Operation::AddressFromPc
            }
        }
        0x03 if function == 3 => {
            decoded.second = 0;
            Operation::Load
        }
        0x23 if function == 3 => {
            decoded.destination = 0;
            decoded.immediate =
                instructions::sign_extend(((word >> 25) << 5) | ((word >> 7) & 31), 12);
            Operation::Store
        }
        0x5b => match function {
            0 => match word >> 25 {
                0x01 => Operation::SpecialReadWrite,
                0x08 => Operation::SetBounds,
                0x09 => Operation::SetBoundsExact,
                0x0a => Operation::SetBoundsRoundDown,
                0x0b => Operation::Seal,
                0x0c => Operation::Unseal,
                0x0d => Operation::AndPermissions,
                0x10 => Operation::SetAddress,
                0x11 => Operation::IncrementAddress,
                0x14 => Operation::Subtract,
                0x16 => Operation::SetHigh,
                0x20 => Operation::TestSubset,
                0x21 => Operation::TestEqual,
                0x7f => {
                    let selector = decoded.second;
                    decoded.second = 0;

                    match selector {
                        0 => Operation::GetPermissions,
                        1 => Operation::GetType,
                        2 => Operation::GetBase,
                        3 => Operation::GetLength,
                        4 => Operation::GetTag,
                        8 => Operation::RepresentableLength,
                        9 => Operation::RepresentableAlignment,
                        10 => Operation::Move,
                        11 => Operation::ClearTag,
                        15 => Operation::GetAddress,
                        23 => Operation::GetHigh,
                        24 => Operation::GetTop,
                        _ => return None,
                    }
                }
                _ => return None,
            },
            1 => {
                decoded.second = 0;
                Operation::IncrementAddressImmediate
            }
            2 => {
                decoded.second = 0;
                decoded.immediate = (word >> 20) as i32;
                Operation::SetBoundsImmediate
            }
            _ => return None,
        },
        _ => {
            let ordinary = instructions::decode_standard(word, true)?;

            match ordinary.instruction {
                Instruction::JumpAndLink {
                    destination,
                    offset,
                } => {
                    decoded.destination = destination;
                    decoded.first = 0;
                    decoded.second = 0;
                    decoded.immediate = offset;
                    Operation::Jump
                }
                Instruction::JumpAndLinkRegister {
                    destination,
                    base,
                    offset,
                } => {
                    decoded.destination = destination;
                    decoded.first = base;
                    decoded.second = 0;
                    decoded.immediate = offset;
                    Operation::JumpRegister
                }
                instruction => {
                    return Some(DecodedInstruction {
                        destination: 0,
                        first: 0,
                        second: 0,
                        immediate: 0,
                        operation: Operation::Integer(instruction),
                        ..decoded
                    })
                }
            }
        }
    };

    if decoded.destination >= 16
        || decoded.first >= 16
        || (decoded.operation != Operation::SpecialReadWrite && decoded.second >= 16)
    {
        return None;
    }

    Some(decoded)
}

pub fn decode_compressed(halfword: u16) -> Option<DecodedInstruction> {
    let word = u32::from(halfword);
    let mut decoded = DecodedInstruction {
        destination: ((word >> 7) & 31) as u8,
        encoded: word,
        first: 2,
        immediate: 0,
        length: 2,
        operation: Operation::Move,
        second: ((word >> 2) & 31) as u8,
    };

    match (word & 3, word >> 13) {
        (0, 3 | 7) => {
            decoded.destination = 8 + ((word >> 2) & 7) as u8;
            decoded.first = 8 + ((word >> 7) & 7) as u8;
            decoded.immediate = (((word >> 10) & 7) << 3 | ((word >> 5) & 3) << 6) as i32;
            decoded.operation = if word >> 13 == 3 {
                Operation::Load
            } else {
                Operation::Store
            };
            decoded.second = if decoded.operation == Operation::Store {
                decoded.destination
            } else {
                0
            };
        }
        (2, 3) => {
            if decoded.destination == 0 {
                return None;
            }

            decoded.immediate =
                (((word >> 12) & 1) << 5 | ((word >> 5) & 3) << 3 | ((word >> 2) & 7) << 6) as i32;
            decoded.operation = Operation::Load;
            decoded.second = 0;
        }
        (2, 7) => {
            decoded.destination = 0;
            decoded.immediate = (((word >> 10) & 7) << 3 | ((word >> 7) & 7) << 6) as i32;
            decoded.operation = Operation::Store;
        }
        _ => {
            let ordinary = instructions::decode_compressed(halfword)?;
            decoded.destination = 0;
            decoded.first = 0;
            decoded.second = 0;
            decoded.operation = match ordinary.instruction {
                Instruction::Immediate {
                    destination,
                    source,
                    immediate,
                    ..
                } if (word & 3 == 0 && word >> 13 == 0)
                    || (word & 3 == 1 && word >> 13 == 3 && destination == 2) =>
                {
                    decoded.destination = destination;
                    decoded.first = source;
                    decoded.immediate = immediate;
                    Operation::IncrementAddressImmediate
                }
                Instruction::JumpAndLink {
                    destination,
                    offset,
                } => {
                    decoded.destination = destination;
                    decoded.immediate = offset;
                    Operation::Jump
                }
                Instruction::JumpAndLinkRegister {
                    destination,
                    base,
                    offset,
                } => {
                    decoded.destination = destination;
                    decoded.first = base;
                    decoded.immediate = offset;
                    Operation::JumpRegister
                }
                instruction => Operation::Integer(instruction),
            };
        }
    }

    (decoded.destination < 16 && decoded.first < 16 && decoded.second < 16).then_some(decoded)
}
