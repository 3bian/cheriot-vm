// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn inspect_reads_elf_and_rejects_non_elf() {
    let good = run(&[
        "inspect",
        "tests/fixtures/hello_world.elf",
        "--sections",
        "--format",
        "json",
    ]);
    assert!(good.status.success(), "{}", stderr(&good));
    let report: serde_json::Value = serde_json::from_slice(&good.stdout).unwrap();
    assert_eq!(report["architecture"], "Riscv32");
    assert!(!report["sections"].as_array().unwrap().is_empty());
    assert!(
        !run(&["inspect", "tests/fixtures/arithmetic.bin"])
            .status
            .success()
    );
}

#[cfg(unix)]
fn fake_toolchain(fixture: &test_fixture::TestFixture) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture.directory.join("fake-toolchain");
    std::fs::create_dir_all(&dir).unwrap();
    let program = r#"#!/bin/sh
set -eu
output=''; depfile=''; report=''; probe=false; link=false
if test "${1:-}" = --headers; then printf '  1 .loader_start 00000010 80000000 TEXT\n  2 .producer_code 00000020 80000010 TEXT\n  3 __cap_relocs 00000008 80000030 DATA\n'; exit 0; fi
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
if test -n "$output"; then if "$link"; then cp "$CHERIOT_TEST_ELF" "$output"; else printf 'object' > "$output"; fi; fi
if test -n "$report"; then printf '{}' > "$report"; fi
if test -n "$depfile"; then printf 'dummy: extra.h\n' > "$depfile"; fi
if test -z "$output"; then printf 'tool output\n'; fi
"#;
    for name in [
        "clang",
        "clang++",
        "ld.lld",
        "llvm-objdump",
        "llvm-objcopy",
        "llvm-strip",
    ] {
        let path = dir.join(name);
        std::fs::write(&path, program).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}

#[cfg(unix)]
#[test]
fn build_cli_executes_the_plan_and_clean_preserves_unowned_files() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let dir = fake_toolchain(&fixture);
    fixture.write_file("extra.h", "header");
    let elf = fixture.directory.join("sample.elf");
    std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
    let file = fixture.project_file();
    let d = dir.to_str().unwrap();
    let f = file.to_str().unwrap();
    let plan = run(&[
        "plan",
        "--config",
        f,
        "--toolchain-dir",
        d,
        "--format",
        "json",
    ]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let path = plan["publication"]["elf"].as_str().unwrap();
    let result = cheriot()
        .args([
            "build",
            "--config",
            f,
            "--toolchain-dir",
            d,
            "--format",
            "json",
        ])
        .env("CHERIOT_TEST_ELF", &elf)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        stderr(&result),
        stdout(&result)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report["error"].is_null(), "{report}");
    assert_eq!(
        std::fs::read(path).unwrap(),
        include_bytes!("../fixtures/hello_world.elf")
    );
    let profile = std::fs::canonicalize("profiles/cheriot.toml").unwrap();
    let execution = cheriot()
        .args([
            "exec",
            "--config",
            f,
            "--toolchain-dir",
            d,
            "--build",
            "--steps",
            "3000000",
            "--profile",
        ])
        .arg(profile)
        .env("CHERIOT_TEST_ELF", &elf)
        .output()
        .unwrap();
    assert!(
        execution.status.success(),
        "{} {}",
        stderr(&execution),
        stdout(&execution)
    );
    assert!(stdout(&execution).contains("Hello world"));
    let size = cheriot()
        .args([
            "size",
            "--config",
            f,
            "--toolchain-dir",
            d,
            "--format",
            "json",
        ])
        .env("CHERIOT_TEST_ELF", &elf)
        .output()
        .unwrap();
    assert!(size.status.success(), "{}", stderr(&size));
    let size: serde_json::Value = serde_json::from_slice(&size.stdout).unwrap();
    assert_eq!(size["bytes"], 56);
    let keep = fixture.directory.join("unowned.txt");
    std::fs::write(&keep, b"keep").unwrap();
    let clean = run(&["clean", "--config", f, "--toolchain-dir", d]);
    assert!(clean.status.success(), "{}", stderr(&clean));
    assert!(!Path::new(path).exists());
    assert_eq!(std::fs::read(keep).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn build_publishes_stable_target_paths_and_execute_finds_or_builds_them() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let dir = fake_toolchain(&fixture);
    let elf = fixture.directory.join("sample.elf");
    std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
    let file = fixture.project_file();
    let d = dir.to_str().unwrap();
    let f = file.to_str().unwrap();
    let plan = run(&[
        "plan",
        "--config",
        f,
        "--toolchain-dir",
        d,
        "--format",
        "json",
    ]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    let publication = &plan["publication"];
    let output_dir = std::path::PathBuf::from(publication["output_dir"].as_str().unwrap());
    let name = publication["firmware_name"].as_str().unwrap();
    assert!(matches!(
        output_dir.file_name().and_then(|n| n.to_str()),
        Some("release" | "debug")
    ));
    assert_eq!(
        output_dir.ancestors().nth(2).and_then(|p| p.file_name()),
        Some(std::ffi::OsStr::new("target"))
    );
    let stable = output_dir.join(format!("{name}.elf"));
    let profile = std::fs::canonicalize("profiles/cheriot.toml").unwrap();
    let execute = |extra: &[&str]| {
        cheriot()
            .args(["exec", "--config", f, "--toolchain-dir", d])
            .args(extra)
            .args(["--steps", "3000000", "--profile"])
            .arg(&profile)
            .env("CHERIOT_TEST_ELF", &elf)
            .output()
            .unwrap()
    };
    let missing = execute(&[]);
    assert!(!missing.status.success());
    let message = stderr(&missing);
    assert!(message.contains("firmware not built"), "{message}");
    assert!(!stable.exists());
    let built = execute(&["--build"]);
    let guest = stdout(&built);
    assert!(built.status.success(), "{} {guest}", stderr(&built));
    assert!(guest.contains("Hello world"), "{guest}");
    assert_eq!(
        std::fs::read(&stable).unwrap(),
        include_bytes!("../fixtures/hello_world.elf")
    );
    if publication["report_enabled"] == true {
        assert!(output_dir.join(format!("{name}.json")).is_file());
    }
    let rerun = execute(&[]);
    assert!(rerun.status.success(), "{}", stderr(&rerun));
    assert!(stdout(&rerun).contains("Hello world"));
    let conflicting = cheriot()
        .args(["exec", "tests/fixtures/hello_world.elf", "--config", f])
        .args(["--mode", "debug", "--profile"])
        .arg(&profile)
        .output()
        .unwrap();
    assert!(!conflicting.status.success());
    assert!(stderr(&conflicting).contains("project options need --build"));
    let clean = run(&["clean", "--config", f, "--toolchain-dir", d]);
    assert!(clean.status.success(), "{}", stderr(&clean));
    assert!(std::fs::symlink_metadata(&stable).is_err());
}

#[cfg(unix)]
#[test]
fn sonata_uf2_packages_are_native_validated_and_cleaned_as_one_generation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = test_fixture::TestFixture::new();
    fixture.replace_text(
        "boards/sail.toml",
        "name = \"sail\"",
        "name = \"sail\"\npackaging = \"sonata_v1\"",
    );
    fixture.write_plan_placeholders();
    fixture.write_file("extra.h", "header");
    let dir = fixture.directory.join("fake-tools");
    std::fs::create_dir_all(&dir).unwrap();
    let script = r#"#!/bin/sh
set -eu
output=''; depfile=''; report=''; probe=false; link=false
name=$(basename "$0")
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
if test "$name" = llvm-strip && test "${CHERIOT_TEST_FAIL_STRIP:-0}" = 1; then exit 1; fi
if test -n "$output"; then if "$link" || test "$name" = llvm-strip; then cp "$CHERIOT_TEST_ELF" "$output"; else printf 'object' > "$output"; fi; fi
if test -n "$report"; then printf '{}' > "$report"; fi
if test -n "$depfile"; then printf 'dummy: extra.h\n' > "$depfile"; fi
if test -z "$output"; then printf 'tool output\n'; fi
"#;
    for name in [
        "clang",
        "clang++",
        "ld.lld",
        "llvm-objdump",
        "llvm-objcopy",
        "llvm-strip",
    ] {
        let path = dir.join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let elf = fixture.directory.join("sample.elf");
    std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
    let file = fixture.project_file();
    let f = file.to_str().unwrap();
    let d = dir.to_str().unwrap();
    let launch = |fail: bool| {
        let mut cmd = cheriot();
        cmd.args([
            "package",
            "--config",
            f,
            "--toolchain-dir",
            d,
            "--format",
            "json",
        ])
        .env("CHERIOT_TEST_ELF", &elf);
        if fail {
            cmd.env("CHERIOT_TEST_FAIL_STRIP", "1");
        }
        cmd.output().unwrap()
    };
    let out = launch(false);
    assert!(out.status.success(), "{} {}", stderr(&out), stdout(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let artifacts = report["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 3);
    let mut first = Vec::new();
    for (i, item) in artifacts.iter().enumerate() {
        let path = item.as_str().unwrap();
        let data = std::fs::read(path).unwrap();
        let first_address = validate_uf2(&data, 0x6ce2_9e60, 256);
        assert_eq!(first_address, (i as u32) * 0x1000_0000);
        if i == 0 {
            first = data;
        }
    }
    let first_path = Path::new(artifacts[0].as_str().unwrap());
    let failed = launch(true);
    assert!(!failed.status.success());
    assert_eq!(std::fs::read(first_path).unwrap(), first);
    std::fs::write(first_path, b"modified").unwrap();
    let clean = run(&["clean", "--config", f, "--toolchain-dir", d]);
    assert!(!clean.status.success());
    assert_eq!(std::fs::read(first_path).unwrap(), b"modified");
    std::fs::write(first_path, &first).unwrap();
    let clean = run(&["clean", "--config", f, "--toolchain-dir", d]);
    assert!(clean.status.success(), "{}", stderr(&clean));
    assert!(!first_path.exists());
}

#[test]
fn generated_templates_match_the_recorded_baseline() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let file = fixture.project_file();
    let output = cheriot()
        .args(["plan", "--format", "json", "--config"])
        .arg(file)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let operations = plan["operations"].as_array().unwrap();
    let mmio: String =
        serde_json::from_str(include_str!("../fixtures/generated/mmio.ldscript.json")).unwrap();
    for (name, expected) in [
        ("generate.mmio.ldscript", mmio.as_str()),
        (
            "generate.board-interrupts.h",
            include_str!("../fixtures/generated/board-interrupts.h"),
        ),
    ] {
        let generated = operations.iter().find(|op| op["id"] == name).unwrap();
        assert_eq!(
            generated["action"]["content"].as_str().unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn empty_build_selections_are_rejected_before_project_loading() {
    for flag in ["--board", "--project", "--firmware"] {
        let output = run(&["build", flag, ""]);
        assert!(!output.status.success(), "{flag}");
        assert!(
            stderr(&output).contains(&format!("{flag} must not be empty")),
            "{}",
            stderr(&output)
        );
    }
    for flag in [
        "--config",
        "--overlay",
        "--build-dir",
        "--output-dir",
        "--toolchain-dir",
    ] {
        let output = run(&["build", flag, ""]);
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(stderr(&output).contains("a value is required"));
    }
}

fn validate_uf2(data: &[u8], family: u32, max_payload: u32) -> u32 {
    const START0: u32 = 0x0A32_4655;
    const START1: u32 = 0x9E5D_5157;
    const END: u32 = 0x0AB1_6F30;
    const FAMILY_FLAG: u32 = 0x0000_2000;
    assert!(
        !data.is_empty() && data.len().is_multiple_of(512),
        "whole blocks"
    );
    let count = (data.len() / 512) as u32;
    let word = |block: &[u8], offset: usize| {
        u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap())
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut ranges = Vec::new();
    for block in data.chunks_exact(512) {
        assert_eq!(word(block, 0), START0);
        assert_eq!(word(block, 4), START1);
        assert_eq!(word(block, 508), END);
        assert_eq!(word(block, 8), FAMILY_FLAG);
        assert_eq!(word(block, 28), family);
        assert_eq!(word(block, 24), count);
        assert!(word(block, 20) < count && seen.insert(word(block, 20)));
        let (address, length) = (word(block, 12), word(block, 16));
        assert!(address % 4 == 0 && length != 0 && length % 4 == 0 && length <= max_payload);
        ranges.push((address, address.checked_add(length).unwrap()));
    }
    ranges.sort_unstable();
    assert!(
        ranges.windows(2).all(|pair| pair[1].0 >= pair[0].1),
        "overlap"
    );
    ranges[0].0
}

#[cfg(unix)]
#[test]
fn shell_plan_runs_through_cheriot_on_path_and_publishes_the_firmware() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let dir = fake_toolchain(&fixture);
    fixture.write_file("extra.h", "header");
    let elf = fixture.directory.join("sample.elf");
    std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
    let file = fixture.project_file();
    let (d, f) = (dir.to_str().unwrap(), file.to_str().unwrap());
    let json = run(&[
        "plan",
        "--config",
        f,
        "--toolchain-dir",
        d,
        "--format",
        "json",
    ]);
    assert!(json.status.success(), "{}", stderr(&json));
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let published = json["publication"]["elf"].as_str().unwrap().to_owned();
    let shell = run(&[
        "plan",
        "--config",
        f,
        "--toolchain-dir",
        d,
        "--format",
        "shell",
    ]);
    assert!(shell.status.success(), "{}", stderr(&shell));
    let script = fixture.directory.join("plan.sh");
    std::fs::write(&script, &shell.stdout).unwrap();
    let bin = fixture.directory.join("wrapper-bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_cheriot"), bin.join("cheriot")).unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let result = Command::new("sh")
        .arg(script)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("CHERIOT_TEST_ELF", &elf)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(
        std::fs::read(published).unwrap(),
        include_bytes!("../fixtures/hello_world.elf")
    );
}

#[cfg(unix)]
struct TargetProject {
    fixture: test_fixture::TestFixture,
    toolchain: String,
    file: String,
    elf: std::path::PathBuf,
}

#[cfg(unix)]
impl TargetProject {
    fn new(extra: &str) -> Self {
        let fixture = test_fixture::TestFixture::new();
        fixture.write_plan_placeholders();
        let toolchain = fake_toolchain(&fixture).to_str().unwrap().to_owned();
        let elf = fixture.directory.join("sample.elf");
        std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
        let path = fixture.project_file();
        let profile = std::fs::canonicalize("profiles/cheriot.toml").unwrap();
        let project = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            format!(
                "target = \"vm\"\n{project}\n[targets.vm]\ndescription = \"emulator\"\n[targets.vm.exec]\nemulator = true\nprofile = {profile:?}\n\n[targets.sim.exec]\ncommand = [\"sh\", \"-c\", \"echo Assertion failure in ${{firmware_name}}; exit 3\"]\n\n{extra}"
            ),
        )
        .unwrap();
        Self {
            file: path.to_str().unwrap().to_owned(),
            fixture,
            toolchain,
            elf,
        }
    }

    fn run(&self, arguments: &[&str]) -> std::process::Output {
        cheriot()
            .args(["--config", &self.file])
            .args(arguments)
            .args(["--toolchain-dir", &self.toolchain])
            .env("CHERIOT_TEST_ELF", &self.elf)
            .output()
            .unwrap()
    }
}

#[cfg(unix)]
#[test]
fn test_builds_executes_and_judges_by_the_test_settings() {
    let project = TargetProject::new(
        "[test]\ntimeout = 60\nsteps = 3000000\npass = [\"Hello world\"]\nfail = [\"Assertion failure\"]\n",
    );
    let junit = project.fixture.directory.join("results.xml");
    let passed = project.run(&[
        "test",
        "--format",
        "json",
        "--quiet",
        "--junit",
        junit.to_str().unwrap(),
    ]);
    assert!(passed.status.success(), "{}", stderr(&passed));
    let report: serde_json::Value = serde_json::from_slice(&passed.stdout).unwrap();
    assert_eq!(report["passed"], 1);
    assert_eq!(report["failed"], 0);
    assert_eq!(report["cases"][0]["status"], "passed");
    assert_eq!(report["cases"][0]["target"], "vm");
    let xml = std::fs::read_to_string(&junit).unwrap();
    assert!(xml.contains("tests=\"1\" failures=\"0\""), "{xml}");
    assert!(xml.contains("Hello world"), "{xml}");

    let failed = project.run(&["test", "--target", "sim", "--no-build"]);
    assert_eq!(failed.status.code(), Some(1));
    let text = stdout(&failed);
    assert!(text.contains("Assertion failure in"), "{text}");
    for expected in [
        "failed",
        "exit code 3, expected 0",
        "output contains",
        "does not contain",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let unknown = project.run(&["test", "--case", "nope"]);
    assert!(!unknown.status.success());
    assert!(stderr(&unknown).contains("unknown test case"));
}

#[cfg(unix)]
#[test]
fn exec_runs_a_command_target_and_refuses_emulator_options_for_it() {
    let project = TargetProject::new("");
    let built = project.run(&["build"]);
    assert!(built.status.success(), "{}", stderr(&built));
    let out = project.run(&["exec", "--target", "sim"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stdout(&out).contains("Assertion failure in producer-consumer"));
    let emulated = project.run(&["exec", "--steps", "3000000"]);
    assert!(emulated.status.success(), "{}", stderr(&emulated));
    assert!(stdout(&emulated).contains("Hello world"));
    let refused = project.run(&["exec", "--target", "sim", "--steps", "5"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("emulator options"),
        "{}",
        stderr(&refused)
    );
    let no_debug = project.run(&["debug", "--target", "sim"]);
    assert!(stderr(&no_debug).contains("does not define [debug]"));
    let missing = project.run(&["exec", "--target", "nowhere"]);
    let message = stderr(&missing);
    assert!(message.contains("unknown target"), "{message}");
    assert!(
        message.contains("sim") && message.contains("vm"),
        "{message}"
    );
}

#[cfg(unix)]
#[test]
fn flash_packages_and_writes_to_the_target() {
    let project = TargetProject::new(
        "[targets.board]\ndescription = \"test board\"\n[targets.board.package]\ncommand = [\"cp\", \"${firmware}\", \"${package}\"]\nfile = \"${output_dir}/${firmware_name}.bin\"\n[targets.board.flash]\ncopy_to = \"${project_dir}/mount\"\n",
    );
    let mount = project.fixture.directory.join("mount");
    let absent = project.run(&["flash", "--target", "board"]);
    assert!(!absent.status.success());
    assert!(
        stderr(&absent).contains("is the board connected"),
        "{}",
        stderr(&absent)
    );

    std::fs::create_dir(&mount).unwrap();
    let dry = project.run(&["flash", "--target", "board", "--dry-run"]);
    assert!(dry.status.success(), "{}", stderr(&dry));
    assert!(stdout(&dry).contains("skipped"));
    assert!(!mount.join("producer-consumer.bin").exists());

    let flashed = project.run(&["flash", "--target", "board"]);
    assert!(flashed.status.success(), "{}", stderr(&flashed));
    assert!(
        stdout(&flashed).contains("test board"),
        "{}",
        stdout(&flashed)
    );
    assert_eq!(
        std::fs::read(mount.join("producer-consumer.bin")).unwrap(),
        std::fs::read(&project.elf).unwrap()
    );
    let other = project.fixture.directory.join("other");
    std::fs::create_dir(&other).unwrap();
    let redirected = project.run(&[
        "flash",
        "--target",
        "board",
        "--to",
        other.to_str().unwrap(),
    ]);
    assert!(redirected.status.success(), "{}", stderr(&redirected));
    assert!(other.join("producer-consumer.bin").is_file());
    let cannot = project.run(&["flash", "--target", "sim"]);
    assert!(stderr(&cannot).contains("does not define [flash]"));
}

#[test]
fn init_copies_the_template_and_the_result_is_a_valid_project() {
    let fixture = test_fixture::TestFixture::new();
    let destination = fixture.directory.join("demo-app");
    let d = destination.to_str().unwrap();
    let created = run(&[
        "init",
        d,
        "--sdk",
        "../sdk",
        "--board",
        "../boards/sail.toml",
    ]);
    assert!(created.status.success(), "{}", stderr(&created));
    let config = std::fs::read_to_string(destination.join("cheriot.toml")).unwrap();
    assert!(config.contains("name = \"demo-app\""), "{config}");
    assert!(!config.contains("{{"), "{config}");
    assert!(destination.join("src/hello.cc").is_file());
    let file = destination.join("cheriot.toml");
    let check = run(&["--config", file.to_str().unwrap(), "config", "check"]);
    assert!(check.status.success(), "{}", stderr(&check));

    let again = run(&["init", d, "--sdk", "../sdk"]);
    assert!(!again.status.success());
    assert!(stderr(&again).contains("--force"));
    assert!(
        run(&["init", d, "--sdk", "../sdk", "--force", "--name", "renamed"])
            .status
            .success()
    );
    let template = fixture.directory.join("my-template");
    std::fs::create_dir(&template).unwrap();
    std::fs::write(
        template.join("NOTES.txt"),
        "project {{name}} on {{board}}\n",
    )
    .unwrap();
    let custom = fixture.directory.join("custom");
    let made = run(&[
        "init",
        custom.to_str().unwrap(),
        "--template",
        template.to_str().unwrap(),
        "--board",
        "sonata",
    ]);
    assert!(made.status.success(), "{}", stderr(&made));
    assert_eq!(
        std::fs::read_to_string(custom.join("NOTES.txt")).unwrap(),
        "project custom on sonata\n"
    );
    let bad_name = run(&["init", custom.to_str().unwrap(), "--name", "has space"]);
    assert!(!bad_name.status.success());
}

#[cfg(unix)]
fn path_with(first: &Path) -> std::ffi::OsString {
    let rest = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(first.to_path_buf()).chain(std::env::split_paths(&rest)))
        .unwrap()
}

#[cfg(unix)]
#[test]
fn build_and_diagnose_find_the_same_toolchain_without_being_told() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let tools = fake_toolchain(&fixture);
    let elf = fixture.directory.join("sample.elf");
    std::fs::write(&elf, include_bytes!("../fixtures/hello_world.elf")).unwrap();
    let file = fixture.project_file();
    let global = fixture.directory.join("global.toml");
    let command = |arguments: &[&str]| {
        let mut command = cheriot();
        command
            .args(arguments)
            .arg("--config")
            .arg(&file)
            .env("CHERIOT_GLOBAL_CONFIG", &global)
            .env("CHERIOT_TEST_ELF", &elf)
            .env_remove("CHERIOT_TOOLCHAIN_DIR");
        command
    };
    let directory = |output: &Output| {
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        (
            report["toolchain"]["directory"].clone(),
            report["toolchain"]["source"].as_str().unwrap().to_owned(),
        )
    };

    let empty = fixture.directory.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let missing = command(&["build"]).env("PATH", &empty).output().unwrap();
    assert!(!missing.status.success());
    let message = stderr(&missing);
    assert!(message.contains("were not found"), "{message}");
    assert!(
        message.contains("cheriot config --global set toolchain_dir=DIR"),
        "{message}"
    );
    assert!(
        command(&["plan"])
            .env("PATH", &empty)
            .output()
            .unwrap()
            .status
            .success()
    );

    let on_path = command(&["build"])
        .env("PATH", path_with(&tools))
        .output()
        .unwrap();
    assert!(on_path.status.success(), "{}", stderr(&on_path));
    let diagnosed = command(&["diagnose", "--format", "json"])
        .env("PATH", path_with(&tools))
        .output()
        .unwrap();
    let (found, source) = directory(&diagnosed);
    assert_eq!(found, tools.to_str().unwrap());
    assert_eq!(source, "found on PATH");

    let set = command(&["config", "--global", "set"])
        .arg(format!("toolchain_dir={}", tools.display()))
        .output()
        .unwrap();
    assert!(set.status.success(), "{}", stderr(&set));
    command(&["clean"]).output().unwrap();
    let from_global = command(&["build"]).output().unwrap();
    assert!(from_global.status.success(), "{}", stderr(&from_global));
    let (found, source) = directory(&command(&["diagnose", "--format", "json"]).output().unwrap());
    assert_eq!(found, tools.to_str().unwrap());
    assert_eq!(source, "toolchain_dir in the global configuration");

    for (arguments, variable, expected) in [
        (
            &["diagnose", "--format", "json"][..],
            Some(&empty),
            "environment variable CHERIOT_TOOLCHAIN_DIR",
        ),
        (
            &[
                "diagnose",
                "--format",
                "json",
                "--toolchain-dir",
                empty.to_str().unwrap(),
            ][..],
            None,
            "command-line option --toolchain-dir",
        ),
    ] {
        let mut command = command(arguments);
        if let Some(variable) = variable {
            command.env("CHERIOT_TOOLCHAIN_DIR", variable);
        }
        let (found, source) = directory(&command.output().unwrap());
        assert_eq!(found, empty.to_str().unwrap());
        assert_eq!(source, expected);
    }
}

#[test]
fn plan_text_puts_each_argument_on_its_own_line() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let file = fixture.project_file();
    let plan = run(&["plan", "--config", file.to_str().unwrap()]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    let text = stdout(&plan);
    for section in ["Plan\n", "Notes\n", "Operations\n"] {
        assert!(text.contains(section), "{text}");
    }
    assert!(text.contains("clang++ \\\n        -c \\\n"), "{text}");
    assert!(
        text.contains("        -target riscv32-unknown-cheriotrtos \\\n"),
        "{text}"
    );
    assert!(!text.contains("Sourced {"), "{text}");
}

#[cfg(unix)]
#[test]
fn flash_finds_a_board_by_its_volume_label_and_package_stops_before_writing() {
    let project = TargetProject::new(
        "[targets.board.package]\ncommand = [\"cp\", \"${firmware}\", \"${package}\"]\nfile = \"${output_dir}/${firmware_name}.bin\"\n[targets.board.flash]\nvolume = \"CHERIOT-TEST-NO-SUCH-VOLUME\"\n",
    );
    let packaged = project.run(&["package", "--target", "board", "--format", "json"]);
    assert!(packaged.status.success(), "{}", stderr(&packaged));
    let report: serde_json::Value = serde_json::from_slice(&packaged.stdout).unwrap();
    assert_eq!(report["target"], "board");
    let package = report["artifacts"][0].as_str().unwrap();
    assert!(package.ends_with("producer-consumer.bin"), "{package}");
    assert!(Path::new(package).is_file());

    let absent = project.run(&["flash", "--target", "board"]);
    assert!(!absent.status.success());
    assert!(
        stderr(&absent).contains("no mounted volume is labelled CHERIOT-TEST-NO-SUCH-VOLUME"),
        "{}",
        stderr(&absent)
    );
    let dry = project.run(&["flash", "--target", "board", "--dry-run"]);
    assert!(dry.status.success(), "{}", stderr(&dry));
    assert!(stdout(&dry).contains("not mounted"), "{}", stdout(&dry));
    let dry = project.run(&[
        "flash",
        "--target",
        "board",
        "--dry-run",
        "--format",
        "json",
    ]);
    let report: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(report["status"], "skipped");
    assert_eq!(report["target"], "board");
    let directory = project.fixture.directory.join("drive");
    std::fs::create_dir(&directory).unwrap();
    let written = project.run(&[
        "flash",
        "--target",
        "board",
        "--to",
        directory.to_str().unwrap(),
    ]);
    assert!(written.status.success(), "{}", stderr(&written));
    assert!(directory.join("producer-consumer.bin").is_file());
}
