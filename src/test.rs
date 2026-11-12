// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::{Selection, TestArgs};
use crate::output;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECONDS: u64 = 300;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    firmware: Option<String>,
    target: Option<String>,
    timeout: Option<u64>,
    steps: Option<u64>,
    pass: Option<Vec<String>>,
    fail: Option<Vec<String>>,
    exit_code: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    target: Option<String>,
    timeout: Option<u64>,
    steps: Option<u64>,
    #[serde(default)]
    pass: Vec<String>,
    #[serde(default)]
    fail: Vec<String>,
    exit_code: Option<i32>,
    #[serde(default)]
    cases: Vec<Case>,
}

#[derive(Serialize)]
struct CaseResult {
    name: String,
    firmware: String,
    target: String,
    status: &'static str,
    seconds: f64,
    exit_code: Option<i32>,
    reasons: Vec<String>,
    #[serde(skip)]
    output: String,
}

#[derive(Serialize)]
struct Report {
    project: String,
    passed: usize,
    failed: usize,
    cases: Vec<CaseResult>,
}

fn settings(manifest: &Path) -> Result<Settings> {
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(manifest)?)
        .map_err(|error| AppError::Message(format!("{}: {error}", manifest.display())))?;
    match table.get("test") {
        None => Ok(Settings::default()),
        Some(value) => value.clone().try_into().map_err(|error| {
            AppError::Message(format!("[test] in {}: {error}", manifest.display()))
        }),
    }
}

fn command_line(
    arguments: &TestArgs,
    selection: &Selection,
    config: Option<&Path>,
    target: Option<&str>,
    steps: Option<u64>,
) -> Result<(Vec<String>, PathBuf, String, String)> {
    use crate::build::{selection as select, workflow, workflow::request::Action};
    let invalid =
        |error: crate::config::diagnostic::Diagnostic| AppError::Message(error.to_string());
    let project = workflow::resolved_project(select::request(selection, config, Action::Build)?)
        .map_err(invalid)?;
    let request = select::request(selection, config, Action::Build)?;
    let firmware = if arguments.no_build {
        workflow::published_firmware(request)
    } else {
        workflow::build_firmware(request)
    }
    .map_err(invalid)?;
    if !firmware.is_file() {
        return Err(AppError::Message(format!(
            "firmware not built: {}; drop --no-build or run `cheriot build`",
            firmware.display()
        )));
    }
    let path = |path: &Path| path.display().to_string();
    let myself = std::env::current_exe()?;
    let mut emulator = vec![
        path(&myself),
        "--config".into(),
        path(&project.manifest),
        "exec".into(),
        path(&firmware),
    ];
    if let Some(steps) = steps {
        emulator.extend(["--steps".into(), steps.to_string()]);
    }
    let name = target
        .map(str::to_owned)
        .or_else(|| crate::target::default_name(&project.manifest));
    let Some(name) = name else {
        return Ok((
            emulator,
            project.root,
            "emulator".into(),
            project.firmware.name,
        ));
    };
    let found = crate::target::find(&name, &project.manifest, Some(&project.sdk))?;
    let action = found.definition.exec.clone().ok_or_else(|| {
        AppError::Message(format!("target {} does not define [exec]", found.name))
    })?;
    let command = match action.command {
        None => {
            if let Some(profile) = &action.profile {
                emulator.extend([
                    "--profile".into(),
                    path(&crate::target::profile_path(&found, profile)),
                ]);
            }
            emulator
        }
        Some(command) => {
            let mut prepared = crate::target::prepare(
                selection,
                config,
                Some(&name),
                false,
                Some(firmware.clone()),
            )?;
            if command
                .iter()
                .any(|argument| argument.contains("${package}"))
            {
                prepared.package(selection, config)?;
            }
            prepared.context.expand_command(&command)?
        }
    };
    Ok((command, project.root, found.name, project.firmware.name))
}

fn pump(
    mut stream: impl Read + Send + 'static,
    buffer: Arc<Mutex<Vec<u8>>>,
    echo: Option<fn(&[u8])>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut chunk = [0; 4096];
        while let Ok(count) = stream.read(&mut chunk) {
            if count == 0 {
                break;
            }
            if let Some(echo) = echo {
                echo(&chunk[..count]);
            }
            if let Ok(mut buffer) = buffer.lock() {
                buffer.extend_from_slice(&chunk[..count]);
            }
        }
    })
}

fn run_case(
    command: &[String],
    directory: &Path,
    timeout: Duration,
    quiet: bool,
) -> Result<(Option<i32>, String, bool)> {
    let (program, rest) = command
        .split_first()
        .ok_or_else(|| AppError::Message("target command is empty".into()))?;
    let mut child = std::process::Command::new(program)
        .args(rest)
        .current_dir(directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| AppError::Message(format!("cannot run {program}: {error}")))?;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let to_stdout: fn(&[u8]) = |bytes| {
        let _ = std::io::stdout().write_all(bytes);
        let _ = std::io::stdout().flush();
    };
    let to_stderr: fn(&[u8]) = |bytes| {
        let _ = std::io::stderr().write_all(bytes);
    };
    let readers = [
        child
            .stdout
            .take()
            .map(|stream| pump(stream, buffer.clone(), (!quiet).then_some(to_stdout))),
        child
            .stderr
            .take()
            .map(|stream| pump(stream, buffer.clone(), (!quiet).then_some(to_stderr))),
    ];
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let output = buffer
        .lock()
        .map(|buffer| String::from_utf8_lossy(&buffer).into_owned())
        .unwrap_or_default();
    Ok((status.and_then(|status| status.code()), output, timed_out))
}

fn xml(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

fn junit(report: &Report) -> String {
    let total: f64 = report.cases.iter().map(|case| case.seconds).sum();
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" time=\"{total:.3}\">\n",
        xml(&report.project),
        report.cases.len(),
        report.failed
    );
    for case in &report.cases {
        text.push_str(&format!(
            "  <testcase name=\"{}\" classname=\"{}.{}\" time=\"{:.3}\">\n",
            xml(&case.name),
            xml(&report.project),
            xml(&case.target),
            case.seconds
        ));
        if !case.reasons.is_empty() {
            text.push_str(&format!(
                "    <failure message=\"{}\"/>\n",
                xml(&case.reasons.join("; "))
            ));
        }
        let start = case.output.len().saturating_sub(64 * 1024);
        let tail = case.output.get(start..).unwrap_or(&case.output);
        text.push_str(&format!(
            "    <system-out>{}</system-out>\n  </testcase>\n",
            xml(tail)
        ));
    }
    text.push_str("</testsuite>\n");
    text
}

pub(crate) fn handle(arguments: TestArgs, config: Option<&Path>) -> Result<u8> {
    let file = config.map_or_else(|| PathBuf::from("cheriot.toml"), Path::to_path_buf);
    let manifest =
        crate::config::configuration::select_project(&file, arguments.selection.project.as_deref())
            .map_err(|error| AppError::Message(error.to_string()))?;
    let project_name = toml::from_str::<toml::Table>(&std::fs::read_to_string(&manifest)?)
        .ok()
        .and_then(|table| table.get("name")?.as_str().map(str::to_owned))
        .unwrap_or_default();
    let settings = settings(&manifest)?;
    let mut cases = settings.cases.clone();
    if cases.is_empty() {
        cases.push(Case {
            name: arguments
                .selection
                .firmware
                .clone()
                .unwrap_or_else(|| "firmware".into()),
            ..Case::default()
        });
    }
    if !arguments.case.is_empty() {
        for wanted in &arguments.case {
            if !cases.iter().any(|case| &case.name == wanted) {
                let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
                return Err(AppError::Message(format!(
                    "unknown test case {wanted:?}{}",
                    output::choices("Test cases", &names)
                )));
            }
        }
        cases.retain(|case| arguments.case.contains(&case.name));
    }

    let mut results = Vec::new();
    for case in &cases {
        let mut selection = arguments.selection.clone().with_jobs(arguments.jobs);
        if case.firmware.is_some() {
            selection.firmware = case.firmware.clone();
        }
        let target = arguments
            .target
            .as_deref()
            .or(case.target.as_deref())
            .or(settings.target.as_deref());
        let steps = case.steps.or(settings.steps);
        let timeout = Duration::from_secs(
            case.timeout
                .or(settings.timeout)
                .unwrap_or(DEFAULT_TIMEOUT_SECONDS),
        );
        let started = Instant::now();
        let mut reasons = Vec::new();
        let mut exit_code = None;
        let mut output_text = String::new();
        let mut target_name = target.unwrap_or("emulator").to_owned();
        let mut firmware_name = case.firmware.clone().unwrap_or_default();
        match command_line(&arguments, &selection, config, target, steps) {
            Err(error) => reasons.push(error.to_string()),
            Ok((command, directory, resolved_target, firmware)) => {
                target_name = resolved_target;
                firmware_name = firmware;
                if !arguments.quiet {
                    eprintln!("{} {}", output::bold("Running"), case.name);
                }
                let (code, text, timed_out) =
                    run_case(&command, &directory, timeout, arguments.quiet)?;
                exit_code = code;
                if timed_out {
                    reasons.push(format!("timed out after {} seconds", timeout.as_secs()));
                }
                let expected = case.exit_code.or(settings.exit_code).unwrap_or(0);
                if !timed_out && code != Some(expected) {
                    reasons.push(match code {
                        Some(code) => format!("exit code {code}, expected {expected}"),
                        None => "ended by a signal".to_owned(),
                    });
                }
                for pattern in case.fail.as_ref().unwrap_or(&settings.fail) {
                    if text.contains(pattern.as_str()) {
                        reasons.push(format!("output contains {pattern:?}"));
                    }
                }
                for pattern in case.pass.as_ref().unwrap_or(&settings.pass) {
                    if !text.contains(pattern.as_str()) {
                        reasons.push(format!("output does not contain {pattern:?}"));
                    }
                }
                output_text = text;
            }
        }
        results.push(CaseResult {
            name: case.name.clone(),
            firmware: firmware_name,
            target: target_name,
            status: if reasons.is_empty() {
                "passed"
            } else {
                "failed"
            },
            seconds: (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
            exit_code,
            reasons,
            output: output_text,
        });
    }

    let failed = results
        .iter()
        .filter(|case| !case.reasons.is_empty())
        .count();
    let report = Report {
        project: project_name,
        passed: results.len() - failed,
        failed,
        cases: results,
    };
    if let Some(path) = &arguments.junit {
        std::fs::write(path, junit(&report))?;
    }
    match output::structured(&report, &arguments.format, "tests")? {
        Some(text) => println!("{text}"),
        None => {
            let mut text = String::new();
            output::heading(&mut text, "Tests");
            let rows: Vec<[String; 5]> = report
                .cases
                .iter()
                .map(|case| {
                    [
                        case.name.clone(),
                        case.status.to_owned(),
                        case.target.clone(),
                        format!("{:.2}s", case.seconds),
                        case.reasons.join("; "),
                    ]
                })
                .collect();
            output::table(
                &mut text,
                &["CASE", "RESULT", "TARGET", "TIME", "REASON"],
                &rows,
            );
            output::heading(&mut text, "Summary");
            output::fields(
                &mut text,
                &[
                    ("Passed", report.passed.to_string()),
                    ("Failed", report.failed.to_string()),
                ],
            );
            if !arguments.quiet {
                println!();
            }
            print!("{text}");
        }
    }
    Ok(u8::from(report.failed > 0))
}
