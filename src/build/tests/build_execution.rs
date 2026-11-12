// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(unix)]
use crate::build::build_graph::BuildAction;
use crate::build::execution::{self, ExecutionEvent};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};
#[allow(dead_code)]
#[path = "../../../tests/support/test_fixture.rs"]
mod test_fixture;

fn build_plan(fixture: &test_fixture::TestFixture) -> crate::build::build_graph::BuildPlan {
    let project = crate::config::project::load_project(&fixture.project_file()).unwrap();
    for recipe in crate::build::calculations::sdk::resolve_dependencies(&project).unwrap() {
        for source in recipe.sources {
            let source = source.path;
            if !source.exists() {
                fs::create_dir_all(source.parent().unwrap()).unwrap();
                fs::write(source, "// source existence fixture\n").unwrap();
            }
        }
        let script = project.sdk.join(recipe.script);
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(script, "/* linker input existence fixture */\n").unwrap();
    }
    crate::build::planning::create_build_plan(&project).unwrap()
}

#[test]
fn plan_executes_incrementally_publishes_and_cleans_owned_outputs() {
    let fixture = test_fixture::TestFixture::new();
    fixture.replace_text(
        "cheriot.toml",
        r#"mode = "release""#,
        "mode = \"release\"\nbuild_dir = \".build\"\noutput_dir = \".build/final\"",
    );
    fixture.write_file(
        "fake compiler ' tool",
        r#"#!/bin/sh
set -eu
output=''
depfile=''
report=''
probe=false
link=false
while test "$#" -gt 0; do
  case "$1" in
    -fsyntax-only) probe=true ;;
    --script=*) link=true ;;
    -o) shift; output=$1 ;;
    -MF) shift; depfile=$1 ;;
    --compartment-report=*) report=${1#*=} ;;
  esac
  shift
done
if "$probe"; then exit 0; fi
if test -n "$output"; then if "$link"; then cp "$SAMPLE_ELF" "$output"; else printf '%s' 'test artifact' > "$output"; fi; fi
if test -n "$report"; then printf '%s' '{}' > "$report"; fi
if test -n "$depfile"; then printf 'dummy: extra.h\n' > "$depfile"; fi
if test -z "$output"; then printf '%s\n' 'test stdout'; fi
"#,
    );
    fixture.write_file("extra.h", "first header\n");
    let sample = fixture.directory.join("sample.elf");
    fs::write(
        &sample,
        include_bytes!("../../../tests/fixtures/hello_world.elf"),
    )
    .unwrap();
    let tool = fixture.directory.join("fake compiler ' tool");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    let mut plan = build_plan(&fixture);
    for operation in &mut plan.operations {
        if let BuildAction::Command { command, .. } | BuildAction::Probe { command } =
            &mut operation.action
        {
            command.executable = tool.display().to_string();
            command.environment =
                BTreeMap::from([("SAMPLE_ELF".into(), sample.display().to_string())]);
        }
        operation.update_fingerprint().unwrap();
    }
    let mut events = Vec::new();
    execution::execute_build_plan(&plan, false, |event| events.push(event.clone())).unwrap();
    assert!(events.iter().any(
        |e| matches!(e,ExecutionEvent::Completed{operation,..} if operation=="publish.firmware")
    ));
    let publication = plan.publication.as_ref().unwrap();
    assert_eq!(
        fs::read(&publication.elf).unwrap(),
        include_bytes!("../../../tests/fixtures/hello_world.elf")
    );
    let mut repeat = Vec::new();
    execution::execute_build_plan(&plan, false, |e| repeat.push(e.clone())).unwrap();
    assert!(
        !repeat
            .iter()
            .any(|e| matches!(e, ExecutionEvent::Started { .. })),
        "{repeat:?}"
    );
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(plan.build_dir.join(".cheriot.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let blocked = execution::clean_build_plan(&plan).unwrap_err();
    assert!(blocked.to_string().contains("locked"));
    assert!(publication.elf.exists());
    drop(lock);
    let keep = fixture.directory.join("user-owned.txt");
    fs::write(&keep, "keep").unwrap();
    assert!(execution::clean_build_plan(&plan).unwrap() > 0);
    assert!(!publication.elf.exists());
    assert_eq!(fs::read_to_string(keep).unwrap(), "keep");
}
