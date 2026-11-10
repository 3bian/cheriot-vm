// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) fn write_elf32_header(image: &mut [u8], entry: u32, flags: u32, program_headers: u16) {
    image[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
    for (offset, value) in [
        (16, 2_u16),
        (18, 243),
        (40, 52),
        (42, 32),
        (44, program_headers),
    ] {
        image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, value) in [(20, 1_u32), (24, entry), (28, 52), (36, flags)] {
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
