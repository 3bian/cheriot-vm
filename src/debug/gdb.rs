// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

mod breakpoint_type;
mod run_control;
mod run_state;
mod session_target;
mod stop_reason;

pub use super::inspection::inspect_memory;
pub use breakpoint_type::BreakpointType;
pub use run_control::RunControl;
pub use run_state::RunState;
pub use session_target::SessionTarget;
pub use stop_reason::StopReason;

pub(crate) fn read_register(bytes: &[Option<u8>], output: &mut [u8]) -> Option<usize> {
    if output.len() < bytes.len() {
        return None;
    }
    if bytes.iter().any(Option::is_none) {
        return Some(0);
    }
    for (destination, value) in output.iter_mut().zip(bytes) {
        *destination = (*value)?;
    }
    Some(bytes.len())
}

pub(crate) fn monitor_words(command: &[u8]) -> Result<Vec<&str>, &'static str> {
    if command.len() > 128 {
        return Err("monitor command too long");
    }
    let command = std::str::from_utf8(command).map_err(|_| "monitor requires UTF-8")?;
    Ok(command.split_whitespace().collect())
}

pub(crate) fn present_monitor(
    result: Result<String, &'static str>,
    mut output: gdbstub::target::ext::monitor_cmd::ConsoleOutput<'_>,
) {
    match result {
        Ok(text) => gdbstub::output!(output, "{}", text),
        Err(error) => gdbstub::outputln!(output, "error: {}", error),
    }
}
