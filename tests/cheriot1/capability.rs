// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn bounds_rounding_and_authority_are_distinct() {
    let root = Capability::from_memory(0x7e3e_0000_0000_0000, true).set_address(0x1001);
    let (bounded, exact) = root.set_bounds(512, false);

    assert!(!exact);
    assert_eq!(bounded.bounds(), (0x1000, 0x1202));
    assert!(bounded.tag());
    assert!(!root.set_bounds(512, true).0.tag());
    assert!(!bounded.set_bounds(1000, false).0.tag());
    assert!(bounded.set_address(0x1202).tag());
    assert!(!bounded.set_address(0x2000).tag());
}

#[test]
fn root_memory_and_null() {
    let root = Capability::from_memory(0x7e3e_0000_0000_0000, true);

    assert_eq!(root.bounds(), (0, 1u64 << 32));
    assert_eq!(root.permissions(), 0x7f);
    assert_eq!(root.object_type(), 0);
    assert_eq!(root.exponent(), 24);

    let null = Capability::integer(0);

    assert_eq!(null.bounds(), (0, 0));
    assert_eq!(null.permissions(), 0);
    assert!(!null.tag());
}

#[test]
fn sealed_address_changes_clear_tags_and_revocation_excludes_sealing_authority() {
    let sealed = Capability::from_memory(0x7e7e_0000_0000_0100, true);

    assert_eq!(sealed.object_type(), 9);
    assert!(!sealed.set_address(0x100).tag());

    let memory = Capability::from_memory(0x7e3e_0000_0000_0000, true);

    assert!(!memory.filter_revoked(true).tag());

    let seal_root = Capability::from_memory(0x4e3e_0000_0000_0000, true);

    assert_eq!(seal_root.permissions(), 0xe01);
    assert!(seal_root.filter_revoked(true).tag());
}
