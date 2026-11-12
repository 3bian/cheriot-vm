// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{Report, TOOLS, Tool};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) fn executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn file_name(tool: &str) -> String {
    format!("{tool}{}", std::env::consts::EXE_SUFFIX)
}

pub(crate) fn search_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|directory| directory.is_absolute())
        .find(|directory| {
            TOOLS
                .iter()
                .all(|tool| executable(&directory.join(file_name(tool))))
        })
}

pub(crate) fn default_directory() -> Result<Option<(PathBuf, &'static str)>, String> {
    if let Some(directory) = crate::global::value("toolchain_dir")? {
        return Ok(Some((
            PathBuf::from(directory),
            "toolchain_dir in the global configuration",
        )));
    }
    Ok(search_path().map(|directory| (directory, "found on PATH")))
}

pub(crate) const HOW_TO_SELECT: &str = "\n\nName the directory that holds the CHERIoT LLVM tools in one of these ways:\n  --toolchain-dir DIR                            for this command\n  CHERIOT_TOOLCHAIN_DIR=DIR                      in the environment\n  toolchain_dir = \"DIR\"                          in cheriot.toml\n  cheriot config --global set toolchain_dir=DIR  for every project";

pub(super) fn inspect(directory: Option<&Path>, source: &str) -> Report {
    let tools: Vec<Tool> = TOOLS
        .iter()
        .map(|name| {
            let path = directory
                .map(|directory| directory.join(file_name(name)))
                .filter(|path| executable(path));
            let (version, error) = match &path {
                Some(path) => match Command::new(path).arg("--version").output() {
                    Ok(output) if output.status.success() => (
                        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
                        None,
                    ),
                    Ok(output) => (None, Some(format!("--version failed: {}", output.status))),
                    Err(error) => (None, Some(error.to_string())),
                },
                None => (None, Some("not found".to_owned())),
            };
            Tool {
                name: (*name).into(),
                path,
                number: version.as_deref().and_then(version_number),
                version: version.map(|text| text.lines().next().unwrap_or_default().to_owned()),
                error,
            }
        })
        .collect();
    let (target_supported, target_diagnostic) = super::verify::probe(&tools);
    Report {
        directory: directory.map(Path::to_path_buf),
        source: source.to_owned(),
        tools,
        target_supported,
        target_diagnostic,
    }
}

fn version_number(output: &str) -> Option<String> {
    output
        .split(|character: char| character.is_whitespace() || "(),:".contains(character))
        .find(|word| {
            word.contains('.')
                && word.starts_with(|character: char| character.is_ascii_digit())
                && word
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
        })
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::version_number;

    #[test]
    fn version_numbers_are_found_in_tool_output() {
        for (output, expected) in [
            (
                "clang version 23.1.3 (https://example.org/llvm.git e2219e2)",
                Some("23.1.3"),
            ),
            ("LLD 23.1.3 (compatible with GNU linkers)", Some("23.1.3")),
            (
                "llvm-strip, compatible with GNU strip\nLLVM (http://llvm.org/):\n  LLVM version 18.1.3\n",
                Some("18.1.3"),
            ),
            ("no number here", None),
        ] {
            assert_eq!(version_number(output).as_deref(), expected, "{output}");
        }
    }
}
