// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepReport<T> {
    pub completion: Option<u64>,
    pub outcome: T,
}
