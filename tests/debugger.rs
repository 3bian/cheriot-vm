// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[cfg(all(feature = "gdb", target_os = "linux"))]
#[path = "firmware/elf.rs"]
mod firmware_elf;
#[cfg(all(feature = "gdb", target_os = "linux"))]
#[path = "firmware/words.rs"]
mod firmware_words;

#[path = "debugger/cli.rs"]
mod cli;
#[path = "debugger/protocol.rs"]
mod protocol;
