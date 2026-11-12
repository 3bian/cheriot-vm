// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) mod machine;
pub(crate) mod runtime;

use crate::app_error::{AppError, Result, require_utf8_path};
use crate::build::selection;
use crate::build::workflow::{self, request::Action};
use crate::cli::{ExecArgs, Selection};
use crate::exec::machine::completion::CompletionPolicy;
use crate::exec::machine::execution::TrapPolicy;
use crate::exec::runtime::request::{
    self, AddressKind, Firmware, MachineRequest, PeripheralSettings, Presentation, RegisterDump,
};
use std::path::{Path, PathBuf};
fn invalid(e: impl std::fmt::Display) -> AppError {
    AppError::Message(e.to_string())
}

fn selects_project(s: &Selection) -> bool {
    s.board.is_some()
        || s.build_dir.is_some()
        || s.firmware.is_some()
        || s.jobs.is_some()
        || s.mode.is_some()
        || s.output_dir.is_some()
        || !s.overlay.is_empty()
        || s.toolchain_dir.is_some()
}

pub(crate) fn machine_request(e: ExecArgs, config: Option<&Path>) -> Result<MachineRequest> {
    let project_file = config.map_or_else(|| PathBuf::from("cheriot.toml"), Path::to_path_buf);
    let project_file = match &e.selection.project {
        Some(name) => crate::config::configuration::select_project(&project_file, Some(name))
            .unwrap_or(project_file),
        None => project_file,
    };
    let selected = crate::config::profiles::select(&project_file, e.profile.as_deref())
        .map_err(AppError::Message)?;
    let profile = selected.file;
    require_utf8_path(&profile)?;
    for path in [e.elf.as_deref(), e.raw.as_deref(), e.checkpoint.as_deref()]
        .into_iter()
        .flatten()
    {
        require_utf8_path(path)?;
    }
    let from_project = e.build || (e.elf.is_none() && e.raw.is_none());
    if !from_project && selects_project(&e.selection) {
        return Err(AppError::Message(
            "project options need --build, or omit the ELF to run the project's firmware".into(),
        ));
    }
    let project_elf = if from_project {
        let request = selection::request(&e.selection, config, Action::Build)?;
        let path = if e.build {
            workflow::build_firmware(request)
        } else {
            workflow::published_firmware(request)
        }
        .map_err(invalid)?;
        require_utf8_path(&path)?;
        if e.elf.is_none() && !std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            return Err(AppError::Message(format!(
                "firmware not built: {}; run `cheriot build` or pass --build",
                path.display()
            )));
        }
        Some(path)
    } else {
        None
    };
    let elf = e.elf.or(project_elf);
    let firmware = match (elf, e.raw, e.address) {
        (Some(path), None, None) => Firmware::Elf(path),
        (None, Some(path), Some(address)) => Firmware::Raw {
            path,
            address: request::address(&address, AddressKind::Raw).map_err(invalid)?,
        },
        _ => {
            return Err(AppError::Message(
                "select ELF or raw firmware and its address".into(),
            ));
        }
    };
    let mut request = MachineRequest::new(firmware, profile);
    request.profile_text = selected.text;
    request.breakpoints = e
        .breakpoint
        .iter()
        .map(|v| request::address(v, AddressKind::Breakpoint).map_err(invalid))
        .collect::<Result<_>>()?;
    request.checkpoint = e.checkpoint;
    request.completion = e
        .completion
        .map(|v| match v.as_str() {
            "disabled" => Ok(CompletionPolicy::Disabled),
            "elf-symbol" => Ok(CompletionPolicy::ElfSymbol),
            _ => request::address(&v, AddressKind::Completion)
                .map(CompletionPolicy::Address)
                .map_err(invalid),
        })
        .transpose()?;
    request.peripherals = PeripheralSettings {
        gpio: e.gpio,
        i2c: e.i2c,
        spi: e.spi,
        tap: e.tap,
        uart: e.uart,
    };
    request.memory = e.memory;
    request.registers = e.registers.map(RegisterDump::Selection);
    request.steps = e.steps;
    request.stats = match (e.stats, e.stats_output) {
        (_, Some(file)) => Some(Some(file)),
        (true, None) => Some(None),
        (false, None) => None,
    };
    request.trace = e.trace;
    request.output = e
        .console_format
        .map(|s| match s.as_str() {
            "raw" => Ok(Presentation::Raw),
            "labelled" => Ok(Presentation::Labelled),
            "escaped" => Ok(Presentation::Escaped),
            _ => Err(AppError::Message("invalid console format".into())),
        })
        .transpose()?;
    request.trap_policy = e
        .trap_policy
        .map(|s| match s.as_str() {
            "stop" => Ok(TrapPolicy::Stop),
            "vector" => Ok(TrapPolicy::Vector),
            _ => Err(AppError::Message("invalid trap policy".into())),
        })
        .transpose()?;
    Ok(request)
}

pub(crate) enum Plan {
    Emulator,
    Command(Vec<String>, PathBuf),
}

pub(crate) fn apply_target(e: &mut ExecArgs, config: Option<&Path>, debug: bool) -> Result<Plan> {
    let project_file = config.map_or_else(|| PathBuf::from("cheriot.toml"), Path::to_path_buf);
    let name = match (&e.target, &e.profile) {
        (Some(name), _) => name.clone(),
        (None, None) => match crate::target::default_name(&project_file) {
            Some(name) => name,
            None => return Ok(Plan::Emulator),
        },
        (None, Some(_)) => return Ok(Plan::Emulator),
    };
    let section = if debug { "debug" } else { "exec" };
    let standalone = !project_file.is_file() && e.elf.is_some();
    let target = if standalone {
        crate::target::find(&name, &project_file, None)?
    } else {
        crate::target::resolve(&e.selection, config, Some(&name))?.0
    };
    let action = if debug {
        target.definition.debug.clone()
    } else {
        target.definition.exec.clone()
    }
    .ok_or_else(|| {
        AppError::Message(format!(
            "target {} does not define [{section}]",
            target.name
        ))
    })?;
    let Some(command) = action.command else {
        if e.profile.is_none()
            && let Some(profile) = &action.profile
        {
            e.profile = Some(crate::target::profile_path(&target, profile));
        }
        return Ok(Plan::Emulator);
    };
    let emulator_only = !e.breakpoint.is_empty()
        || e.checkpoint.is_some()
        || e.completion.is_some()
        || e.console_format.is_some()
        || !e.gpio.is_empty()
        || !e.i2c.is_empty()
        || e.memory.is_some()
        || e.profile.is_some()
        || e.raw.is_some()
        || e.registers.is_some()
        || !e.spi.is_empty()
        || e.stats
        || e.stats_output.is_some()
        || e.steps.is_some()
        || !e.tap.is_empty()
        || e.trace
        || e.trap_policy.is_some()
        || !e.uart.is_empty();
    if emulator_only {
        return Err(AppError::Message(format!(
            "target {} runs a command, so emulator options such as --profile, --steps and --stats do not apply",
            target.name
        )));
    }
    let mut prepared = match (&e.elf, standalone) {
        (Some(elf), true) => crate::target::Prepared::standalone(target, elf)?,
        _ => crate::target::prepare(&e.selection, config, Some(&name), e.build, e.elf.clone())?,
    };
    if command
        .iter()
        .any(|argument| argument.contains("${package}"))
    {
        prepared.package(&e.selection, config)?;
    }
    Ok(Plan::Command(
        prepared.context.expand_command(&command)?,
        prepared.context.project_dir.clone(),
    ))
}

pub(crate) fn handle(mut e: ExecArgs, config: Option<&Path>) -> Result<u8> {
    if crate::help::no_arguments_after("exec") {
        return crate::help::print("exec");
    }
    e.selection.jobs = e.jobs;
    if let Plan::Command(command, directory) = apply_target(&mut e, config, false)? {
        return crate::target::run(&command, &directory);
    }
    let request = machine_request(e, config)?;
    crate::exec::runtime::execute_request(request).map_err(invalid)?;
    Ok(0)
}
