// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::cheriot1::{permissions::*, Capability};

const MEMORY: u64 = 0x7e3e_0000_0000_0000;
const SEALING: u64 = 0x4e3e_0000_0000_0000;
const EXECUTABLE: u64 = 0x5e3e_0000_0000_0000;

#[test]
fn all_permission_formats_and_otype_expansion() {
    let cases = [
        (0, 0),
        (7, USER0 | SEAL | UNSEAL),
        (8, EXECUTE | LOAD | LOAD_STORE_CAPABILITY),
        (
            15,
            EXECUTE | LOAD | LOAD_STORE_CAPABILITY | SYSTEM_REGISTERS | LOAD_MUTABLE | LOAD_GLOBAL,
        ),
        (16, STORE | LOAD_STORE_CAPABILITY),
        (17, STORE),
        (18, LOAD),
        (19, LOAD | STORE),
        (20, LOAD | LOAD_STORE_CAPABILITY),
        (
            23,
            LOAD | LOAD_STORE_CAPABILITY | LOAD_MUTABLE | LOAD_GLOBAL,
        ),
        (24, LOAD | STORE | LOAD_STORE_CAPABILITY),
        (31, 0x7e),
    ];

    for (compressed, expected) in cases {
        for global in [0, 1] {
            for object_type in 0..8 {
                let bits = (((compressed | global << 5) as u64) << 57) | (object_type << 54);
                let c = Capability::from_memory(bits, true);
                assert_eq!(c.permissions(), expected | global);
                let expanded = if object_type == 0 || expected & EXECUTE != 0 {
                    object_type
                } else {
                    object_type + 8
                };
                assert_eq!(c.object_type(), expanded as u8);
                assert_eq!(c.to_memory(), (bits, true));
            }
        }
    }
}

#[test]
fn base_top_corrections_and_exponent_band_gap() {
    let c = Capability::from_memory((0x10 << 41) | (0x1f0 << 32) | 0x200, true);
    assert_eq!(c.bounds(), (0x1f0, 0x210));
    assert!(c.contains(0x210, 0));
    assert!(!c.contains(0x210, 1));
    assert!(!c.contains(0x200, u64::MAX));
    let root = Capability::from_memory(MEMORY, true);

    for length in [0, 1, 510, 511, 512, 513, 0x7f_ffff, 0x80_0000, 0xffff_ffff] {
        let (bounded, exact) = root.set_bounds(length, false);
        let (base, top) = bounded.bounds();
        assert_eq!(base, 0);
        assert!(top >= u64::from(length));
        assert_eq!(exact, top == u64::from(length));
        assert!(bounded.exponent() <= 14 || bounded.exponent() == 24);
    }
}

#[test]
fn permission_restriction_never_grants_new_authority() {
    for compressed in 0..64 {
        let capability = Capability::from_memory((compressed << 57) | 0x003e_0000_0000_0000, true);

        for mask in 0..=0xfff {
            let narrowed = capability.restrict_permissions(mask);
            assert_eq!(
                narrowed.permissions() & !(capability.permissions() & mask),
                0
            );
            assert!(narrowed.tag());
            assert_eq!(narrowed.bounds(), capability.bounds());
        }
    }

    let sealed = Capability::from_memory(MEMORY, true)
        .seal(Capability::from_memory(SEALING, true).set_address(9));
    assert!(sealed.restrict_permissions(0xfff).tag());
    assert!(sealed.restrict_permissions(0xffe).tag());
    assert!(!sealed.restrict_permissions(0x7ff).tag());
}

#[test]
fn revocation_uses_base_and_exempts_only_sealing_permissions() {
    let root = Capability::from_memory(MEMORY, true);
    let c = root
        .set_address(0x1000)
        .set_bounds(128, true)
        .0
        .set_address(0x107f);
    assert_eq!(c.bounds().0 & !7, 0x1000);
    assert!(!c.filter_revoked(true).tag());
    assert!(c.filter_revoked(false).tag());

    for permission in [SEAL, UNSEAL, USER0] {
        let authority = Capability::from_memory(SEALING, true).restrict_permissions(permission);
        assert!(authority.filter_revoked(true).tag());
    }

    assert!(!root.clear_tag().filter_revoked(false).tag());
}

#[test]
fn sealing_type_ranges_and_unsealing_bounds_ignore_authority_cursor() {
    let root = Capability::from_memory(MEMORY, true);
    let sealing = Capability::from_memory(SEALING, true);

    for object_type in 0..=16 {
        let authority = sealing.set_address(object_type);
        assert_eq!(root.seal(authority).tag(), (9..=15).contains(&object_type));
        let executable = Capability::from_memory(EXECUTABLE, true);
        assert_eq!(
            executable.seal(authority).tag(),
            (1..=7).contains(&object_type)
        );
    }

    let sealed = root.seal(sealing.set_address(9));
    let unsealed = sealed.unseal(sealing.set_address(200));
    assert!(unsealed.tag());
    assert!(!unsealed.sealed());
    assert_eq!(unsealed.bounds(), root.bounds());
    assert!(!sealed.seal(sealing.set_address(10)).tag());
    assert!(!sealed.unseal(sealing.clear_tag()).tag());
    let local = sealing.restrict_permissions(0xffe);
    assert_eq!(sealed.unseal(local).permissions() & GLOBAL, 0);
}
