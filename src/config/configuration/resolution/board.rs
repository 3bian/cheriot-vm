// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;

use crate::config::configuration::board_inheritance::BoardPaths;
use crate::config::configuration::{RawBoard, ResolvedBoard};
use crate::config::diagnostic::{Diagnostic, Origin, Result};

pub(super) fn resolve_board(
    input_board: RawBoard,
    values: super::BoardValues,
    paths: &BoardPaths<'_>,
    origins: &BTreeMap<String, Origin>,
) -> Result<ResolvedBoard> {
    let at = |field_path: &str| {
        origins
            .get(field_path)
            .cloned()
            .unwrap_or_else(|| Origin::derived(field_path))
    };
    let super::BoardValues {
        instruction_memory,
        data_memory,
        devices,
        mmio_start,
        mmio_end,
        revocable_start,
        flags,
    } = values;
    let includes = input_board
        .driver_includes
        .iter()
        .enumerate()
        .map(|(index, include)| {
            let origin = at(&format!("/board/driver_includes/{index}"));
            let path = paths.resolve(include, &origin)?;
            if !path.is_dir() {
                return Err(Diagnostic::new(format!(
                    "board include directory does not exist: {}",
                    path.display()
                ))
                .at(origin));
            }
            Ok(crate::config::diagnostic::Sourced::new(path, origin))
        })
        .collect::<Result<Vec<_>>>()?;
    let linker_fragments = super::linker_fragments::resolve_linker_fragments(
        input_board.linker_fragments,
        paths,
        origins,
    )?;
    Ok(ResolvedBoard {
        heap_start: input_board
            .heap
            .start
            .map(|start| format!("0x{start:x}"))
            .unwrap_or_else(|| ".".into()),
        name: input_board.name,
        cpu: input_board.cpu.unwrap_or_else(|| "cheriot".into()),
        heap: input_board.heap,
        revoker: input_board.revoker,
        interrupts: input_board.interrupts,
        defines: input_board.defines,
        cxflags: input_board.cxflags,
        rtos_defines: input_board.rtos_defines,
        timer_hz: input_board.timer_hz,
        tickrate_hz: input_board.tickrate_hz,
        simulation: input_board.simulation,
        packaging: input_board.packaging,
        linker_fragments,
        devices,
        instruction_memory,
        data_memory,
        mmio_start,
        mmio_end,
        revocable_start,
        flags,
        includes,
    })
}
