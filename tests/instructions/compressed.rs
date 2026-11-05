// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn standard_compressed_examples() {
    let nop = decode_compressed(0x0001).unwrap();
    assert_eq!(nop.length, 2);
    assert!(matches!(nop.instruction, Instruction::Hint));
    assert_eq!(
        decode_compressed(0x0085).unwrap().instruction,
        Instruction::Immediate {
            operation: ImmediateOperation::Add,
            destination: 1,
            source: 1,
            immediate: 1
        }
    );
    assert_eq!(
        decode_compressed(0x8082).unwrap().instruction,
        Instruction::JumpAndLinkRegister {
            destination: 0,
            base: 1,
            offset: 0
        }
    );
    assert!(decode_compressed(0).is_none());
}
