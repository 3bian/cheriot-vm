// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub file: Option<PathBuf>,
    pub project: Option<String>,
    pub firmware: Option<String>,
    pub board: Option<String>,
    pub mode: Option<String>,
    pub toolchain_dir: Option<PathBuf>,
    pub build_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub overlays: Vec<PathBuf>,
    pub jobs: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Build,
    Clean,
    Configuration,
    Plan,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Human,
    Text,
    Json,
    Toml,
    Shell,
    CompilationDatabase,
    Bytes,
}

#[derive(Debug)]
pub struct Request {
    pub action: Action,
    pub selection: Selection,
    pub format: Format,
    pub output: Option<PathBuf>,
    pub compartment: Option<String>,
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub probe_toolchain: bool,
    pub include_packaging: bool,
}

impl Request {
    pub fn new(action: Action, selection: Selection) -> Self {
        let format = match action {
            Action::Configuration => Format::Toml,
            Action::Plan | Action::Size => Format::Text,
            _ => Format::Human,
        };
        Self {
            action,
            selection,
            format,
            output: None,
            compartment: None,
            dry_run: false,
            verbose: false,
            quiet: false,
            probe_toolchain: false,
            include_packaging: false,
        }
    }
}
