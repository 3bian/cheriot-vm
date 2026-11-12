// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use crate::exec::machine::EmulatorError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpioBias {
    Disabled,
    PullDown,
    PullUp,
}

impl FromStr for GpioBias {
    type Err = EmulatorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "disabled" => Ok(Self::Disabled),
            "pull-down" => Ok(Self::PullDown),
            "pull-up" => Ok(Self::PullUp),
            _ => Err(EmulatorError::Configuration(
                "GPIO bias requires disabled, pull-down, pull-up".into(),
            )),
        }
    }
}
