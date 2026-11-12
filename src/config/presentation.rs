// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::metadata::{Catalogue, Identity};
use crate::config::configuration::ResolvedProject;
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const ORIGIN_KEYS: [&str; 5] = ["file", "line", "column", "rule", "source"];

fn is_origin(object: &Map<String, Value>) -> bool {
    ORIGIN_KEYS.iter().all(|key| object.contains_key(*key))
}

fn resolve_tool(name: &str) -> String {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return name.to_owned();
    }
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|directory| directory.join(name))
                .find(|candidate| candidate.is_file())
        })
        .map_or_else(|| name.to_owned(), |found| found.display().to_string())
}

struct Presenter<'a> {
    project: &'a Value,
    sdk_file: String,
    catalogue: Catalogue,
    files: BTreeSet<PathBuf>,
}

impl Presenter<'_> {
    fn resolved_path(&self, parent: &Map<String, Value>, key: &str) -> Option<String> {
        let sibling = key
            .strip_suffix("_origin")
            .and_then(|name| parent.get(name));
        let pointed = key
            .starts_with('/')
            .then(|| self.project.pointer(key))
            .flatten();
        let directory = (key == "/toolchain_dir")
            .then(|| self.project.pointer("/toolchain/directory"))
            .flatten();
        sibling
            .or(pointed)
            .or(directory)
            .and_then(Value::as_str)
            .map(resolve_tool)
    }

    fn origin(
        &mut self,
        origin: &mut Map<String, Value>,
        resolved: Option<String>,
    ) -> Result<(), String> {
        let rule = origin
            .remove("rule")
            .and_then(|rule| rule.as_str().map(str::to_owned));
        if origin.get("field").and_then(Value::as_str) == Some("") {
            origin.remove("field");
        }
        let file = origin
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if !file.starts_with('<') {
            self.files.insert(PathBuf::from(&file));
            if let Some(found) = self.catalogue.locate(Path::new(&file))? {
                origin.insert("component".into(), found.identity.name.into());
            }
            return Ok(());
        }
        origin.remove("line");
        origin.remove("column");
        match origin.get("source").and_then(Value::as_str) {
            Some("derived") => {
                let file = resolved.unwrap_or_else(|| self.sdk_file.clone());
                origin.insert("file".into(), file.into());
            }
            _ => {
                origin.remove("file");
                let rule = rule.unwrap_or_default();
                if let Some(option) = rule.strip_prefix("command-line option ") {
                    origin.insert("option".into(), option.into());
                } else if let Some(variable) = rule.strip_prefix("environment variable ") {
                    origin.insert("variable".into(), variable.into());
                }
            }
        }
        Ok(())
    }

    fn walk(&mut self, value: &mut Value) -> Result<(), String> {
        match value {
            Value::Object(object) => {
                let keys: Vec<String> = object.keys().cloned().collect();
                for key in keys {
                    let resolved = self.resolved_path(object, &key);
                    match object.get_mut(&key) {
                        Some(Value::Object(child)) if is_origin(child) => {
                            self.origin(child, resolved)?;
                        }
                        Some(child) => self.walk(child)?,
                        None => {}
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    match item {
                        Value::Object(child) if is_origin(child) => self.origin(child, None)?,
                        other => self.walk(other)?,
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn bill_of_materials(&mut self) -> Result<Sbom, String> {
        let mut by_component: BTreeMap<Identity, SbomComponent> = BTreeMap::new();
        let mut unidentified = Vec::new();
        for file in std::mem::take(&mut self.files) {
            let name = file.display().to_string();
            let Some(found) = self.catalogue.locate(&file)? else {
                unidentified.push(name);
                continue;
            };
            let declared_in = found.declared_in.display().to_string();
            let entry = by_component
                .entry(found.identity.clone())
                .or_insert_with(|| SbomComponent {
                    identity: found.identity,
                    third_party: found.third_party,
                    version_source: None,
                    root: found.root.display().to_string(),
                    declared_in: Vec::new(),
                    files: Vec::new(),
                });
            if !entry.declared_in.contains(&declared_in) {
                entry.declared_in.push(declared_in);
            }
            entry.files.push(name);
        }
        let mut components: Vec<SbomComponent> = by_component.into_values().collect();
        for component in &mut components {
            if component.identity.version.is_some() {
                component.version_source = Some("metadata");
            } else if let Some(version) = git_version(Path::new(&component.root)) {
                component.identity.version = Some(version);
                component.version_source = Some("git");
            }
        }
        Ok(Sbom {
            project: self.project["name"].as_str().unwrap_or_default().to_owned(),
            headers_from_build: false,
            components,
            unidentified,
        })
    }
}

#[derive(Serialize)]
pub(crate) struct SbomComponent {
    #[serde(flatten)]
    pub identity: Identity,
    pub third_party: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_source: Option<&'static str>,
    pub root: String,
    pub declared_in: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct Sbom {
    pub project: String,
    pub headers_from_build: bool,
    pub components: Vec<SbomComponent>,
    pub unidentified: Vec<String>,
}

fn collect_paths(value: &Value, files: &mut BTreeSet<PathBuf>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::Array(sources)) = object.get("sources") {
                for source in sources {
                    if let Some(path) = source.get("path").and_then(Value::as_str) {
                        files.insert(PathBuf::from(path));
                    }
                }
            }
            object
                .values()
                .for_each(|child| collect_paths(child, files));
        }
        Value::Array(items) => items.iter().for_each(|child| collect_paths(child, files)),
        _ => {}
    }
}

fn prune(value: &mut Value) -> bool {
    match value {
        Value::Null => false,
        Value::Object(object) => {
            object.retain(|_, child| prune(child));
            !object.is_empty()
        }
        Value::Array(items) => {
            items.retain_mut(prune);
            !items.is_empty()
        }
        _ => true,
    }
}

fn presenter<'a>(project: &ResolvedProject, original: &'a Value) -> Presenter<'a> {
    let mut files = BTreeSet::new();
    collect_paths(original, &mut files);
    files.extend(project.config_files.iter().cloned());
    Presenter {
        project: original,
        sdk_file: project.sdk.join("cheriot.toml").display().to_string(),
        catalogue: Catalogue::new(&project.config_files),
        files,
    }
}

pub(crate) fn document(project: &ResolvedProject) -> Result<Value, String> {
    let original = serde_json::to_value(project).map_err(|error| error.to_string())?;
    let mut presenter = presenter(project, &original);
    let mut document = original.clone();
    presenter.walk(&mut document)?;
    Ok(document)
}

pub(crate) fn sbom(project: &ResolvedProject) -> Result<Sbom, String> {
    let original = serde_json::to_value(project).map_err(|error| error.to_string())?;
    let mut presenter = presenter(project, &original);
    presenter.walk(&mut original.clone())?;
    let mut headers = BTreeSet::new();
    dependency_files(&project.build_dir, &mut headers, 0);
    let headers_from_build = !headers.is_empty();
    presenter.files.extend(headers);
    let mut sbom = presenter.bill_of_materials()?;
    sbom.headers_from_build = headers_from_build;
    Ok(sbom)
}

fn git_version(directory: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["describe", "--tags", "--always", "--dirty"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let version = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !version.is_empty()).then_some(version)
}

fn dependency_files(directory: &Path, found: &mut BTreeSet<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < 16 {
            dependency_files(&path, found, depth + 1);
        } else if kind.is_file()
            && path.extension().is_some_and(|extension| extension == "d")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            found.extend(
                parse_dependencies(&text)
                    .into_iter()
                    .filter(|file| file.is_absolute() && file.is_file()),
            );
        }
    }
}

fn parse_dependencies(text: &str) -> Vec<PathBuf> {
    let joined = text.replace("\\\n", " ").replace("\\\r\n", " ");
    let mut files = Vec::new();
    for rule in joined.lines() {
        let Some((_, prerequisites)) = rule.split_once(": ") else {
            continue;
        };
        let mut current = String::new();
        let mut characters = prerequisites.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '\\' if characters.peek() == Some(&' ') => {
                    current.push(' ');
                    characters.next();
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        files.push(PathBuf::from(std::mem::take(&mut current)));
                    }
                }
                c => current.push(c),
            }
        }
        if !current.is_empty() {
            files.push(PathBuf::from(current));
        }
    }
    files
}

pub(crate) enum Rendering {
    Text,
    Toml,
    Json,
}

pub(crate) fn render(project: &ResolvedProject, rendering: Rendering) -> Result<String, String> {
    let mut document = document(project)?;
    match rendering {
        Rendering::Json => {
            serde_json::to_string_pretty(&document).map_err(|error| error.to_string())
        }
        Rendering::Toml => {
            prune(&mut document);
            toml::to_string_pretty(&document)
                .map(|text| text.trim_end().to_owned())
                .map_err(|error| error.to_string())
        }
        Rendering::Text => Ok(summary(&document, project)),
    }
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "-".into(),
        Value::Array(items) if items.is_empty() => "-".into(),
        Value::Array(items) => items.iter().map(text_of).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

fn count(value: &Value) -> usize {
    value.as_array().map_or(0, Vec::len)
}

fn path_of(value: &Value) -> String {
    match value.as_str() {
        Some(path) if !path.is_empty() => crate::output::path(Path::new(path)),
        _ => text_of(value),
    }
}

fn summary(document: &Value, project: &ResolvedProject) -> String {
    use crate::output;
    let mut text = String::new();
    output::heading(&mut text, "Project");
    output::fields(
        &mut text,
        &[
            ("Name", text_of(&document["name"])),
            ("Mode", text_of(&document["mode"])),
            ("Board", text_of(&document["board"]["name"])),
            ("Configuration", path_of(&document["manifest"])),
            ("SDK", path_of(&document["sdk"])),
            ("Build directory", path_of(&document["build_dir"])),
            ("Output directory", path_of(&document["output_dir"])),
        ],
    );
    output::heading(&mut text, "Firmware");
    output::fields(
        &mut text,
        &[
            ("Name", text_of(&document["firmware"]["name"])),
            ("Services", text_of(&document["firmware"]["services"])),
            ("Libraries", text_of(&document["firmware"]["libraries"])),
            ("Threads", count(&document["threads"]).to_string()),
        ],
    );
    output::heading(&mut text, "Toolchain");
    let toolchain = &document["toolchain"];
    let located = toolchain["directory"]
        .as_str()
        .is_some_and(|directory| !directory.is_empty());
    if located {
        output::fields(
            &mut text,
            &[
                ("Directory", path_of(&toolchain["directory"])),
                ("Source", output::shorten_paths(&project.toolchain.source())),
            ],
        );
    } else {
        output::fields(&mut text, &[("Directory", "not found".into())]);
    }
    if let Some(components) = document["components"].as_array() {
        let rows: Vec<[String; 3]> = components
            .iter()
            .map(|component| {
                [
                    text_of(&component["name"]),
                    text_of(&component["kind"]),
                    count(&component["sources"]).to_string(),
                ]
            })
            .collect();
        output::heading(&mut text, "Components");
        output::table(&mut text, &["NAME", "KIND", "SOURCES"], &rows);
    }
    if let Some(provenance) = document["provenance"].as_object() {
        output::heading(&mut text, "Provenance");
        provenance_text(&mut text, provenance);
    }
    output::heading(&mut text, "More");
    output::lines(
        &mut text,
        ["Use --format toml or --format json for the complete resolved configuration."],
    );
    text.trim_end().to_owned()
}

type Position<'a> = (u64, u64, &'a str);

fn provenance_text(text: &mut String, provenance: &Map<String, Value>) {
    use crate::output;
    let mut files: BTreeMap<(String, String), Vec<Position>> = BTreeMap::new();
    let mut others: Vec<[String; 3]> = Vec::new();
    for (path, origin) in provenance {
        let path = if path.is_empty() { "/" } else { path.as_str() };
        match (
            origin.get("file"),
            origin.get("line").and_then(Value::as_u64),
        ) {
            (Some(file), Some(line)) => files
                .entry((
                    path_of(file),
                    origin.get("component").map(text_of).unwrap_or_default(),
                ))
                .or_default()
                .push((
                    line,
                    origin.get("column").and_then(Value::as_u64).unwrap_or(1),
                    path,
                )),
            (file, _) => others.push([
                path.to_owned(),
                text_of(&origin["source"]).replace('_', " "),
                origin
                    .get("option")
                    .or_else(|| origin.get("variable"))
                    .map(text_of)
                    .or_else(|| file.map(path_of))
                    .unwrap_or_else(|| "-".into()),
            ]),
        }
    }
    for ((file, component), mut entries) in files {
        entries.sort();
        let title = if component.is_empty() {
            file
        } else {
            format!("{file} ({component})")
        };
        text.push_str(&format!("  {}\n", output::bold(&title)));
        let width = entries
            .iter()
            .map(|(line, column, _)| format!("{line}:{column}").len())
            .max()
            .unwrap_or(0);
        for (line, column, path) in entries {
            let position = format!("{line}:{column}");
            text.push_str(&format!(
                "    {}  {path}\n",
                output::dim(&format!("{position:>width$}"))
            ));
        }
        text.push('\n');
    }
    if !others.is_empty() {
        text.push_str(&format!("  {}\n", output::bold("Not from a file")));
        let mut body = String::new();
        output::table(&mut body, &["PATH", "SOURCE", "VALUE"], &others);
        for line in body.lines() {
            text.push_str(&format!("  {line}\n"));
        }
    }
}
