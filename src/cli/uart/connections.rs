// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::HashSet;
use std::path::Path;

use cheriot_vm::bus::Bus;
use cheriot_vm::devices::uart::Uart;
use cheriot_vm::host::device_selection;
use cheriot_vm::host::uart::input::UartInput;
use cheriot_vm::host::uart::port::UartPort;
use cheriot_vm::EmulatorError;

use crate::cli::uart::options::UartOptions;
use crate::cli::uart::output_binding::UartOutputBinding;

#[derive(Default)]
pub(crate) struct UartConnections {
    pub inputs: Vec<UartInput>,
    pub outputs: Vec<UartOutputBinding>,
    ports: Vec<UartPort>,
}

impl UartConnections {
    pub(crate) fn open(
        bus: &Bus,
        options: Vec<UartOptions>,
        debugger: bool,
    ) -> Result<Self, EmulatorError> {
        let mut devices = HashSet::new();
        let mut inputs = HashSet::new();
        let mut outputs = HashSet::new();
        let mut bindings = Vec::new();
        for option in options {
            let device = device_selection::resolve::<Uart>(bus, option.device.as_deref(), "UART")?;
            if !devices.insert(device.clone()) {
                return Err(EmulatorError::Configuration(format!(
                    "duplicate UART binding for {device}"
                )));
            }
            if option
                .input
                .as_ref()
                .is_some_and(|path| !inputs.insert(path.clone()))
            {
                return Err(EmulatorError::Configuration(
                    "a host UART input path may only be bound once".into(),
                ));
            }
            if option.output.as_ref().is_some_and(|path| {
                !matches!(
                    path.as_str(),
                    "/dev/stdout" | "/dev/fd/1" | "/proc/self/fd/1"
                ) && !outputs.insert(path.clone())
            }) {
                return Err(EmulatorError::Configuration("a host UART output path may only be bound once; use stdout for combined output".into()));
            }
            bindings.push((device, option));
        }
        let mut connections = Self::default();
        let mut configured_ports = HashSet::new();
        for (_, option) in &bindings {
            if let Some(settings) = &option.settings {
                #[cfg(target_os = "linux")]
                {
                    use std::os::unix::fs::{FileTypeExt, MetadataExt};
                    let mut ports = HashSet::new();
                    for path in option.input.iter().chain(option.output.iter()) {
                        if matches!(
                            path.as_str(),
                            "/dev/stdin"
                                | "/dev/stdout"
                                | "/dev/fd/0"
                                | "/dev/fd/1"
                                | "/proc/self/fd/0"
                                | "/proc/self/fd/1"
                        ) {
                            continue;
                        }
                        if let Ok(metadata) = std::fs::metadata(path) {
                            if metadata.file_type().is_char_device()
                                && ports.insert(metadata.rdev())
                            {
                                if !configured_ports.insert(metadata.rdev()) {
                                    return Err(EmulatorError::Configuration(
                                        "a host UART port may only be configured by one binding"
                                            .into(),
                                    ));
                                }
                                connections
                                    .ports
                                    .push(UartPort::open(Path::new(path), settings)?);
                            }
                        }
                    }
                    if ports.is_empty() {
                        return Err(EmulatorError::Configuration("UART baud/framing/flow settings require a host TTY path; stdin/stdout and files keep their existing settings".into()));
                    }
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = settings;
                    return Err(EmulatorError::Configuration(
                        "host UART settings require Linux".into(),
                    ));
                }
            }
        }
        for (device, option) in bindings {
            if let Some(path) = option.input {
                connections
                    .inputs
                    .push(UartInput::open(bus, &device, Path::new(&path))?);
            }
            if let Some(path) = option.output {
                if let Some(output) = UartOutputBinding::open(device, &path, debugger)? {
                    connections.outputs.push(output);
                }
            }
        }
        Ok(connections)
    }
    pub(crate) fn restore(&mut self) -> Result<(), EmulatorError> {
        let mut result = Ok(());
        for port in &mut self.ports {
            let restored = port.restore();
            if result.is_ok() {
                result = restored;
            }
        }
        result
    }
}
