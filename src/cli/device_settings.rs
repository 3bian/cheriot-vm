// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use cheriot_vm::EmulatorError;

pub(super) fn parse<'a>(
    value: &'a str,
    flag: &str,
    allowed: &[&str],
) -> Result<HashMap<&'a str, &'a str>, EmulatorError> {
    let mut settings = HashMap::new();
    for setting in value.split(',') {
        let (key, value) = setting.split_once('=').ok_or_else(|| {
            EmulatorError::Configuration(format!(
                "{flag} requires comma-separated key=value settings"
            ))
        })?;
        if !allowed.contains(&key) {
            return Err(EmulatorError::Configuration(format!(
                "unknown {flag} setting: {key}; choose {}",
                allowed.join(", ")
            )));
        }
        if value.is_empty() {
            return Err(EmulatorError::Configuration(format!(
                "{flag} {key} must not be empty"
            )));
        }
        if settings.insert(key, value).is_some() {
            return Err(EmulatorError::Configuration(format!(
                "{flag} {key} may only be supplied once"
            )));
        }
    }
    Ok(settings)
}
