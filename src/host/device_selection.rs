// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::bus::Bus;
use crate::EmulatorError;

pub fn resolve<T: 'static>(
    bus: &Bus,
    device: Option<&str>,
    kind: &str,
) -> Result<String, EmulatorError> {
    if let Some(name) = device {
        if bus.device::<T>(name).is_none() {
            return Err(EmulatorError::Configuration(format!(
                "no {kind} device named {name}"
            )));
        }
        return Ok(name.to_owned());
    }
    let names: Vec<_> = bus
        .devices()
        .iter()
        .filter(|mapping| mapping.device.as_any().is::<T>())
        .map(|mapping| mapping.name.as_str())
        .collect();
    match names.as_slice() {
        [] => Err(EmulatorError::Configuration(format!(
            "no {kind} device configured"
        ))),
        [name] => Ok((*name).to_owned()),
        _ => Err(EmulatorError::Configuration(format!(
            "multiple {kind} devices configured; add device=NAME (available: {})",
            names.join(", ")
        ))),
    }
}
