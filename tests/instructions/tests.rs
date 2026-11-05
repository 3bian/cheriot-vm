// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn known_encodings() {
    let add = decode_standard(0x002081b3, false).unwrap();
    assert_eq!(
        add.instruction,
        Instruction::Register {
            operation: RegisterOperation::Add,
            destination: 3,
            first: 1,
            second: 2,
        }
    );
    assert!(decode_standard(0x022081b3, false).is_none());
    assert!(decode_standard(0x022081b3, true).is_some());
    assert!(decode_standard(0xffff_ffff, true).is_none());
}

#[test]
fn signed_immediate_extremes() {
    assert_eq!(sign_extend(0xfff, 12), -1);
    assert_eq!(sign_extend(0x800, 12), -2048);
    assert_eq!(sign_extend(0x7ff, 12), 2047);
}
