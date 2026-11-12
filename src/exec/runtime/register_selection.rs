// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;

use crate::exec::runtime::register::Register;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RegisterSelection {
    Names(Vec<Register>),
}

impl RegisterSelection {
    pub(crate) fn includes(&self, register: Register) -> bool {
        let Self::Names(names) = self;
        names.contains(&register)
    }

    pub(crate) fn parse(value: &str) -> Result<Self, EmulatorError> {
        let mut names = Vec::new();
        for name in value.split('+') {
            if names.len() == 64 {
                return Err(EmulatorError::Configuration(
                    "at most 64 register names are accepted".into(),
                ));
            }
            let register = Register::parse(name)?;
            if names.contains(&register) {
                return Err(EmulatorError::Configuration(format!(
                    "duplicate register: {name}"
                )));
            }
            names.push(register);
        }
        Ok(Self::Names(names))
    }

    pub(crate) fn validate(&self, cheriot: bool, embedded: bool) -> Result<(), EmulatorError> {
        let Self::Names(names) = self;
        for name in names {
            name.validate(cheriot, embedded)?;
        }
        Ok(())
    }
}
