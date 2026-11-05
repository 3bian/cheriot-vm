// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::FrameTransport;

pub enum NetworkBackend {
    Disconnected,
    Host(Box<dyn FrameTransport>),
    InMemory,
}
