// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::str::FromStr;

use crate::exec::machine::EmulatorError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UartParity {
    Even,
    #[default]
    None,
    Odd,
}

impl FromStr for UartParity {
    type Err = EmulatorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "even" => Ok(Self::Even),
            "odd" => Ok(Self::Odd),
            _ => Err(EmulatorError::Configuration(
                "UART parity requires none, even, odd".into(),
            )),
        }
    }
}
