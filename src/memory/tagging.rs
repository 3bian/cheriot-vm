// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::cheriot_tags::CheriotTags;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tagging {
    Cheriot,
    #[default]
    None,
}

impl Tagging {
    pub const fn granule_bytes(self) -> Option<usize> {
        match self {
            Self::Cheriot => Some(CheriotTags::GRANULE_BYTES),
            Self::None => None,
        }
    }
}
