// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::{collections::BTreeMap, path::Path};

use super::{RawToolchain, Toolchain};
use crate::config::diagnostic::{Diagnostic, Origin, Result};

pub(super) fn resolve(
    raw: &RawToolchain,
    configured_directory: Option<&str>,
    root: &Path,
    sdk: &Path,
    origins: &mut BTreeMap<String, Origin>,
) -> Result<Toolchain> {
    let mut origin = origins
        .get("/toolchain_dir")
        .cloned()
        .unwrap_or_else(|| Origin::derived("not found"));
    let directory = match configured_directory {
        Some("") => return Err(Diagnostic::new("toolchain directory must not be empty").at(origin)),
        Some(value) => super::resolution::resolve_path_value(value, &origin, root, sdk)?,
        None => match crate::toolchain::default_directory().map_err(Diagnostic::new)? {
            Some((directory, source)) => {
                origin = Origin::derived(source);
                directory
            }
            None => std::path::PathBuf::new(),
        },
    };
    if directory.to_string_lossy().contains(['\0', '\n', '\r']) {
        return Err(Diagnostic::new("invalid toolchain directory").at(origin));
    }
    origins.insert("/toolchain_dir".into(), origin.clone());
    for field in [
        "c_compiler",
        "cpp_compiler",
        "linker",
        "object_dump",
        "object_copy",
        "strip",
    ] {
        origins.insert(format!("/toolchain/{field}"), origin.clone());
    }
    let executable = |name: &str| {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        if directory.as_os_str().is_empty() {
            file
        } else {
            directory.join(file).display().to_string()
        }
    };
    let exceptional = |field: &str, value: &str| -> Result<String> {
        let source = origins
            .get(&format!("/toolchain/{field}"))
            .cloned()
            .unwrap_or_else(|| Origin::derived("default exceptional tool"));
        if value.is_empty() || value.starts_with('-') || value.contains(['\0', '\n', '\r']) {
            return Err(Diagnostic::new("tool must be an executable name or path").at(source));
        }
        if value.contains('/') || value.contains("${") {
            Ok(
                super::resolution::resolve_path_value(value, &source, root, sdk)?
                    .display()
                    .to_string(),
            )
        } else {
            Ok(value.to_owned())
        }
    };
    let uf2_converter = exceptional(
        "uf2_converter",
        raw.uf2_converter.as_deref().unwrap_or("uf2conv"),
    )?;
    let uf2_converter_origin = origins
        .get("/toolchain/uf2_converter")
        .cloned()
        .unwrap_or_else(|| Origin::derived("UF2 converter on PATH"));
    Ok(Toolchain {
        c_compiler: executable("clang"),
        cpp_compiler: executable("clang++"),
        linker: executable("ld.lld"),
        object_dump: executable("llvm-objdump"),
        object_copy: executable("llvm-objcopy"),
        strip: executable("llvm-strip"),
        directory,
        directory_origin: origin,
        uf2_converter,
        uf2_converter_origin,
    })
}
