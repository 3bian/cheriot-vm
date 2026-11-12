// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommandName {
    Configuration,
    Plan,
    Build,
    Clean,
    Size,
}

impl CommandName {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Plan => "plan",
            Self::Build => "build",
            Self::Clean => "clean",
            Self::Size => "size",
        }
    }
}
