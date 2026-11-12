// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy)]
pub enum EscapeMode {
    Text,
    #[cfg_attr(not(test), allow(dead_code))]
    Html,
}
