// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{ImmediateOperation, RegisterOperation};

pub fn perform_immediate_operation(
    operation: ImmediateOperation,
    first: u32,
    immediate: i32,
) -> u32 {
    use ImmediateOperation::*;
    match operation {
        Add => first.wrapping_add(immediate as u32),
        SetLessThan => u32::from((first as i32) < immediate),
        SetLessThanUnsigned => u32::from(first < immediate as u32),
        Xor => first ^ immediate as u32,
        Or => first | immediate as u32,
        And => first & immediate as u32,
        ShiftLeft => first.wrapping_shl(immediate as u32 & 31),
        ShiftRightLogical => first.wrapping_shr(immediate as u32 & 31),
        ShiftRightArithmetic => ((first as i32) >> (immediate as u32 & 31)) as u32,
    }
}

pub fn perform_register_operation(operation: RegisterOperation, first: u32, second: u32) -> u32 {
    use RegisterOperation::*;
    match operation {
        Add => first.wrapping_add(second),
        Sub => first.wrapping_sub(second),
        ShiftLeft => first.wrapping_shl(second & 31),
        SetLessThan => u32::from((first as i32) < (second as i32)),
        SetLessThanUnsigned => u32::from(first < second),
        Xor => first ^ second,
        ShiftRightLogical => first.wrapping_shr(second & 31),
        ShiftRightArithmetic => ((first as i32) >> (second & 31)) as u32,
        Or => first | second,
        And => first & second,
        Mul => first.wrapping_mul(second),
        MulHighSigned => (((first as i32 as i64) * (second as i32 as i64)) >> 32) as u32,
        MulHighSignedUnsigned => (((first as i32 as i64) * (second as u64 as i64)) >> 32) as u32,
        MulHighUnsigned => (((first as u64) * (second as u64)) >> 32) as u32,
        DivSigned if second == 0 => u32::MAX,
        DivSigned if first == 0x8000_0000 && second == u32::MAX => first,
        DivSigned => ((first as i32) / (second as i32)) as u32,
        DivUnsigned if second == 0 => u32::MAX,
        DivUnsigned => first / second,
        RemainderSigned if second == 0 => first,
        RemainderSigned if first == 0x8000_0000 && second == u32::MAX => 0,
        RemainderSigned => ((first as i32) % (second as i32)) as u32,
        RemainderUnsigned if second == 0 => first,
        RemainderUnsigned => first % second,
    }
}

#[cfg(test)]
#[path = "../tests/instructions/arithmetic.rs"]
mod tests;
