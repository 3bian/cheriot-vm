// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchCondition {
    Equal,
    GreaterOrEqual,
    GreaterOrEqualUnsigned,
    LessThan,
    LessThanUnsigned,
    NotEqual,
}

impl BranchCondition {
    pub fn test(self, lhs: u32, rhs: u32) -> bool {
        match self {
            Self::Equal => lhs == rhs,
            Self::NotEqual => lhs != rhs,
            Self::LessThan => (lhs as i32) < rhs as i32,
            Self::LessThanUnsigned => lhs < rhs,
            Self::GreaterOrEqual => (lhs as i32) >= rhs as i32,
            Self::GreaterOrEqualUnsigned => lhs >= rhs,
        }
    }
}
