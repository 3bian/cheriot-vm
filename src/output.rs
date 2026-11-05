// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::completion::HostInterface;
use crate::EmulatorError;

pub fn drain(
    bus: &mut Bus,
    host_interface: &mut Option<HostInterface>,
    scratch: &mut Vec<u8>,
    mut consume: impl FnMut(&str, &str, &[u8]) -> Result<(), EmulatorError>,
) -> Result<(), EmulatorError> {
    bus.drain_output_each(scratch, &mut consume)?;
    if let Some(interface) = host_interface {
        scratch.clear();
        interface.drain_transmitted(scratch);
        if !scratch.is_empty() {
            consume("HTIF", "tohost", scratch)?;
        }
    }
    Ok(())
}
