// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[path = "firmware/elf.rs"]
mod firmware_elf;
#[cfg(feature = "gdb")]
#[path = "firmware/words.rs"]
mod firmware_words;

#[path = "rv32/debugger.rs"]
mod debugger;
#[path = "rv32/execution.rs"]
mod execution;
#[path = "rv32/guests.rs"]
mod guests;
#[path = "rv32/peripherals.rs"]
mod peripherals;
#[path = "rv32/registers.rs"]
mod registers;
#[path = "rv32/upstream.rs"]
mod upstream;
