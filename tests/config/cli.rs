// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn config_check_and_plan_use_the_same_project_and_do_not_invoke_a_compiler() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let file = fixture.project_file();
    let file = file.to_str().unwrap();
    let check = run(&["--config", file, "config", "check"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(stdout(&check).contains("Configuration valid"));
    let plan = run(&["plan", "--config", file, "--format", "json"]);
    assert!(plan.status.success(), "{}", stderr(&plan));
    let p: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(p["operations"].as_array().unwrap().len() > 20);
    assert!(
        p["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["id"].as_str().unwrap_or("").contains("compile"))
    );
    let dry = run(&["build", "--dry-run", "--config", file, "--format", "json"]);
    assert!(dry.status.success(), "{}", stderr(&dry));
    let dry: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert!(dry["events"].as_array().unwrap().len() >= p["operations"].as_array().unwrap().len());
    assert!(!fixture.directory.join(".cheriot").exists());
}

#[test]
fn configuration_edits_are_validated_atomically_and_reset_to_default() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    let before = std::fs::read(&path).unwrap();
    let bad = run(&[
        "config",
        "set",
        "mode=debug",
        "not_a_field=7",
        "--config",
        file,
    ]);
    assert!(!bad.status.success(), "{}", stdout(&bad));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let good = run(&[
        "config",
        "set",
        "mode=debug",
        "output_dir=products",
        "--config",
        file,
    ]);
    assert!(good.status.success(), "{}", stderr(&good));
    let get = run(&["config", "get", "mode", "--config", file]);
    assert!(get.status.success(), "{}", stderr(&get));
    assert_eq!(stdout(&get).trim(), "debug");
    let reset = run(&["config", "reset", "mode", "output_dir", "--config", file]);
    assert!(reset.status.success(), "{}", stderr(&reset));
    let get = run(&["config", "get", "mode", "--config", file]);
    assert!(get.status.success());
    assert_eq!(stdout(&get).trim(), "release");
}

#[test]
fn config_lock_holder() {
    let Some(path) = std::env::var_os("CHERIOT_TEST_LOCK_PATH") else {
        return;
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.lock().unwrap();
    println!("CHERIOT_TEST_LOCKED");
    std::io::stdout().flush().unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn killed_lock_owner_releases_os_lock_and_sidecar_does_not_block() {
    use std::io::BufRead;
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let lock = path.with_file_name(".cheriot.toml.cheriot.lock");
    let mut child = Server(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "config::config_lock_holder", "--nocapture"])
            .env("CHERIOT_TEST_LOCK_PATH", &lock)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let reader = std::io::BufReader::new(child.0.stdout.take().unwrap());
    let mut ready = false;
    for line in reader.lines().take(10) {
        if line.unwrap().contains("CHERIOT_TEST_LOCKED") {
            ready = true;
            break;
        }
    }
    assert!(ready, "lock holder did not start");
    let file = path.to_str().unwrap();
    let blocked = run(&["config", "set", "mode=debug", "--config", file]);
    assert!(!blocked.status.success());
    assert!(stderr(&blocked).contains("locked"));
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(lock.exists());
    let after = run(&["config", "set", "mode=debug", "--config", file]);
    assert!(after.status.success(), "{}", stderr(&after));
}

#[test]
fn config_check_json_validates_machine_mappings_without_running_firmware() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    let good = run(&[
        "config",
        "check",
        "--config",
        file,
        "--profile",
        "tests/fixtures/rv32imc.toml",
        "--format",
        "json",
    ]);
    assert!(good.status.success(), "{}", stderr(&good));
    let report: serde_json::Value = serde_json::from_slice(&good.stdout).unwrap();
    assert_eq!(report["valid"], true);
    let profile = fixture.directory.join("overlap.toml");
    let mut text = std::fs::read_to_string("tests/fixtures/rv32imc.toml").unwrap();
    text.push_str(
        "\n[[memory]]\nname = \"overlap\"\ntype = \"ram\"\nbase = 0x80000000\nsize = 4096\n",
    );
    std::fs::write(&profile, text).unwrap();
    let bad = run(&[
        "config",
        "check",
        "--config",
        file,
        "--profile",
        profile.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert!(!bad.status.success());
    let report: serde_json::Value = serde_json::from_slice(&bad.stdout).unwrap();
    assert_eq!(report["valid"], false);
    assert!(!report["errors"].as_array().unwrap().is_empty());
}

#[test]
fn build_selection_prefers_explicit_values_and_rejects_invalid_environment() {
    let fixture = test_fixture::TestFixture::new();
    fixture.write_plan_placeholders();
    let file = fixture.project_file();
    let execute = |extra: &[&str], jobs: &str| {
        cheriot()
            .args(["build", "--dry-run", "--format", "json"])
            .args(extra)
            .env("CHERIOT_CONFIG", &file)
            .env("CHERIOT_JOBS", jobs)
            .output()
            .unwrap()
    };
    let from_environment = execute(&[], "1");
    assert!(
        from_environment.status.success(),
        "{}",
        stderr(&from_environment)
    );
    let invalid_jobs = execute(&[], "0");
    assert!(!invalid_jobs.status.success());
    assert!(stderr(&invalid_jobs).contains("jobs must be a whole number"));
    let explicit_jobs = execute(&["--jobs", "2"], "0");
    assert!(explicit_jobs.status.success(), "{}", stderr(&explicit_jobs));

    let explicit_file = cheriot()
        .args(["plan", "--config"])
        .arg(&file)
        .env("CHERIOT_CONFIG", "")
        .output()
        .unwrap();
    assert!(explicit_file.status.success(), "{}", stderr(&explicit_file));
    let missing_file = cheriot()
        .args(["plan"])
        .env("CHERIOT_CONFIG", "")
        .output()
        .unwrap();
    assert!(!missing_file.status.success());
    assert!(stderr(&missing_file).contains("CHERIOT_CONFIG"));
}

#[test]
fn config_with_no_subcommand_prints_help() {
    let bare = run(&["config"]);
    let help = run(&["config", "--help"]);
    assert!(help.status.success());
    assert_eq!(stdout(&bare), stdout(&help));
    assert!(stdout(&bare).contains("ls"));
    assert!(!stdout(&bare).contains("device"));
}

#[test]
fn config_ls_walks_the_selected_file_one_level_at_a_time() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();

    let top = run(&["--config", file, "config", "ls", "--format", "json"]);
    assert!(top.status.success(), "{}", stderr(&top));
    let top: serde_json::Value = serde_json::from_slice(&top.stdout).unwrap();
    let keys: Vec<_> = top
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["key"].as_str().unwrap().to_owned())
        .collect();
    assert!(keys.contains(&"firmwares".to_owned()), "{keys:?}");

    let firmwares = run(&[
        "--config",
        file,
        "config",
        "ls",
        "firmwares",
        "--format",
        "json",
    ]);
    let firmwares: serde_json::Value = serde_json::from_slice(&firmwares.stdout).unwrap();
    let first = firmwares[0]["key"].as_str().unwrap().to_owned();
    let by_name = run(&[
        "--config",
        file,
        "config",
        "ls",
        &format!("/firmwares/{first}"),
    ]);
    let by_index = run(&["--config", file, "config", "ls", "firmwares/0"]);
    assert!(by_name.status.success(), "{}", stderr(&by_name));
    assert_eq!(stdout(&by_name), stdout(&by_index));

    let missing = run(&["--config", file, "config", "ls", "firmwares/no-such"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("Keys:"), "{}", stderr(&missing));
}

#[test]
fn config_ls_is_a_sorted_name_list_and_get_returns_the_value_as_written() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();

    let top = run(&["--config", file, "config", "ls"]);
    assert!(top.status.success(), "{}", stderr(&top));
    let listing = stdout(&top);
    let names: Vec<&str> = listing.lines().collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(names.contains(&"firmwares/"), "{names:?}");
    assert!(names.contains(&"services/"), "{names:?}");
    assert!(names.contains(&"board"), "{names:?}");

    let board = run(&["--config", file, "config", "get", "/board"]);
    assert!(board.status.success(), "{}", stderr(&board));
    assert_eq!(stdout(&board).trim(), "./boards/sail.toml");
    let libraries = run(&[
        "--config",
        file,
        "config",
        "get",
        "firmwares/producer-consumer/libraries",
    ]);
    assert_eq!(
        stdout(&libraries).lines().collect::<Vec<_>>(),
        ["freestanding", "message_queue", "debug"]
    );
    let linker = run(&["--config", file, "config", "get", "toolchain/linker"]);
    assert!(linker.status.success(), "{}", stderr(&linker));
    assert!(stdout(&linker).trim().ends_with("ld.lld"));
    assert!(
        !run(&["--config", file, "config", "get", "firmwares.0"])
            .status
            .success()
    );
}

fn append(path: &std::path::Path, text: &str) {
    let mut contents = std::fs::read_to_string(path).unwrap();
    contents.push_str(text);
    std::fs::write(path, contents).unwrap();
}

#[test]
fn metadata_in_the_configuration_identifies_components_for_the_sbom() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    append(
        &path,
        "\n[metadata]\nname = \"producer-consumer\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
    );

    let check = run(&["--config", file, "config", "check"]);
    assert!(check.status.success(), "{}", stderr(&check));

    let out = run(&["--config", file, "config", "sbom", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["project"], "producer-consumer");
    let components = report["components"].as_array().unwrap();
    let find = |name: &str| {
        components
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("missing component {name}"))
    };
    let has = |component: &serde_json::Value, part: &str| {
        component["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().contains(part))
    };
    let project = find("producer-consumer");
    assert_eq!(project["version"], "0.1.0");
    assert_eq!(project["third_party"], false);
    assert!(has(project, "producer.cc"));
    let vendored = find("compiler-rt");
    assert_eq!(vendored["third_party"], true);
    assert_eq!(vendored["license"], "Apache-2.0 WITH LLVM-exception");
    assert!(has(vendored, "third_party/compiler-rt/"));
    assert!(!has(find("cheriot-rtos"), "third_party/"));
    assert!(report["unidentified"].as_array().unwrap().is_empty());

    let as_toml = run(&["--config", file, "config", "sbom", "--format", "toml"]);
    let parsed: toml::Value = toml::from_str(&stdout(&as_toml)).unwrap();
    assert_eq!(
        parsed["components"].as_array().unwrap().len(),
        components.len()
    );
    let text = stdout(&run(&["--config", file, "config", "sbom"]));
    assert!(text.contains("COMPONENT"), "{text}");
    assert!(text.contains("compiler-rt"), "{text}");
    assert!(text.contains("third-party"), "{text}");

    let out = run(&["--config", file, "config", "inspect", "--format", "json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let provenance = report["provenance"].as_object().unwrap();
    for (key, origin) in provenance {
        assert!(origin.get("rule").is_none(), "{key}");
        if let Some(file) = origin.get("file").and_then(|f| f.as_str()) {
            assert!(!file.starts_with('<'), "{key}: {file}");
        }
    }
    let tool = &provenance["/toolchain/object_copy"];
    assert!(tool["file"].as_str().unwrap().ends_with("llvm-objcopy"));
    assert_eq!(provenance["/mode"]["component"], "producer-consumer");
    let text = run(&["--config", file, "config", "inspect", "--format", "toml"]);
    assert!(toml::from_str::<toml::Value>(&stdout(&text)).is_ok());

    append(&path, "nme = 1\n");
    let bad = run(&["--config", file, "config", "check"]);
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("metadata"), "{}", stderr(&bad));
}

#[test]
fn every_report_is_available_as_text_json_and_toml() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    for arguments in [
        &["config", "check"][..],
        &["config", "ls"][..],
        &["config", "get", "firmwares"][..],
        &["plan"][..],
        &["build", "--dry-run"][..],
        &["inspect", "tests/fixtures/hello_world.elf", "--sections"][..],
    ] {
        let mut json = vec!["--config", file];
        json.extend_from_slice(arguments);
        let mut as_toml = json.clone();
        json.extend_from_slice(&["--format", "json"]);
        as_toml.extend_from_slice(&["--format", "toml"]);
        let json = run(&json);
        assert!(json.status.success(), "{arguments:?}: {}", stderr(&json));
        assert!(serde_json::from_slice::<serde_json::Value>(&json.stdout).is_ok());
        let as_toml = run(&as_toml);
        assert!(
            as_toml.status.success(),
            "{arguments:?}: {}",
            stderr(&as_toml)
        );
        assert!(
            toml::from_str::<toml::Value>(&stdout(&as_toml)).is_ok(),
            "{arguments:?}: {}",
            stdout(&as_toml)
        );
    }
}

fn nested(text: &str, prefix: &str) -> String {
    let mut out = format!("\n[{prefix}]\n");
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(name) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
            out.push_str(&format!("[[{prefix}.{name}]]\n"));
        } else if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push_str(&format!("[{prefix}.{name}]\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[test]
fn boards_and_profiles_can_live_inside_the_project_file() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    let inspect = |file: &str| -> serde_json::Value {
        let out = run(&["--config", file, "config", "inspect", "--format", "json"]);
        assert!(out.status.success(), "{}", stderr(&out));
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let separate = inspect(file);

    let board = std::fs::read_to_string(fixture.directory.join("boards/sail.toml")).unwrap();
    let profile = std::fs::read_to_string("profiles/cheriot.toml").unwrap();
    let project = std::fs::read_to_string(&path).unwrap();
    let project = project
        .lines()
        .map(|line| {
            if line.starts_with("board = ") {
                "board = \"inline\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &path,
        format!(
            "{project}\n{}{}",
            nested(&board, "boards.inline"),
            nested(&profile, "profiles.vm")
        ),
    )
    .unwrap();

    let embedded = inspect(file);
    for key in ["audit_board", "threads", "components", "firmware"] {
        assert_eq!(embedded[key], separate[key], "{key}");
    }
    assert_eq!(embedded["board"]["name"], separate["board"]["name"]);
    let files = embedded["config_files"].as_array().unwrap();
    assert!(
        !files
            .iter()
            .any(|f| f.as_str().unwrap().ends_with("boards/sail.toml"))
    );

    let check = run(&["--config", file, "config", "check", "--profile", "vm"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(stdout(&check).contains("Machine profile valid"));
    for extra in [&[][..], &["--profile", "vm"][..]] {
        let mut arguments = vec!["--config", file, "exec", "tests/fixtures/hello_world.elf"];
        arguments.extend_from_slice(extra);
        let out = run(&arguments);
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("Hello world"));
    }
    let out = run(&[
        "--config",
        file,
        "exec",
        "tests/fixtures/hello_world.elf",
        "--profile",
        "profiles/cheriot.toml",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn sbom_exports_cyclonedx_and_spdx_with_file_hashes() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    append(
        &path,
        "\n[metadata]\nname = \"producer-consumer\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
    );
    let header = fixture.directory.join("sdk/include/cdefs.h");
    let inspect = run(&["--config", file, "config", "inspect", "--format", "json"]);
    let inspect: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    let build = std::path::PathBuf::from(inspect["build_dir"].as_str().unwrap());
    std::fs::create_dir_all(&build).unwrap();
    let escaped = header.display().to_string().replace(' ', "\\ ");
    std::fs::write(build.join("a.o.d"), format!("a.o: \\\n  {escaped}\n")).unwrap();

    let export = |format: &str| -> serde_json::Value {
        let out = cheriot()
            .args(["--config", file, "config", "sbom", "--format", format])
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        serde_json::from_slice(&out.stdout).unwrap()
    };

    let cyclonedx = export("cyclonedx");
    assert_eq!(cyclonedx["bomFormat"], "CycloneDX");
    assert_eq!(cyclonedx["specVersion"], "1.5");
    assert_eq!(cyclonedx["metadata"]["timestamp"], "2023-11-14T22:13:20Z");
    assert_eq!(
        cyclonedx["metadata"]["component"]["name"],
        "producer-consumer"
    );
    let components = cyclonedx["components"].as_array().unwrap();
    let project = components
        .iter()
        .find(|c| c["name"] == "producer-consumer")
        .unwrap();
    assert_eq!(project["version"], "0.1.0");
    assert_eq!(project["licenses"][0]["expression"], "MIT");
    let hash = project["components"][0]["hashes"][0]["content"]
        .as_str()
        .unwrap();
    assert_eq!(hash.len(), 64);
    let sdk = components
        .iter()
        .find(|c| c["name"] == "cheriot-rtos")
        .unwrap();
    assert!(
        sdk["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == "include/cdefs.h")
    );
    assert_eq!(cyclonedx, export("cyclonedx"));

    let spdx = export("spdx");
    assert_eq!(spdx["spdxVersion"], "SPDX-2.3");
    assert_eq!(spdx["creationInfo"]["created"], "2023-11-14T22:13:20Z");
    let packages = spdx["packages"].as_array().unwrap();
    assert_eq!(packages.len(), components.len());
    let package = packages
        .iter()
        .find(|p| p["name"] == "producer-consumer")
        .unwrap();
    assert_eq!(package["licenseDeclared"], "MIT");
    assert_eq!(
        package["packageVerificationCode"]["packageVerificationCodeValue"]
            .as_str()
            .unwrap()
            .len(),
        40
    );
    let files = spdx["files"].as_array().unwrap();
    assert!(!files.is_empty());
    assert_eq!(files[0]["checksums"][0]["algorithm"], "SHA1");
    let contains = spdx["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["relationshipType"] == "CONTAINS")
        .count();
    assert_eq!(contains, files.len());
}

#[test]
fn configuration_schema_version_is_checked() {
    let fixture = test_fixture::TestFixture::new();
    let path = fixture.project_file();
    let file = path.to_str().unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("schema = 1\n{original}")).unwrap();
    assert!(run(&["--config", file, "config", "check"]).status.success());
    std::fs::write(&path, format!("schema = 2\n{original}")).unwrap();
    let newer = run(&["--config", file, "config", "check"]);
    assert!(!newer.status.success());
    assert!(
        stderr(&newer).contains("this cheriot reads schema 1"),
        "{}",
        stderr(&newer)
    );
}

#[test]
fn global_settings_are_stored_for_the_user_and_checked() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("settings").join("config.toml");
    let global = |arguments: &[&str]| {
        cheriot()
            .args(["config", "--global"])
            .args(arguments)
            .env("CHERIOT_GLOBAL_CONFIG", &file)
            .current_dir(directory.path())
            .output()
            .unwrap()
    };
    let listed = stdout(&global(&["ls"]));
    assert_eq!(
        listed.lines().collect::<Vec<_>>(),
        ["color", "jobs", "targets_dir", "template", "toolchain_dir"]
    );
    let unset = global(&["get", "toolchain_dir"]);
    assert!(!unset.status.success());
    assert!(stderr(&unset).contains("is not set"), "{}", stderr(&unset));

    let set = global(&["set", "toolchain_dir=tools/bin", "jobs=4", "color=never"]);
    assert!(set.status.success(), "{}", stderr(&set));
    assert!(file.is_file());
    let stored = stdout(&global(&["get", "/toolchain_dir"]));
    assert!(Path::new(stored.trim()).is_absolute(), "{stored}");
    assert!(stored.trim().ends_with("tools/bin"), "{stored}");
    assert_eq!(stdout(&global(&["get", "jobs"])).trim(), "4");
    let inspected: serde_json::Value =
        serde_json::from_slice(&global(&["inspect", "--format", "json"]).stdout).unwrap();
    assert_eq!(inspected["settings"]["color"], "never");
    assert!(inspected["settings"]["template"].is_null());
    assert!(global(&["check"]).status.success());

    let before = std::fs::read_to_string(&file).unwrap();
    for (assignment, expected) in [
        ("colour=always", "unknown global setting"),
        ("color=pink", "auto, always or never"),
        ("jobs=0", "between 1 and 256"),
    ] {
        let refused = global(&["set", assignment]);
        assert!(!refused.status.success(), "{assignment}");
        assert!(stderr(&refused).contains(expected), "{}", stderr(&refused));
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    assert!(global(&["reset", "jobs"]).status.success());
    assert!(!global(&["get", "jobs"]).status.success());
    std::fs::write(&file, "nonsense = true\n").unwrap();
    let invalid = global(&["check"]);
    assert!(!invalid.status.success());
    assert!(
        stderr(&invalid).contains("unknown global setting"),
        "{}",
        stderr(&invalid)
    );
    assert!(stderr(&global(&["sbom"])).contains("describes a project"));
}
