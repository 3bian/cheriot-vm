// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::{canonicalize_path, deserialize_configuration, read_toml_document};
use crate::config::diagnostic::{Diagnostic, Origin, Result};

pub struct Workspace {
    pub projects: BTreeMap<String, PathBuf>,
}

pub fn read_workspace(path: &Path) -> Result<Option<Workspace>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WorkspaceDocument {
        workspace: WorkspaceProjects,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WorkspaceProjects {
        projects: BTreeMap<String, String>,
    }

    let path = canonicalize_path(path)?;
    let document = read_toml_document(&path)?;
    if !document.map()?.contains_key("workspace") {
        return Ok(None);
    }
    let raw: WorkspaceDocument = deserialize_configuration(&document)?;
    if raw.workspace.projects.is_empty() {
        return Err(Diagnostic::new("workspace must name at least one project").at(document.origin));
    }
    let root = path
        .parent()
        .ok_or_else(|| Diagnostic::new("workspace needs a directory"))?;
    let mut origins = BTreeMap::<String, Origin>::new();
    document.origins("", &mut origins);
    let mut projects = BTreeMap::new();
    for (name, value) in raw.workspace.projects {
        let origin = origins
            .get(&format!("/workspace/projects/{name}"))
            .cloned()
            .unwrap_or_else(|| document.origin.clone());
        let relative = Path::new(&value);
        if !super::resolution::is_target_name(&name)
            || relative.is_absolute()
            || relative.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || relative
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("toml")
        {
            return Err(Diagnostic::new("workspace project names must be valid and paths must be relative TOML files within the workspace").at(origin));
        }
        let project = canonicalize_path(&root.join(relative)).map_err(|_| {
            Diagnostic::new(format!("workspace project {name} is missing: {value}; copy the complete RTOS overlay into the checkout"))
                .at(origin.clone())
        })?;
        if !project.starts_with(root) || project == path || !project.is_file() {
            return Err(Diagnostic::new(
                "workspace project must be a different file inside the workspace",
            )
            .at(origin));
        }
        projects.insert(name, project);
    }
    Ok(Some(Workspace { projects }))
}

pub fn missing_project(path: &Path) -> Diagnostic {
    Diagnostic::new(format!(
        "no project here: {} was not found. Run this in a project directory, name the file with --config FILE or the CHERIOT_CONFIG environment variable, or create a project with `cheriot init`",
        path.display()
    ))
}

pub fn select_project(path: &Path, name: Option<&str>) -> Result<PathBuf> {
    if !path.exists() {
        return Err(missing_project(path));
    }
    let available = |workspace: &Workspace| {
        let names: Vec<&String> = workspace.projects.keys().collect();
        crate::output::choices("Projects", &names)
    };
    match (read_workspace(path)?, name) {
        (Some(workspace), Some(name)) => workspace.projects.get(name).cloned().ok_or_else(|| {
            Diagnostic::new(format!(
                "unknown workspace project {name:?}{}",
                available(&workspace)
            ))
        }),
        (Some(workspace), None) => Err(Diagnostic::new(format!(
            "this is a workspace of several projects; select one with --project NAME{}",
            available(&workspace)
        ))),
        (None, Some(_)) => Err(Diagnostic::new(
            "--project selects a project of a workspace, and this configuration is a single project",
        )),
        (None, None) => Ok(path.to_path_buf()),
    }
}
