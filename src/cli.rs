// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::env;
use std::io::Write;

use cheriot_vm::execution::TrapPolicy;
use cheriot_vm::input::{read_file, MAX_FIRMWARE_BYTES};
use cheriot_vm::{EmulatorError, MachineConfiguration};

use crate::cli::output::OutputPresentation;
use crate::cli::run_options::RunOptions;
use options::Options;

mod device_settings;
pub(crate) mod gpio_options;
mod host_bindings;
pub(crate) mod host_options;
pub(crate) mod i2c_options;
mod options;
mod output;
mod register;
mod register_selection;
mod run;
mod run_options;
mod runtime_io;
pub(crate) mod spi_options;
pub(crate) mod tap_options;
pub(crate) mod uart;

pub(crate) fn execute_cli() -> Result<(), EmulatorError> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
    {
        usage()?;
        return Ok(());
    }
    let Options {
        breakpoint,
        checkpoint,
        completion,
        firmware_is_elf,
        firmware_path,
        gdb,
        gdb_raw_output,
        host,
        memory_dump,
        output,
        profile_path,
        raw_address,
        registers,
        steps,
        trace,
        trap_policy,
    } = Options::parse(&arguments)?;
    let profile = read_file(
        profile_path,
        MachineConfiguration::MAX_PROFILE_BYTES,
        "profile",
    )?;
    let profile = std::str::from_utf8(&profile)
        .map_err(|error| EmulatorError::Configuration(format!("profile is not UTF-8: {error}")))?;
    let mut configuration = MachineConfiguration::parse(profile)?;
    if let Some(selection) = &registers {
        selection.validate(
            configuration.is_cheriot1(),
            configuration.processor_profile.embedded,
        )?;
    }
    if completion.is_some() {
        configuration.completion = completion;
    }
    let firmware = read_file(firmware_path, MAX_FIRMWARE_BYTES, "firmware")?;
    if configuration.is_cheriot1() {
        let mut system = if firmware_is_elf {
            configuration.build_cheriot1(&firmware)?
        } else {
            configuration.build_cheriot1_raw(raw_address, &firmware)?
        };
        let uart =
            crate::cli::host_bindings::connect(&mut system.computer.bus, host, gdb.is_some())?;
        #[cfg(all(feature = "gdb", target_os = "linux"))]
        if let Some(address) = gdb {
            return run::debugger::run_cheriot1(system, address, steps, gdb_raw_output, uart);
        }
        let (system, _) = run::cheriot1::run_with_options(
            system,
            RunOptions {
                uart,
                breakpoint,
                memory_dump,
                presentation: output.unwrap_or(OutputPresentation::Compatibility),
                registers,
                steps,
                trace,
                trap_policy: trap_policy.unwrap_or(TrapPolicy::Vector),
            },
        )?;
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
    computer.trap_policy = trap_policy.unwrap_or(TrapPolicy::Stop);
    let uart = crate::cli::host_bindings::connect(&mut computer.bus, host, gdb.is_some())?;
    #[cfg(all(feature = "gdb", target_os = "linux"))]
    if let Some(address) = gdb {
        return run::debugger::run_rv32(computer, address, steps, gdb_raw_output, uart);
    }
    #[cfg(not(all(feature = "gdb", target_os = "linux")))]
    let _ = (gdb, gdb_raw_output);
    let (computer, _) = run::rv32::run_with_options(
        computer,
        RunOptions {
            uart,
            steps,
            breakpoint,
            trace,
            registers,
            memory_dump,
            presentation: output.unwrap_or(OutputPresentation::Labelled),
            trap_policy: trap_policy.unwrap_or(TrapPolicy::Stop),
        },
    )?;
    if let Some(path) = checkpoint {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        computer.write_checkpoint(&mut file)?;
        file.flush()?;
    }
    Ok(())
}

fn usage() -> Result<(), EmulatorError> {
    const HELP: &str = r#"Usage: cheriot-vm --profile FILE --elf FILE [OPTION]...
   or: cheriot-vm --profile FILE --raw FILE [OPTION]...

Run CHERIoT and RV32 RISC-V firmware with simulated memory and devices.

Options:
  --address ADDRESS
    Load a raw binary at this address (default: 0).

  --breakpoint ADDRESS
    Stop before executing at this address.

  --checkpoint FILE
    Write an architecture-aware comparison snapshot.

  --completion disabled|elf-symbol|ADDRESS
    Select the optional HTIF observer.

  --elf FILE
    Load an ELF32 program.

  --gdb [KEY=VALUE[,KEY=VALUE]...]
    Start halted for GDB (requires the gdb feature).
    Optional settings:
      ip=IP (loopback only; default: 127.0.0.1)
      port=PORT (default: 333)
      output=escaped|raw (default: escaped)

  --gpio [device=NAME,]path=PATH,lines=N[+N]...
    Connect GPIO to Linux chip line offsets.
    Optional settings:
      active-low=true|false (default: false)
      bias=as-is|disabled|pull-up|pull-down (default: as-is)
      drive=push-pull|open-drain|open-source (default: push-pull)

  --i2c [device=NAME,]path=PATH
    Connect I2C to a Linux adapter (host sets clock).
    Optional settings:
      addresses=N[+N]... (unique 0x08..0x77; default: all)

  --memory address=ADDRESS[,length=BYTES]
    Inspect 1-256 RAM/ROM bytes on stop (default: 16).

  --output raw|labelled|escaped
    Select guest-output presentation.

  --profile FILE
    Select a TOML machine profile.

  --raw FILE
    Load a raw binary.

  --registers [names=REGISTER[+REGISTER]...]
    Show registers on stop; all if names are omitted.

  --spi [device=NAME,]path=PATH
    Connect SPI to Linux spidev.
    Optional settings:
      select=N (0..3; default: 0)
      speed=HZ (1..50000000; default: 500000)

  --steps NUMBER
    Limit attempted steps; 0 is unlimited.

  --tap [device=NAME,]interface=NAME
    Connect Ethernet to an existing Linux TAP interface.

  --trace
    Print instructions and traps.

  --trap-policy stop|vector
    Stop on a trap or enter the guest handler.

  --uart [device=NAME,]in=PATH[,out=PATH]
  --uart [device=NAME,]out=PATH
    Connect UART input, output or both.
    At least one of in or out is required.
    Optional host TTY settings:
      baud=RATE (default: keep current rate)
      data-bits=5|6|7|8 (default: 8)
      flow=none|rtscts|xonxoff (default: none)
      parity=none|even|odd (default: none)
      stop-bits=1|2 (default: 1)

  -h, --help
    Display this help and exit.
"#;
    std::io::stdout().lock().write_all(HELP.as_bytes())?;
    Ok(())
}
