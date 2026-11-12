// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;

use super::{BuildPlan, Diagnostic, Result, load_state, rebuild_reason, record};

pub(super) fn ensure_plan_fresh(plan: &BuildPlan) -> Result<()> {
    let state = load_state(plan)?.ok_or_else(|| {
        Diagnostic::new("build state is missing; build the firmware again before running")
    })?;
    let mut probes = plan.unresolved_probes();
    for (id, value) in &mut probes {
        if let Some(record) = state.operations.get(id) {
            *value = record.probe;
        }
    }
    let mut tools = BTreeMap::new();
    for operation in &plan.operations {
        let current = record(operation, &probes, &mut tools)?;
        if rebuild_reason(&current, state.operations.get(&operation.id)).is_some() {
            return Err(Diagnostic::new(format!(
                "{} changed after the build; build again",
                operation.id
            )));
        }
    }
    Ok(())
}
