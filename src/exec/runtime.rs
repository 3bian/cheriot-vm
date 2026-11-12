// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::Write;

use crate::exec::machine::EmulatorError;
use crate::exec::machine::MachineConfiguration;
use crate::exec::machine::execution::TrapPolicy;
use crate::exec::machine::input::{MAX_FIRMWARE_BYTES, read_file};

use crate::exec::runtime::output::OutputPresentation;
use crate::exec::runtime::run_options::RunOptions;
use options::Options;

mod device_settings;
pub(crate) mod gpio_options;
mod host_bindings;
pub(crate) mod host_options;
pub(crate) mod i2c_options;
pub(crate) mod insights;
mod options;
mod output;
mod register;
mod register_selection;
pub mod request;
mod run;
mod run_options;
mod runtime_io;
pub(crate) mod spi_options;
pub(crate) mod tap_options;
pub(crate) mod uart;

pub fn execute_request(request: request::MachineRequest) -> Result<(), EmulatorError> {
    execute_options(request.validate()?)
}

fn execute_options(options: Options) -> Result<(), EmulatorError> {
    let Options {
        breakpoint,
        checkpoint,
        completion,
        firmware_is_elf,
        firmware_path,
        gdb,
        gdb_client,
        gdb_raw_output,
        host,
        memory_dump,
        output,
        profile_path,
        profile_text,
        raw_address,
        stats,
        registers,
        steps,
        trace,
        trap_policy,
    } = options;
    let profile = match profile_text {
        Some(text) => text.into_bytes(),
        None => read_file(
            profile_path,
            MachineConfiguration::MAX_PROFILE_BYTES,
            "profile",
        )?,
    };
    let profile = std::str::from_utf8(&profile)
        .map_err(|error| EmulatorError::Configuration(format!("profile is not UTF-8: {error}")))?;
    let mut configuration = MachineConfiguration::parse(profile)?;
    if let Some(selection) = &registers {
        selection.validate(
            configuration.is_cheriot(),
            configuration.processor_profile.embedded,
        )?;
    }
    if completion.is_some() {
        configuration.completion = completion;
    }
    let is_cheriot = configuration.is_cheriot();
    let trap_policy = trap_policy.unwrap_or(if is_cheriot {
        TrapPolicy::Vector
    } else {
        TrapPolicy::Stop
    });
    let presentation = output.unwrap_or(if is_cheriot {
        OutputPresentation::Compatibility
    } else {
        OutputPresentation::Labelled
    });
    let firmware = read_file(&firmware_path, MAX_FIRMWARE_BYTES, "firmware")?;
    #[cfg(all(feature = "gdb", unix))]
    let client =
        gdb_client
            .filter(|_| firmware_is_elf)
            .map(|executable| run::debugger::GdbClient {
                executable,
                firmware: firmware_path.into(),
            });
    let insights = match &stats {
        Some(_) if !firmware_is_elf => {
            return Err(EmulatorError::Configuration(
                "--stats needs an ELF image, which names the code sections".into(),
            ));
        }
        Some(_) => Some(std::rc::Rc::new(std::cell::RefCell::new(
            insights::Insights::from_elf(&firmware).ok_or_else(|| {
                EmulatorError::Configuration("--stats found no code sections in the ELF".into())
            })?,
        ))),
        None => None,
    };
    let collected = insights.clone();
    let run_options = |uart| RunOptions {
        insights,
        uart,
        breakpoint,
        memory_dump,
        presentation,
        registers,
        steps,
        trace,
        trap_policy,
    };
    if is_cheriot {
        let mut system = if firmware_is_elf {
            configuration.build_cheriot(&firmware)?
        } else {
            configuration.build_cheriot_raw(raw_address, &firmware)?
        };
        let uart = crate::exec::runtime::host_bindings::connect(
            &mut system.computer.bus,
            host,
            gdb.is_some(),
        )?;
        #[cfg(all(feature = "gdb", unix))]
        if let Some(address) = gdb {
            return run::debugger::run_cheriot(
                system,
                address,
                steps,
                gdb_raw_output,
                uart,
                client,
            );
        }
        let (system, _) = run::cheriot::run_with_options(system, run_options(uart))?;
        report_stats(collected, stats)?;
        if let Some(path) = checkpoint {
            let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
            system.write_checkpoint(&mut file)?;
            file.flush()?;
        }
        return Ok(());
    }
    let mut computer = if firmware_is_elf {
        configuration.build_elf(&firmware)?
    } else {
        configuration.build_raw(raw_address, &firmware)?
    };
    computer.trap_policy = trap_policy;
    let uart =
        crate::exec::runtime::host_bindings::connect(&mut computer.bus, host, gdb.is_some())?;
    #[cfg(all(feature = "gdb", unix))]
    if let Some(address) = gdb {
        return run::debugger::run_rv32(computer, address, steps, gdb_raw_output, uart, client);
    }
    #[cfg(not(all(feature = "gdb", unix)))]
    let _ = (gdb, gdb_client, gdb_raw_output);
    let (computer, _) = run::rv32::run_with_options(computer, run_options(uart))?;
    report_stats(collected, stats)?;
    if let Some(path) = checkpoint {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        computer.write_checkpoint(&mut file)?;
        file.flush()?;
    }
    Ok(())
}

fn report_stats(
    insights: Option<std::rc::Rc<std::cell::RefCell<insights::Insights>>>,
    destination: Option<Option<std::path::PathBuf>>,
) -> Result<(), EmulatorError> {
    let (Some(insights), Some(destination)) = (insights, destination) else {
        return Ok(());
    };
    let report = insights.borrow().report();
    match destination {
        Some(path) => {
            let json = serde_json::to_string_pretty(&report)
                .map_err(|error| EmulatorError::Configuration(error.to_string()))?;
            std::fs::write(path, json + "\n")?;
        }
        None => println!("\n{}", insights::text(&report)),
    }
    Ok(())
}

pub(crate) struct AdapterMachine {
    pub system: crate::exec::machine::cheriot::CheriotSystem,
    pub firmware: Vec<u8>,
    _uart: crate::exec::runtime::uart::connections::UartConnections,
}

pub(crate) fn adapter_machine(
    request: crate::exec::runtime::request::MachineRequest,
) -> Result<AdapterMachine, EmulatorError> {
    let options = request.validate()?;
    let profile = match options.profile_text {
        Some(text) => text.into_bytes(),
        None => read_file(
            options.profile_path,
            MachineConfiguration::MAX_PROFILE_BYTES,
            "profile",
        )?,
    };
    let profile = std::str::from_utf8(&profile)
        .map_err(|error| EmulatorError::Configuration(format!("profile is not UTF-8: {error}")))?;
    let mut configuration = MachineConfiguration::parse(profile)?;
    if options.completion.is_some() {
        configuration.completion = options.completion;
    }
    if !configuration.is_cheriot() || !options.firmware_is_elf {
        return Err(EmulatorError::Configuration(
            "the debug adapter needs a CHERIoT profile and an ELF image".into(),
        ));
    }
    let firmware = read_file(options.firmware_path, MAX_FIRMWARE_BYTES, "firmware")?;
    let mut system = configuration.build_cheriot(&firmware)?;
    let uart =
        crate::exec::runtime::host_bindings::connect(&mut system.computer.bus, options.host, true)?;
    system.computer.pause_host_clock();
    Ok(AdapterMachine {
        system,
        firmware,
        _uart: uart,
    })
}
