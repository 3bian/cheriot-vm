// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub fn program(cheriot: bool, output: u8, complete: bool) -> Vec<u8> {
    let mut words = if cheriot {
        vec![0x03d0_00db, 0x1000_0137, 0x2020_80db]
    } else {
        vec![0x1000_00b7]
    };
    words.extend([0x0550_0193, 0x0030_a023]);
    if cheriot {
        words.extend([0x03d0_00db, 0x8000_0137, 0x1001_0113, 0x2020_80db]);
    } else {
        words.extend([0x8000_00b7, 0x1000_8093]);
    }
    words.extend([
        (u32::from(output) << 20) | 0x193,
        0x0030_a023,
        0x0101_01b7,
        0x0030_a223,
    ]);
    if complete {
        words.extend([0x0010_0193, 0x0030_a023, 0x0000_a223]);
    }
    words.push(0x0010_0073);
    crate::firmware_words::firmware_from_words(&words)
}
