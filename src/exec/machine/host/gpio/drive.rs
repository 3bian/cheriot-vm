// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use crate::exec::machine::EmulatorError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpioDrive {
    OpenDrain,
    OpenSource,
    #[default]
    PushPull,
}

impl FromStr for GpioDrive {
    type Err = EmulatorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open-drain" => Ok(Self::OpenDrain),
            "open-source" => Ok(Self::OpenSource),
            "push-pull" => Ok(Self::PushPull),
            _ => Err(EmulatorError::Configuration(
                "GPIO drive requires open-drain, open-source, push-pull".into(),
            )),
        }
    }
}
