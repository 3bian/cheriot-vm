// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::EmulatorError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Register {
    Capability(u8),
    Integer(u8),
    Mepcc,
    Mscratchc,
    Mtcc,
    Mtdc,
    Pc,
    Pcc,
}

impl Register {
    pub(crate) fn parse(name: &str) -> Result<Self, EmulatorError> {
        let selection = match name {
            "mepcc" => Self::Mepcc,
            "mscratchc" => Self::Mscratchc,
            "mtcc" => Self::Mtcc,
            "mtdc" => Self::Mtdc,
            "pc" => Self::Pc,
            "pcc" => Self::Pcc,
            _ => {
                if let Some(index) = name
                    .strip_prefix('x')
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|index| *index < 32)
                {
                    return Ok(Self::Integer(index));
                }
                if let Some(index) = name
                    .strip_prefix('c')
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|index| *index < 16)
                {
                    return Ok(Self::Capability(index));
                }
                return Err(EmulatorError::Configuration(format!(
                    "unknown register: {name}; use x0..x31, pc, or a CHERIoT capability register"
                )));
            }
        };
        Ok(selection)
    }

    pub(crate) fn validate(self, cheriot: bool, embedded: bool) -> Result<(), EmulatorError> {
        let valid = match self {
            Self::Pc => true,
            Self::Integer(index) => !cheriot && index < if embedded { 16 } else { 32 },
            _ => cheriot,
        };
        if !valid {
            return Err(EmulatorError::Configuration(format!(
                "requested register is unavailable for this processor; {}",
                if cheriot {
                    "use c0..c15, pc, pcc, mtcc, mtdc, mscratchc or mepcc"
                } else if embedded {
                    "use x0..x15 or pc"
                } else {
                    "use x0..x31 or pc"
                }
            )));
        }
        Ok(())
    }
}
