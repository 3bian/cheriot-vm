// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::FrameTransport;

pub enum NetworkBackend {
    #[cfg_attr(not(test), allow(dead_code))]
    Disconnected,
    Host(Box<dyn FrameTransport>),
    InMemory,
}
