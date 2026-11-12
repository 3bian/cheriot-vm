// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn help_snapshots_follow_the_parser_graph() {
    let root = run(&["--help"]);
    assert!(root.status.success());
    assert_eq!(stdout(&root), include_str!("../snapshots/root-help.txt"));
    let plan = run(&["plan", "--help"]);
    assert!(plan.status.success());
    assert_eq!(stdout(&plan), include_str!("../snapshots/plan-help.txt"));
    assert!(run(&["plan", "--bad-option"]).status.code() == Some(2));
    assert!(run(&["exec", "--raw", "a", "--profile", "b"]).status.code() == Some(2));
    assert!(run(&["--version"]).status.success());
}

#[test]
fn global_options_work_after_subcommands() {
    let out = run(&[
        "inspect",
        "tests/fixtures/hello_world.elf",
        "--format",
        "json",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(Path::new("tests/fixtures/hello_world.elf").exists());
}

#[test]
fn run_and_debug_accept_the_same_firmware_options() {
    for command in ["exec", "debug"] {
        let help = stdout(&run(&[command, "--help"]));
        for option in [
            "--profile",
            "--build",
            "--raw",
            "--address",
            "--project",
            "--steps",
        ] {
            assert!(help.contains(option), "{command} is missing {option}");
        }
    }
}

#[test]
fn bare_commands_show_their_help_screen() {
    for command in ["exec", "debug", "config", "inspect"] {
        let bare = run(&[command]);
        let help = run(&[command, "--help"]);
        assert!(bare.status.success(), "{command}");
        assert_eq!(stdout(&bare), stdout(&help), "{command}");
    }
    let help = stdout(&run(&["exec", "--help"]));
    assert!(help.contains("--uart in=PATH|out=PATH"), "{help}");
    assert!(help.contains("path=/dev/gpiochip0,lines=17+27"), "{help}");
    assert!(help.contains("--console-format"));
    assert!(!help.contains("SPEC"));
    let missing = run(&["exec", "firmware.elf"]);
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("no machine profile selected"),
        "{}",
        stderr(&missing)
    );
}

const COMMANDS: &[&[&str]] = &[
    &[],
    &["build"],
    &["clean"],
    &["config"],
    &["config", "check"],
    &["config", "get"],
    &["config", "inspect"],
    &["config", "ls"],
    &["config", "reset"],
    &["config", "sbom"],
    &["config", "set"],
    &["debug"],
    &["diagnose"],
    &["exec"],
    &["flash"],
    &["init"],
    &["inspect"],
    &["package"],
    &["plan"],
    &["size"],
    &["test"],
];

#[test]
fn every_help_screen_fits_eighty_columns_and_reads_as_sentences() {
    for command in COMMANDS {
        let mut arguments = command.to_vec();
        arguments.push("--help");
        let help = run(&arguments);
        assert!(help.status.success(), "{command:?}");
        let text = stdout(&help);
        for line in text.lines() {
            assert!(line.chars().count() <= 80, "{command:?}: {line}");
        }
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let ends_description = line.starts_with("    ")
                && lines
                    .get(index + 1)
                    .is_none_or(|next| !next.starts_with("    "));
            if ends_description {
                assert!(line.ends_with('.'), "{command:?}: {line}");
            }
        }
    }
}

#[test]
fn only_config_groups_commands_and_the_old_build_subcommands_stand_alone() {
    for moved in ["clean", "inspect", "package", "plan", "size"] {
        assert!(run(&[moved, "--help"]).status.success(), "{moved}");
        assert_eq!(run(&["build", moved]).status.code(), Some(2), "{moved}");
    }
    for command in ["init", "inspect"] {
        let help = stdout(&run(&[command, "--help"]));
        assert!(!help.contains("--config"), "{command}: {help}");
    }
}

#[test]
fn long_errors_wrap_and_list_the_choices_in_columns() {
    let directory = tempfile::tempdir().unwrap();
    let mut workspace = String::from("[workspace.projects]\n");
    for index in 0..12 {
        let name = format!("a-project-with-quite-a-long-name-{index}");
        std::fs::write(directory.path().join(format!("{name}.toml")), "").unwrap();
        workspace.push_str(&format!("{name} = \"{name}.toml\"\n"));
    }
    let file = directory.path().join("cheriot.toml");
    std::fs::write(&file, workspace).unwrap();
    let refused = run(&["--config", file.to_str().unwrap(), "build"]);
    assert!(!refused.status.success());
    let message = String::from_utf8_lossy(&refused.stderr).into_owned();
    assert!(message.contains("--project NAME"), "{message}");
    assert!(message.contains("\nProjects:\n"), "{message}");
    assert!(
        message.contains("a-project-with-quite-a-long-name-11"),
        "{message}"
    );
    for line in message.lines() {
        assert!(line.chars().count() <= 80, "{line}");
    }
}

#[test]
fn usage_errors_read_like_every_other_error_and_point_at_help() {
    let moved = run(&["build", "plan"]);
    assert_eq!(moved.status.code(), Some(2));
    let message = String::from_utf8_lossy(&moved.stderr).into_owned();
    assert!(
        message.starts_with("cheriot: unexpected argument 'plan'"),
        "{message}"
    );
    assert!(
        message.contains("`cheriot build plan` is now `cheriot plan`."),
        "{message}"
    );
    assert!(
        message.ends_with("Run `cheriot build --help` for the options.\n"),
        "{message}"
    );

    let unknown = run(&["config", "set"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        stderr(&unknown).contains("Run `cheriot config set --help` for the options."),
        "{}",
        stderr(&unknown)
    );

    let directory = tempfile::tempdir().unwrap();
    for command in ["build", "clean", "plan", "size", "package", "test", "flash"] {
        let outside = cheriot()
            .arg(command)
            .current_dir(directory.path())
            .env_remove("CHERIOT_CONFIG")
            .output()
            .unwrap();
        assert_eq!(outside.status.code(), Some(1), "{command}");
        let message = stderr(&outside);
        assert!(
            message.starts_with("cheriot: no project here"),
            "{command}: {message}"
        );
        assert!(message.contains("cheriot init"), "{command}: {message}");
    }
}
