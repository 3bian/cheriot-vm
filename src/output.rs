// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::completion::HostInterface;
use crate::EmulatorError;

#[cfg(all(feature = "gdb", target_os = "linux"))]
#[inline]
pub fn debugger_escape(byte: u8) -> ([u8; 4], usize) {
    if matches!(byte, b'\n' | b'\t' | 0x20..=0x7e) {
        ([byte, 0, 0, 0], 1)
    } else {
        let hex = b"0123456789abcdef";
        (
            [
                b'\\',
                b'x',
                hex[usize::from(byte >> 4)],
                hex[usize::from(byte & 15)],
            ],
            4,
        )
    }
}

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
