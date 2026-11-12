// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{ls, read_project, selected};
use crate::app_error::{AppError, Result};
use crate::cli::Format;
use crate::config::transaction;
use std::path::Path;

pub(super) fn handle(
    global: Option<&Path>,
    project: Option<String>,
    key: String,
    format: Format,
) -> Result<u8> {
    let segments = transaction::path_segments(&key);
    if segments.is_empty() {
        return Err(AppError::Message("empty configuration key".into()));
    }
    let written = ls::read_document(global, project.as_deref())?;
    let value = match ls::find(&written, &key) {
        Some(node) => node.to_json(),
        None => {
            let resolved = serde_json::to_value(read_project(&selected(global, project)?)?)?;
            let mut value = &resolved;
            for segment in &segments {
                value = match value {
                    serde_json::Value::Array(items) => segment
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| items.get(index)),
                    other => other.get(segment),
                }
                .ok_or_else(|| AppError::Message(format!("unknown configuration key {key:?}")))?;
            }
            value.clone()
        }
    };
    let name = segments.last().copied().unwrap_or("value");
    match crate::output::structured(&value, &format, name)? {
        Some(text) => println!("{text}"),
        None => print_text(&value, name)?,
    }
    Ok(0)
}

fn scalar(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Null => Some(String::new()),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => Some(value.to_string()),
        _ => None,
    }
}

fn print_text(value: &serde_json::Value, name: &str) -> Result<()> {
    if let Some(text) = scalar(value) {
        println!("{text}");
        return Ok(());
    }
    if let serde_json::Value::Array(items) = value
        && let Some(lines) = items.iter().map(scalar).collect::<Option<Vec<_>>>()
    {
        for line in lines {
            println!("{line}");
        }
        return Ok(());
    }
    let document = match value {
        serde_json::Value::Object(_) => value.clone(),
        other => serde_json::json!({ name: other }),
    };
    let text =
        toml::to_string_pretty(&document).map_err(|error| AppError::Message(error.to_string()))?;
    print!("{text}");
    Ok(())
}
