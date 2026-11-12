// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;
#[test]
fn immediate_sign() {
    assert_eq!(
        perform_immediate_operation(ImmediateOperation::Add, 0, -1),
        u32::MAX
    );
    assert_eq!(
        perform_immediate_operation(ImmediateOperation::SetLessThanUnsigned, 1, -1),
        1
    );
}

#[test]
fn multiplication_division_edge_cases() {
    assert_eq!(
        perform_register_operation(RegisterOperation::MulHighUnsigned, u32::MAX, u32::MAX),
        0xffff_fffe
    );
    assert_eq!(
        perform_register_operation(RegisterOperation::MulHighSignedUnsigned, u32::MAX, u32::MAX),
        u32::MAX
    );
    assert_eq!(
        perform_register_operation(RegisterOperation::DivSigned, i32::MIN as u32, u32::MAX),
        i32::MIN as u32
    );
    assert_eq!(
        perform_register_operation(
            RegisterOperation::RemainderSigned,
            i32::MIN as u32,
            u32::MAX
        ),
        0
    );
    assert_eq!(
        perform_register_operation(RegisterOperation::DivUnsigned, 1, 0),
        u32::MAX
    );
    assert_eq!(
        perform_register_operation(RegisterOperation::RemainderUnsigned, 42, 0),
        42
    );
}
