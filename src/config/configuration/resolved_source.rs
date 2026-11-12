// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::PathBuf;

use serde::Serialize;

use super::{CompilerFlag, ForcedInclude};
use crate::config::diagnostic::Origin;

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedSource {
    pub path: PathBuf,
    pub compiler: Option<crate::config::diagnostic::Sourced<String>>,
    pub flags: Vec<CompilerFlag>,
    pub c_flags: Vec<CompilerFlag>,
    pub cpp_flags: Vec<CompilerFlag>,
    pub assembly_flags: Vec<CompilerFlag>,
    pub includes: Vec<crate::config::diagnostic::Sourced<PathBuf>>,
    pub force_includes: Vec<ForcedInclude>,
    pub rtos_git_description: Option<Origin>,
}

impl ResolvedSource {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            compiler: None,
            flags: Vec::new(),
            c_flags: Vec::new(),
            cpp_flags: Vec::new(),
            assembly_flags: Vec::new(),
            includes: Vec::new(),
            force_includes: Vec::new(),
            rtos_git_description: None,
        }
    }
}
