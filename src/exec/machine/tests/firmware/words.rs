// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) fn firmware_from_words(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}
