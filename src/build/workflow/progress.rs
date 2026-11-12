// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::time::Instant;

use crate::build::build_graph::BuildPlan;
use crate::build::execution::ExecutionEvent;
use crate::build::render::progress;
use crate::config::configuration::ResolvedProject;

fn say(line: &str) {
    eprintln!("{}", crate::output::line(line));
}

pub(super) struct BuildProgress<'a> {
    plan: &'a BuildPlan,
    directory: std::path::PathBuf,
    started: Instant,
    counts: BTreeMap<&'static str, usize>,
    skipped: usize,
    verbose: bool,
    quiet: bool,
    dry_run: bool,
}

impl<'a> BuildProgress<'a> {
    pub(super) fn new(
        project: &ResolvedProject,
        plan: &'a BuildPlan,
        verbose: bool,
        quiet: bool,
        dry_run: bool,
    ) -> Self {
        if !quiet {
            for line in progress::introduction(project, verbose, dry_run) {
                say(&line);
            }
        }
        Self {
            plan,
            directory: project.root.clone(),
            started: Instant::now(),
            counts: BTreeMap::new(),
            skipped: 0,
            verbose,
            quiet,
            dry_run,
        }
    }

    pub(super) fn event(&mut self, event: &ExecutionEvent) {
        match event {
            ExecutionEvent::Skipped { .. } => self.skipped += 1,
            ExecutionEvent::Planned { operation, reason } => {
                self.count(operation);
                if !self.quiet {
                    say(&format!("Would run {operation}: {reason}"));
                }
            }
            ExecutionEvent::Started {
                operation,
                reason,
                command,
            } => {
                self.count(operation);
                if self.verbose {
                    say(&format!("{operation}: {reason}"));
                    if let Some(command) = command {
                        for line in progress::verbose_command(command, &self.directory) {
                            eprintln!("  {line}");
                        }
                    }
                }
            }
            ExecutionEvent::Failed { operation, .. } if !self.quiet => {
                say(&format!("Failed: {operation}"))
            }
            _ => {}
        }
    }

    fn count(&mut self, identifier: &str) {
        let Some(operation) = self
            .plan
            .operations
            .iter()
            .find(|operation| operation.id == identifier)
        else {
            return;
        };
        let Some(stage) = progress::stage(&operation.kind, &operation.action) else {
            return;
        };
        let count = self.counts.entry(stage).or_default();
        *count += 1;
        if !self.quiet
            && !self.verbose
            && !self.dry_run
            && (*count == 1 || count.is_multiple_of(20))
        {
            say(&format!("{stage}: {identifier} ({count} started)"));
        }
    }

    pub(super) fn finish(&self, succeeded: bool) {
        if self.quiet {
            return;
        }
        let count: usize = self.counts.values().sum();
        for line in progress::conclusion(
            self.plan,
            succeeded,
            self.dry_run,
            count,
            self.skipped,
            self.started.elapsed().as_secs_f64(),
        ) {
            eprintln!("{}", crate::output::shorten_paths(&line));
        }
    }
}
