// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Debug)]
pub(crate) struct LoadableSegment {
    pub(super) address: u32,
    pub(super) executable: bool,
    pub(super) file_offset: usize,
    pub(super) file_size: usize,
    pub(super) memory_size: usize,
}
