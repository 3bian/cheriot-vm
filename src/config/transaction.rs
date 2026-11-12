// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::file_lock::FileLock;
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Value};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
type Result<T> = std::result::Result<T, ConfigError>;

pub fn path_segments(key: &str) -> Vec<&str> {
    key.trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect()
}

pub fn segments(key: &str) -> Result<Vec<&str>> {
    let result = path_segments(key);
    if result.is_empty() {
        return Err(ConfigError::Invalid("empty configuration key".into()));
    }
    if result.iter().any(|part| {
        part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }) {
        return Err(ConfigError::Invalid(format!(
            "invalid configuration key {key:?}; separate path segments with '/'"
        )));
    }
    Ok(result)
}
#[derive(Debug)]
pub enum Edit {
    Set { key: String, value: Value },
    Reset { key: String },
}
pub fn parse_value(text: &str) -> Result<Value> {
    if let Ok(value) = text.parse::<Value>() {
        return Ok(value);
    }
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | ':'))
    {
        return Ok(Value::from(text));
    }
    Err(ConfigError::Invalid(
        "invalid value; use TOML syntax or a simple unquoted string".into(),
    ))
}
fn lock_path(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ConfigError::Invalid("configuration filename must be UTF-8".into()))?;
    Ok(path.with_file_name(format!(".{name}.cheriot.lock")))
}
fn open_lock(path: &Path) -> Result<FileLock> {
    let path = lock_path(path)?;
    FileLock::try_exclusive(&path).map_err(|e| {
        ConfigError::Invalid(format!("configuration is locked ({}): {e}", path.display()))
    })
}
fn regular_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(ConfigError::Invalid(format!(
            "configuration must be a regular file: {}",
            path.display()
        )));
    }
    Ok(meta)
}
#[cfg(unix)]
fn same_identity(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
#[cfg(not(unix))]
fn same_identity(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}
fn edit_document(doc: &mut DocumentMut, edit: &Edit) -> Result<()> {
    let (key, value) = match edit {
        Edit::Set { key, value } => (key, Some(value)),
        Edit::Reset { key } => (key, None),
    };
    let components = segments(key)?;
    let (last, parents) = components
        .split_last()
        .ok_or_else(|| ConfigError::Invalid("empty configuration key".into()))?;
    let mut table = doc.as_table_mut();
    for part in parents {
        let item = table.get_mut(part).ok_or_else(|| {
            ConfigError::Invalid(format!(
                "table {part:?} is not present in the local document"
            ))
        })?;
        table = item
            .as_table_mut()
            .ok_or_else(|| ConfigError::Invalid(format!("{part:?} is not a table")))?;
    }
    if let Some(value) = value {
        table.insert(last, Item::Value(value.clone()));
    } else if table.remove(last).is_none() {
        return Err(ConfigError::Invalid(format!(
            "no local override for {key:?}"
        )));
    }
    Ok(())
}
pub fn apply(
    path: &Path,
    edits: &[Edit],
    validate: impl FnOnce(&Path) -> std::result::Result<(), String>,
) -> Result<()> {
    let _lock = open_lock(path)?;
    let original_meta = regular_file(path)?;
    let original = fs::read(path)?;
    let text = std::str::from_utf8(&original)
        .map_err(|e| ConfigError::Invalid(format!("configuration is not UTF-8: {e}")))?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::Invalid(format!("invalid TOML: {e}")))?;
    for edit in edits {
        edit_document(&mut document, edit)?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::Invalid("configuration has no parent".into()))?;
    let mut staged = tempfile::Builder::new()
        .prefix(".cheriot-edit-")
        .suffix(".toml")
        .tempfile_in(parent)?;
    staged
        .as_file()
        .set_permissions(original_meta.permissions())?;
    staged.write_all(document.to_string().as_bytes())?;
    staged.as_file_mut().sync_all()?;
    validate(staged.path()).map_err(|error| {
        ConfigError::Validation(error.replace(
            &staged.path().display().to_string(),
            &path.display().to_string(),
        ))
    })?;
    if !same_identity(&original_meta, &regular_file(path)?) || fs::read(path)? != original {
        return Err(ConfigError::Invalid(
            "configuration changed during edit; retry".into(),
        ));
    }
    test_barrier("before-commit");
    staged.persist(path).map_err(|e| ConfigError::Io(e.error))?;
    test_barrier("after-commit");
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(test))]
fn test_barrier(_: &str) {}
#[cfg(test)]
fn test_barrier(phase: &str) {
    if std::env::var("CHERIOT_TEST_CONFIG_BARRIER").ok().as_deref() == Some(phase) {
        println!("CHERIOT_CONFIG_BARRIER {phase}");
        let _ = std::io::stdout().flush();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    const MODULE: &str = {
        let path = module_path!().as_bytes();
        let mut start = 0;
        while path[start] != b':' {
            start += 1;
        }
        match str::from_utf8(path.split_at(start + 2).1) {
            Ok(rest) => rest,
            Err(_) => panic!("module path"),
        }
    };
    use super::*;
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    fn validate(path: &Path) -> std::result::Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let document = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;
        match document["mode"].as_str() {
            Some("debug" | "release") => Ok(()),
            _ => Err("invalid mode".into()),
        }
    }
    #[test]
    fn commit_child() {
        let Some(path) = std::env::var_os("CHERIOT_TEST_CONFIG_FILE") else {
            return;
        };
        apply(
            Path::new(&path),
            &[Edit::Set {
                key: "mode".into(),
                value: Value::from("debug"),
            }],
            validate,
        )
        .unwrap();
    }
    fn kill_at(file: &Path, phase: &str) {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{}::commit_child", MODULE),
                "--nocapture",
            ])
            .env("CHERIOT_TEST_CONFIG_FILE", file)
            .env("CHERIOT_TEST_CONFIG_BARRIER", phase)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        let mut ready = false;
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("CHERIOT_CONFIG_BARRIER") {
                ready = true;
                break;
            }
            line.clear();
        }
        if !ready {
            let _ = child.kill();
            let _ = child.wait();
            panic!("configuration child did not reach {phase}");
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    #[test]
    fn killed_writer_leaves_valid_old_or_new_document() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("project.toml");
        std::fs::write(&file, "mode = \"release\"\n").unwrap();
        let old = fs::read(&file).unwrap();
        kill_at(&file, "before-commit");
        assert_eq!(fs::read(&file).unwrap(), old);
        validate(&file).unwrap();
        kill_at(&file, "after-commit");
        let doc = fs::read_to_string(&file)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(doc["mode"].as_str(), Some("debug"));
        validate(&file).unwrap();
    }
}
