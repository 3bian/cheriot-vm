// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![forbid(unsafe_code)]

pub mod bus;
pub mod checkpoint;
pub mod cheriot1;
pub mod completion;
pub mod configuration;
#[cfg(feature = "gdb")]
pub mod debugger;
pub mod devices;
pub mod error;
pub mod execution;
pub mod firmware;
pub mod host;
pub mod input;
pub mod instructions;
pub mod memory;
pub mod output;
pub mod processor;
pub mod rv32;
pub mod timing_policy;

pub use configuration::MachineConfiguration;
pub use error::{EmulatorError, ExceptionCause};
pub use rv32::Computer;
pub use timing_policy::TimingPolicy;
