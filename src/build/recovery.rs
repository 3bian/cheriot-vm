// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::publication::{
    claim, managed_entries, mark_committed, remove_generation, selected_generation, sync_dir,
    validate_generation,
};
use crate::build::build_graph::Publication;
use crate::config::diagnostic::{Diagnostic, Result};
use std::{fs, path::Path};

pub(super) fn recover_incomplete(p: &Publication) -> Result<()> {
    let selected = selected_generation(p)?;
    if let Some(current) = selected.as_ref() {
        mark_committed(p, current)?;
    }
    for dir in managed_entries(p)? {
        if selected.as_ref() == Some(&dir) {
            continue;
        }
        if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let path = dir.join(".claim");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
            && fs::read_to_string(&path).ok().as_deref() == Some(claim(p).as_str())
            && !dir.join(".committed").exists()
        {
            if dir.join(".owner").exists() {
                validate_generation(p, &dir)?;
            }
            let pointer_prefix = format!(".{}.cheriot-pointer-", p.firmware_name);
            for entry in
                fs::read_dir(&p.output_dir).map_err(|e| Diagnostic::io(&p.output_dir, e))?
            {
                let entry = entry.map_err(|e| Diagnostic::io(&p.output_dir, e))?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with(&pointer_prefix))
                    && entry
                        .file_type()
                        .map_err(|e| Diagnostic::io(&entry.path(), e))?
                        .is_symlink()
                    && fs::read_link(entry.path()).ok().as_deref() == dir.file_name().map(Path::new)
                {
                    fs::remove_file(entry.path()).map_err(|e| Diagnostic::io(&entry.path(), e))?;
                }
            }
            remove_generation(p, &dir)?;
        }
    }
    sync_dir(&p.output_dir)?;
    Ok(())
}
