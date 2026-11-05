// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::cheriot1_tags::Cheriot1Tags;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tagging {
    Cheriot1,
    #[default]
    None,
}

impl Tagging {
    pub const fn granule_bytes(self) -> Option<usize> {
        match self {
            Self::Cheriot1 => Some(Cheriot1Tags::GRANULE_BYTES),
            Self::None => None,
        }
    }
}
