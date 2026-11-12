// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::Format;
use crate::output;
use object::{Object, ObjectSection, ObjectSymbol};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
struct Section {
    name: String,
    address: u64,
    size: u64,
    kind: String,
}
#[derive(Serialize)]
struct Symbol {
    name: String,
    address: u64,
    size: u64,
    kind: String,
}
#[derive(Serialize)]
struct ElfReport {
    path: String,
    architecture: String,
    entry: u64,
    sections: Vec<Section>,
    symbols: Vec<Symbol>,
}
pub(super) fn handle(
    file: &Path,
    format: Format,
    output: Option<&Path>,
    sections: bool,
    symbols: bool,
) -> Result<u8> {
    let data = fs::read(file)
        .map_err(|error| AppError::Message(format!("cannot read {}: {error}", file.display())))?;
    if !data.starts_with(b"\x7fELF") {
        return Err(AppError::Message(format!(
            "{} is not an ELF file",
            file.display()
        )));
    }
    let obj =
        object::File::parse(&*data).map_err(|e| AppError::Message(format!("invalid ELF: {e}")))?;
    let report = ElfReport {
        path: file.display().to_string(),
        architecture: format!("{:?}", obj.architecture()),
        entry: obj.entry(),
        sections: if sections {
            obj.sections()
                .map(|s| Section {
                    name: s.name().unwrap_or("<invalid>").into(),
                    address: s.address(),
                    size: s.size(),
                    kind: format!("{:?}", s.kind()),
                })
                .collect()
        } else {
            vec![]
        },
        symbols: if symbols {
            obj.symbols()
                .map(|s| Symbol {
                    name: s.name().unwrap_or("<invalid>").into(),
                    address: s.address(),
                    size: s.size(),
                    kind: format!("{:?}", s.kind()),
                })
                .collect()
        } else {
            vec![]
        },
    };
    let text = match crate::output::structured(&report, &format, "elf")? {
        Some(text) => text,
        None => {
            let mut t = String::new();
            output::heading(&mut t, "ELF");
            output::fields(
                &mut t,
                &[
                    ("File", output::path(file)),
                    ("Architecture", report.architecture.clone()),
                    ("Entry", format!("{:#010x}", report.entry)),
                ],
            );
            if !report.sections.is_empty() {
                let rows: Vec<[String; 4]> = report
                    .sections
                    .iter()
                    .map(|s| {
                        [
                            format!("{:#010x}", s.address),
                            s.size.to_string(),
                            s.kind.clone(),
                            s.name.clone(),
                        ]
                    })
                    .collect();
                output::heading(&mut t, "Sections");
                output::table(&mut t, &["ADDRESS", "SIZE", "KIND", "NAME"], &rows);
            }
            if !report.symbols.is_empty() {
                let rows: Vec<[String; 4]> = report
                    .symbols
                    .iter()
                    .map(|s| {
                        [
                            format!("{:#010x}", s.address),
                            s.size.to_string(),
                            s.kind.clone(),
                            s.name.clone(),
                        ]
                    })
                    .collect();
                output::heading(&mut t, "Symbols");
                output::table(&mut t, &["ADDRESS", "SIZE", "KIND", "NAME"], &rows);
            }
            t.trim_end().to_owned()
        }
    };
    if let Some(p) = output {
        fs::write(p, format!("{text}\n"))?;
    } else {
        println!("{text}");
    }
    Ok(0)
}
