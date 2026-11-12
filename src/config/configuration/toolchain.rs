// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::diagnostic::{Diagnostic, Origin};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Toolchain {
    pub directory: PathBuf,
    pub directory_origin: Origin,
    pub c_compiler: String,
    pub cpp_compiler: String,
    pub linker: String,
    pub object_dump: String,
    pub object_copy: String,
    pub strip: String,
    pub uf2_converter: String,
    pub uf2_converter_origin: Origin,
}

impl Toolchain {
    pub fn uf2_error(&self, executable: &str) -> Diagnostic {
        Diagnostic::new(format!(
            "missing UF2 converter; expected {executable} (names use the action's PATH). Set toolchain.uf2_converter to the desired executable"
        )).at(self.uf2_converter_origin.clone())
    }

    pub fn source(&self) -> String {
        let origin = &self.directory_origin;
        match origin.source {
            crate::config::diagnostic::OriginSource::File => {
                format!("toolchain_dir in {}", origin.file)
            }
            _ => origin.rule.clone(),
        }
    }

    pub fn located(&self) -> bool {
        !self.directory.as_os_str().is_empty()
    }

    pub fn llvm_error(&self, executable: &str) -> Option<Diagnostic> {
        let (name, _) = [
            ("clang", &self.c_compiler),
            ("clang++", &self.cpp_compiler),
            ("ld.lld", &self.linker),
            ("llvm-objdump", &self.object_dump),
            ("llvm-objcopy", &self.object_copy),
            ("llvm-strip", &self.strip),
        ]
        .into_iter()
        .find(|(_, value)| value.as_str() == executable)?;
        let how = crate::toolchain::HOW_TO_SELECT;
        let message = if self.located() {
            format!(
                "missing or non-executable tool {name}; expected {executable} ({}).{how}",
                self.source()
            )
        } else {
            format!(
                "the CHERIoT LLVM tools were not found: no directory on PATH holds all of {}.{how}",
                crate::toolchain::TOOLS.join(", ")
            )
        };
        Some(Diagnostic::new(message).at(self.directory_origin.clone()))
    }
}
