// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_KEYS: &[&str] = &["target", "targets", "test"];

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Action {
    #[serde(default)]
    pub emulator: bool,
    pub profile: Option<String>,
    pub command: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Package {
    pub format: Option<String>,
    pub command: Option<Vec<String>>,
    pub file: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Flash {
    pub volume: Option<String>,
    pub copy_to: Option<String>,
    pub command: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Definition {
    pub description: Option<String>,
    pub package: Option<Package>,
    pub exec: Option<Action>,
    pub debug: Option<Action>,
    pub flash: Option<Flash>,
}

#[derive(Clone, Debug)]
pub(crate) struct Target {
    pub name: String,
    pub file: PathBuf,
    pub definition: Definition,
}

impl Definition {
    fn validate(&self) -> std::result::Result<(), String> {
        for (name, action) in [("exec", &self.exec), ("debug", &self.debug)] {
            let Some(action) = action else { continue };
            let command = action.command.as_ref();
            if action.emulator == command.is_some() {
                return Err(format!("[{name}] needs either emulator = true or command"));
            }
            if command.is_some_and(Vec::is_empty) {
                return Err(format!("[{name}] command must not be empty"));
            }
            if action.profile.is_some() && !action.emulator {
                return Err(format!(
                    "[{name}] profile only applies with emulator = true"
                ));
            }
        }
        if let Some(package) = &self.package {
            match (&package.format, &package.command, &package.file) {
                (Some(format), None, None) if format == "uf2" => {}
                (Some(format), None, None) => {
                    return Err(format!(
                        "[package] format {format:?} is not built in; use \"uf2\", or command and file"
                    ));
                }
                (None, Some(command), Some(_)) if !command.is_empty() => {}
                _ => {
                    return Err(
                        "[package] needs either format, or both command and file".to_owned()
                    );
                }
            }
        }
        if let Some(flash) = &self.flash {
            let command = flash.command.as_ref();
            let ways = [
                flash.volume.is_some(),
                flash.copy_to.is_some(),
                command.is_some(),
            ];
            if ways.iter().filter(|way| **way).count() != 1 {
                return Err("[flash] needs exactly one of volume, copy_to or command".to_owned());
            }
            if flash
                .volume
                .as_deref()
                .is_some_and(|volume| volume.is_empty() || volume.contains(['/', '\\']))
            {
                return Err("[flash] volume is a volume label, not a path".to_owned());
            }
            if command.is_some_and(Vec::is_empty) {
                return Err("[flash] command must not be empty".to_owned());
            }
        }
        Ok(())
    }
}

fn parse(text: &str, file: &Path, name: &str) -> Result<Target> {
    let definition: Definition = toml::from_str(text).map_err(|error| {
        AppError::Message(format!("target {name} in {}: {error}", file.display()))
    })?;
    definition.validate().map_err(|error| {
        AppError::Message(format!("target {name} in {}: {error}", file.display()))
    })?;
    Ok(Target {
        name: name.to_owned(),
        file: file.to_path_buf(),
        definition,
    })
}

fn config_table(config: &Path) -> Option<toml::Table> {
    toml::from_str(&std::fs::read_to_string(config).ok()?).ok()
}

/// The project's default target: the top-level `target` key, if any.
pub(crate) fn default_name(config: &Path) -> Option<String> {
    config_table(config)?
        .get("target")?
        .as_str()
        .map(str::to_owned)
}

fn search_directories(project_dir: &Path, sdk: Option<&Path>) -> Result<Vec<PathBuf>> {
    let mut directories = vec![project_dir.join("targets")];
    directories.extend(sdk.map(|sdk| sdk.join("targets")));
    if let Some(paths) = std::env::var_os("CHERIOT_TARGETS_DIR") {
        directories.extend(std::env::split_paths(&paths));
    }
    if let Some(paths) = crate::global::value("targets_dir").map_err(AppError::Message)? {
        directories.extend(std::env::split_paths(&paths));
    }
    directories.extend(crate::resources::directories("targets"));
    Ok(directories)
}

fn mount_points() -> Vec<PathBuf> {
    if let Ok(table) = std::fs::read_to_string("/proc/self/mounts") {
        return table
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .map(|point| PathBuf::from(point.replace("\\040", " ")))
            .collect();
    }
    let Ok(output) = std::process::Command::new("mount").output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(" on ")?;
            let rest = rest.rsplit_once(" (").map_or(rest, |x| x.0);
            Some(PathBuf::from(
                rest.rsplit_once(" type ").map_or(rest, |x| x.0),
            ))
        })
        .collect()
}

pub(crate) fn mounted_volume(label: &str) -> Option<PathBuf> {
    mount_points()
        .into_iter()
        .find(|point| point.file_name().is_some_and(|name| name == label))
}

pub(crate) fn find(name: &str, config: &Path, sdk: Option<&Path>) -> Result<Target> {
    let mut known = Vec::new();
    if let Some(toml::Value::Table(targets)) =
        config_table(config).and_then(|mut table| table.remove("targets"))
    {
        if let Some(definition) = targets.get(name) {
            let text = toml::to_string(definition)
                .map_err(|error| AppError::Message(error.to_string()))?;
            return parse(&text, config, name);
        }
        known.extend(targets.keys().cloned());
    }
    let read = |file: &Path, name: &str| -> Result<Target> {
        parse(&std::fs::read_to_string(file)?, file, name)
    };
    let direct = Path::new(name);
    if direct.is_file() {
        let stem = direct
            .file_stem()
            .map_or(name.into(), |stem| stem.to_string_lossy());
        return read(direct, &stem);
    }
    let project_dir = config.parent().unwrap_or(Path::new("."));
    let directories = search_directories(project_dir, sdk)?;
    for directory in &directories {
        let file = directory.join(format!("{name}.toml"));
        if file.is_file() {
            return read(&file, name);
        }
        if let Ok(entries) = std::fs::read_dir(directory) {
            known.extend(entries.flatten().filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == "toml").then(|| {
                    path.file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                })?
            }));
        }
    }
    known.sort();
    known.dedup();
    Err(AppError::Message(format!(
        "unknown target {name:?}. Define [targets.{name}] in {}, or add {name}.toml to a targets directory: the project's or the SDK's targets folder, one named by CHERIOT_TARGETS_DIR or the global targets_dir setting, or the one shipped with cheriot.{}",
        config.display(),
        crate::output::choices("Available targets", &known)
    )))
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Context {
    pub firmware: PathBuf,
    pub firmware_name: String,
    pub package: Option<PathBuf>,
    pub project_dir: PathBuf,
    pub build_dir: PathBuf,
    pub output_dir: PathBuf,
    pub sdk: PathBuf,
    pub destination: Option<String>,
}

impl Context {
    pub(crate) fn expand(&self, text: &str) -> Result<String> {
        let mut result = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("${") {
            result.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let end = after.find('}').ok_or_else(|| {
                AppError::Message(format!("unterminated ${{ in target value {text:?}"))
            })?;
            let name = &after[..end];
            let path = |path: &Path| path.display().to_string();
            let value = match name {
                "firmware" => path(&self.firmware),
                "firmware_name" => self.firmware_name.clone(),
                "project_dir" => path(&self.project_dir),
                "build_dir" => path(&self.build_dir),
                "output_dir" => path(&self.output_dir),
                "sdk" => path(&self.sdk),
                "package" => self.package.as_deref().map(path).ok_or_else(|| {
                    AppError::Message("${package} needs a [package] table in the target".into())
                })?,
                "destination" => self.destination.clone().ok_or_else(|| {
                    AppError::Message("${destination} needs the --to option".into())
                })?,
                other => match other.strip_prefix("env:") {
                    Some(variable) => std::env::var(variable).map_err(|_| {
                        AppError::Message(format!(
                            "environment variable {variable} is not set (used by the target)"
                        ))
                    })?,
                    None => {
                        return Err(AppError::Message(format!(
                            "unknown placeholder ${{{other}}} in target value {text:?}"
                        )));
                    }
                },
            };
            result.push_str(&value);
            rest = &after[end + 1..];
        }
        result.push_str(rest);
        Ok(result)
    }

    pub(crate) fn expand_command(&self, command: &[String]) -> Result<Vec<String>> {
        command
            .iter()
            .map(|argument| self.expand(argument))
            .collect()
    }
}

pub(crate) fn run(command: &[String], directory: &Path) -> Result<u8> {
    let (program, arguments) = command
        .split_first()
        .ok_or_else(|| AppError::Message("target command is empty".into()))?;
    let status = std::process::Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .status()
        .map_err(|error| AppError::Message(format!("cannot run {program}: {error}")))?;
    Ok(status.code().map_or(1, |code| code.clamp(0, 255) as u8))
}

pub(crate) fn profile_path(target: &Target, profile: &str) -> PathBuf {
    let relative = target.file.parent().unwrap_or(Path::new(".")).join(profile);
    if relative.is_file() {
        relative
    } else {
        PathBuf::from(profile)
    }
}

pub(crate) struct Prepared {
    pub target: Target,
    pub context: Context,
}

pub(crate) fn resolve_optional(
    selection: &crate::cli::Selection,
    config: Option<&Path>,
    name: Option<&str>,
) -> Result<(
    Option<Target>,
    crate::config::configuration::ResolvedProject,
)> {
    use crate::build::{selection as select, workflow, workflow::request::Action};
    let project = workflow::resolved_project(select::request(selection, config, Action::Build)?)
        .map_err(|error| AppError::Message(error.to_string()))?;
    let name = name
        .map(str::to_owned)
        .or_else(|| default_name(&project.manifest));
    let target = match name {
        Some(name) => Some(find(&name, &project.manifest, Some(&project.sdk))?),
        None => None,
    };
    Ok((target, project))
}

pub(crate) fn resolve(
    selection: &crate::cli::Selection,
    config: Option<&Path>,
    name: Option<&str>,
) -> Result<(Target, crate::config::configuration::ResolvedProject)> {
    let (target, project) = resolve_optional(selection, config, name)?;
    let target = target.ok_or_else(|| {
        AppError::Message(format!(
            "no target selected: pass --target NAME or set target = \"NAME\" in {}",
            project.manifest.display()
        ))
    })?;
    Ok((target, project))
}

pub(crate) fn prepare(
    selection: &crate::cli::Selection,
    config: Option<&Path>,
    name: Option<&str>,
    build: bool,
    elf: Option<PathBuf>,
) -> Result<Prepared> {
    use crate::build::{selection as select, workflow, workflow::request::Action};
    let (target, project) = resolve(selection, config, name)?;
    let firmware = match elf {
        Some(elf) => elf,
        None => {
            let request = select::request(selection, config, Action::Build)?;
            let path = if build {
                workflow::build_firmware(request)
            } else {
                workflow::published_firmware(request)
            }
            .map_err(|error| AppError::Message(error.to_string()))?;
            if !path.is_file() {
                return Err(AppError::Message(format!(
                    "firmware not built: {}; run `cheriot build` or pass --build",
                    path.display()
                )));
            }
            path
        }
    };
    Ok(Prepared {
        context: Context {
            firmware,
            firmware_name: project.firmware.name.clone(),
            package: None,
            project_dir: project.root.clone(),
            build_dir: project.build_dir.clone(),
            output_dir: project.output_dir.clone(),
            sdk: project.sdk.clone(),
            destination: None,
        },
        target,
    })
}

impl Prepared {
    pub(crate) fn standalone(target: Target, elf: &Path) -> Result<Self> {
        let firmware = std::fs::canonicalize(elf)
            .map_err(|error| AppError::Message(format!("{}: {error}", elf.display())))?;
        let directory = std::env::current_dir()?;
        Ok(Self {
            context: Context {
                firmware_name: firmware
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                output_dir: firmware
                    .parent()
                    .map_or(directory.clone(), Path::to_path_buf),
                firmware,
                project_dir: directory.clone(),
                build_dir: directory,
                ..Context::default()
            },
            target,
        })
    }

    pub(crate) fn package(
        &mut self,
        selection: &crate::cli::Selection,
        config: Option<&Path>,
    ) -> Result<PathBuf> {
        use crate::build::{selection as select, workflow, workflow::request::Action};
        let package = self.target.definition.package.clone().ok_or_else(|| {
            AppError::Message(format!(
                "target {} has no [package] table",
                self.target.name
            ))
        })?;
        let file = if let (Some(command), Some(file)) = (&package.command, &package.file) {
            let file = PathBuf::from(self.context.expand(file)?);
            self.context.package = Some(file.clone());
            if let Some(parent) = file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)?;
            }
            let command = self.context.expand_command(command)?;
            if run(&command, &self.context.project_dir)? != 0 {
                return Err(AppError::Message(format!(
                    "package command failed: {}",
                    command.join(" ")
                )));
            }
            if !file.is_file() {
                return Err(AppError::Message(format!(
                    "package command did not write {}",
                    file.display()
                )));
            }
            file
        } else if self.context.sdk.as_os_str().is_empty() {
            return Err(AppError::Message(format!(
                "target {} packages with the board's built-in layout, which needs a project",
                self.target.name
            )));
        } else {
            let invalid =
                |error: crate::config::diagnostic::Diagnostic| AppError::Message(error.to_string());
            let request = select::request(selection, config, Action::Build)?;
            let (project, plan) = workflow::build_for_package(request).map_err(invalid)?;
            let report = crate::build::execution::package_uf2(&project, &plan).map_err(invalid)?;
            report
                .artifacts
                .into_iter()
                .find(|artifact| artifact.extension().is_some_and(|e| e == "uf2"))
                .ok_or_else(|| AppError::Message("packaging produced no UF2 file".into()))?
        };
        self.context.package = Some(file.clone());
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_targets_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut count = 0;
        let entries = ["targets", "integrations/cheriot-rtos/sdk/targets"]
            .iter()
            .flat_map(|directory| std::fs::read_dir(root.join(directory)).unwrap().flatten());
        for entry in entries {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                parse(&std::fs::read_to_string(&path).unwrap(), &path, "shipped").unwrap();
                count += 1;
            }
        }
        assert!(count >= 7);
    }

    #[test]
    fn definitions_need_exactly_one_way_to_act() {
        let file = Path::new("target.toml");
        for invalid in [
            "[exec]\n",
            "[exec]\nemulator = true\ncommand = [\"x\"]\n",
            "[exec]\ncommand = [\"x\"]\nprofile = \"p\"\n",
            "[package]\nformat = \"zip\"\n",
            "[package]\ncommand = [\"x\"]\n",
            "[flash]\n",
            "[flash]\nvolume = \"BOARD\"\ncopy_to = \"out\"\n",
            "[flash]\nvolume = \"mnt/BOARD\"\n",
            "[exec]\nemulator = true\nextra = 1\n",
        ] {
            assert!(parse(invalid, file, "t").is_err(), "{invalid}");
        }
        for valid in [
            "[flash]\ncopy_to = \"${project_dir}/out\"\n",
            "[flash]\nvolume = \"BOARD\"\n",
        ] {
            assert!(parse(valid, file, "t").is_ok(), "{valid}");
        }
    }

    #[test]
    fn placeholders_expand_and_unknown_ones_are_refused() {
        let context = Context {
            firmware: PathBuf::from("/out/app.elf"),
            firmware_name: "app".into(),
            ..Context::default()
        };
        assert_eq!(
            context
                .expand("run ${firmware} as ${firmware_name}")
                .unwrap(),
            "run /out/app.elf as app"
        );
        assert!(context.expand("${package}").is_err());
        assert!(context.expand("${destination}").is_err());
        assert!(context.expand("${nonsense}").is_err());
        assert!(context.expand("${firmware").is_err());
    }
}
