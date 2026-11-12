// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) mod connections;
mod identity;
pub(crate) mod options;
pub(crate) mod output_binding;

fn is_stdout_path(path: &str) -> bool {
    matches!(path, "/dev/stdout" | "/dev/fd/1" | "/proc/self/fd/1")
}

fn is_stdin_path(path: &str) -> bool {
    matches!(path, "/dev/stdin" | "/dev/fd/0" | "/proc/self/fd/0")
}
