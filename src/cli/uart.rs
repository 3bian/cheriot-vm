// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

pub(crate) mod connections;
pub(crate) mod options;
pub(crate) mod output_binding;

fn is_stdout_path(path: &str) -> bool {
    matches!(path, "/dev/stdout" | "/dev/fd/1" | "/proc/self/fd/1")
}
