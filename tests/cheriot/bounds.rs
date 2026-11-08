// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::cheriot::Capability;

#[test]
fn bounds_rounding_handles_alignment_exponent_gaps_and_address_space_end() {
    let cases = [
        (1, 511, 0, 1, 512, true, true),
        (1, 512, 1, 0, 514, false, true),
        (0x1000, 1022, 1, 0x1000, 0x13fe, true, true),
        (0x1001, 1021, 1, 0x1000, 0x13fe, false, true),
        (0x1001, 1022, 2, 0x1000, 0x1400, false, true),
        (0, 0x7fc000, 14, 0, 0x7fc000, true, true),
        (0, 0x7fc001, 24, 0, 0x1000000, false, true),
        (0, u32::MAX, 24, 0, 0x100000000, false, true),
        (1, u32::MAX, 24, 0, 0x100000000, false, true),
        (u32::MAX, 1, 0, u32::MAX, 0x100000000, true, true),
        (u32::MAX, 2, 0, u32::MAX, 0x100000001, true, false),
    ];

    for (address, length, exponent, base, top, exact, valid) in cases {
        let authority = Capability::from_memory(0x7e3e_0000_0000_0000, true).set_address(address);

        for require_exact in [false, true] {
            let (bounded, was_exact) = authority.set_bounds(length, require_exact);
            assert_eq!(bounded.address(), address);
            assert_eq!(bounded.exponent(), exponent, "{address:#x}, {length:#x}");
            assert_eq!(bounded.bounds(), (base, top));
            assert_eq!(bounded.tag(), valid && (!require_exact || exact));
            assert_eq!(was_exact, exact);

            let invalid = authority.clear_tag().set_bounds(length, require_exact).0;
            assert!(!invalid.tag());
            assert_eq!(invalid.to_memory().0, bounded.to_memory().0);
        }
    }
}
