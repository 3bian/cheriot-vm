// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[cfg(target_os = "linux")]
use std::io;

use crate::exec::machine::devices::ethernet::FrameTransport;
use crate::exec::machine::error::EmulatorError;

pub fn open(interface: &str) -> Result<Box<dyn FrameTransport>, EmulatorError> {
    if interface.is_empty()
        || interface.len() > 15
        || matches!(interface, "." | "..")
        || !interface
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    {
        return Err(EmulatorError::Configuration(
            "TAP interface names must be 1..15 ASCII letters, digits, underscores, \
             hyphens or dots; '.' and '..' are invalid"
                .into(),
        ));
    }

    #[cfg(target_os = "linux")]
    {
        nix::net::if_::if_nametoindex(interface).map_err(|error| {
            EmulatorError::Configuration(format!(
                "cannot find TAP interface {interface}: {error}; \
                 create a TAP interface owned by your user first"
            ))
        })?;

        let mut configuration = tun::Configuration::default();
        configuration.tun_name(interface).layer(tun::Layer::L2);
        configuration.platform_config(|platform| {
            platform.ensure_root_privileges(false);
        });

        let device = tun::create(&configuration).map_err(|error| {
            EmulatorError::Configuration(format!(
                "cannot attach to TAP interface {interface}: {error}; \
                 check ownership and whether another process has it open"
            ))
        })?;
        device.set_nonblock().map_err(|error| {
            EmulatorError::Configuration(format!(
                "cannot make TAP interface {interface} nonblocking: {error}"
            ))
        })?;

        Ok(Box::new(device))
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(EmulatorError::Configuration(
            "a TAP interface is attached through the Linux TUN/TAP driver, which this system does not have".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
impl FrameTransport for tun::Device {
    fn receive(&mut self, frame: &mut [u8]) -> io::Result<usize> {
        self.recv(frame)
    }

    fn transmit(&mut self, frame: &[u8]) -> io::Result<usize> {
        self.send(frame)
    }
}
