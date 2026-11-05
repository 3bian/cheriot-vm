// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use gdbstub::arch::Arch;
use gdbstub::target::Target;

use super::StopReason;
use crate::EmulatorError;

pub trait SessionTarget: Target<Error = EmulatorError>
where
    Self::Arch: Arch<Usize = u32>,
{
    fn poll_input(
        &mut self,
        _input: &mut crate::host::uart::input::UartInput,
    ) -> Result<(), EmulatorError> {
        Err(EmulatorError::Configuration(
            "this debugger target does not support live uart input".into(),
        ))
    }

    fn advance(&mut self) -> Result<Option<StopReason>, EmulatorError>;

    fn drain_output(&mut self, bytes: &mut Vec<u8>);

    fn drain_output_each(
        &mut self,
        scratch: &mut Vec<u8>,
        consume: &mut impl FnMut(&str, &str, &[u8]) -> Result<(), EmulatorError>,
    ) -> Result<(), EmulatorError>;

    fn halt(&mut self, reason: StopReason) -> StopReason;
}
