// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use crate::EmulatorError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UartFlow {
    #[default]
    None,
    RtsCts,
    XonXoff,
}

impl FromStr for UartFlow {
    type Err = EmulatorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "rtscts" => Ok(Self::RtsCts),
            "xonxoff" => Ok(Self::XonXoff),
            _ => Err(EmulatorError::Configuration(
                "UART flow requires none, rtscts, xonxoff".into(),
            )),
        }
    }
}
