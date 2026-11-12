// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{read_project, selected};
use crate::app_error::{AppError, Result};
use crate::config::transaction::{self, Edit};
use std::path::Path;

pub(super) fn handle(
    global: Option<&Path>,
    project: Option<String>,
    keys: Vec<String>,
) -> Result<u8> {
    let mut edits = Vec::new();
    for key in keys {
        transaction::segments(&key).map_err(|e| AppError::Message(e.to_string()))?;
        edits.push(Edit::Reset { key });
    }
    let path = selected(global, project)?;
    transaction::apply(&path, &edits, |file| {
        read_project(file).map(|_| ()).map_err(|e| e.to_string())
    })
    .map_err(|e| AppError::Message(e.to_string()))?;
    println!("Updated {}", crate::output::path(&path));
    Ok(0)
}
