// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

mod capability;
mod capability_fault_cause;
#[cfg(test)]
#[path = "../tests/cheriot/compressed_reference.rs"]
mod compressed_reference_tests;
mod computer;
#[cfg(test)]
#[path = "../tests/cheriot/csr_reference.rs"]
mod csr_reference_tests;
#[cfg(feature = "gdb")]
pub mod debugger;
mod decoded_instruction;
mod fault;
mod host_interface;
mod instructions;
#[cfg(feature = "verification")]
mod memory_access;
mod operation;
mod processor;
#[cfg(test)]
#[path = "../tests/cheriot/reference_input.rs"]
mod reference_input;
#[cfg(feature = "verification")]
mod register_access;

#[cfg(test)]
#[path = "../tests/cheriot/reference.rs"]
mod reference_tests;
mod step_outcome;
mod system;

pub use capability::{permissions, Capability};
pub use capability_fault_cause::CapabilityFaultCause;
pub use computer::Computer;
pub use decoded_instruction::DecodedInstruction;
pub use fault::Fault;
pub use host_interface::HostInterface;
#[cfg(feature = "verification")]
pub use memory_access::MemoryAccess;
pub use operation::Operation;
pub use processor::Processor;
#[cfg(feature = "verification")]
pub use register_access::RegisterAccess;
pub use step_outcome::StepOutcome;
pub use system::CheriotSystem;

pub use instructions::{decode_compressed, decode_standard};
