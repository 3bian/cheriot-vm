// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::DebugArgs;
use std::path::Path;

pub(super) fn handle(mut d: DebugArgs, config: Option<&Path>) -> Result<u8> {
    if crate::help::no_arguments_after("debug") {
        return crate::help::print("debug");
    }
    d.exec.selection.jobs = d.exec.jobs;
    if let crate::exec::Plan::Command(command, directory) =
        crate::exec::apply_target(&mut d.exec, config, true)?
    {
        return crate::target::run(&command, &directory);
    }
    if d.dap {
        return super::dap::serve(d.exec, config);
    }
    if d.port == 0 {
        return Err(AppError::Message("debug port must be nonzero".into()));
    }
    let ip: std::net::IpAddr = d
        .host
        .parse()
        .map_err(|_| AppError::Message("--host must be a loopback IP address".into()))?;
    if !ip.is_loopback() {
        return Err(AppError::Message(
            "--host must be a loopback IP address".into(),
        ));
    }
    let mut request = crate::exec::machine_request(d.exec, config)?;
    request.debugger = Some(std::net::SocketAddr::new(ip, d.port));
    request.debugger_client = d.gdb;
    eprintln!("Starting GDB server on {ip}:{}", d.port);
    crate::exec::runtime::execute_request(request).map_err(|e| AppError::Message(e.to_string()))?;
    Ok(0)
}
