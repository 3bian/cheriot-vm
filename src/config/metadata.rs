// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component as PathComponent, Path, PathBuf},
};

pub(crate) const KEY: &str = "metadata";
const DEFAULT_FILE: &str = "cheriot.toml";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpe: Option<String>,
}

impl Identity {
    fn validate(&self) -> Result<(), String> {
        let fields = [
            Some(&self.name),
            self.version.as_ref(),
            self.supplier.as_ref(),
            self.license.as_ref(),
            self.description.as_ref(),
            self.repository.as_ref(),
            self.purl.as_ref(),
            self.cpe.as_ref(),
        ];
        if fields
            .into_iter()
            .flatten()
            .any(|value| value.trim().is_empty())
        {
            return Err("metadata values must not be empty".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThirdParty {
    path: String,
    name: String,
    version: Option<String>,
    supplier: Option<String>,
    license: Option<String>,
    description: Option<String>,
    repository: Option<String>,
    purl: Option<String>,
    cpe: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: String,
    version: Option<String>,
    supplier: Option<String>,
    license: Option<String>,
    description: Option<String>,
    repository: Option<String>,
    purl: Option<String>,
    cpe: Option<String>,
    #[serde(default)]
    third_party: Vec<ThirdParty>,
}

#[derive(Clone, Debug)]
pub(crate) struct Metadata {
    pub identity: Identity,
    pub third_party: Vec<(PathBuf, Identity)>,
}

pub(crate) fn parse(source: &str) -> Result<Option<Metadata>, String> {
    #[derive(Deserialize)]
    struct Holder {
        metadata: Option<Table>,
    }
    let holder: Holder = toml::from_str(source)
        .map_err(|error| format!("invalid [metadata]: {}", error.message()))?;
    let Some(table) = holder.metadata else {
        return Ok(None);
    };
    let identity = Identity {
        name: table.name,
        version: table.version,
        supplier: table.supplier,
        license: table.license,
        description: table.description,
        repository: table.repository,
        purl: table.purl,
        cpe: table.cpe,
    };
    identity.validate()?;
    let mut third_party = Vec::new();
    for entry in table.third_party {
        let path = PathBuf::from(&entry.path);
        let inside = !entry.path.is_empty()
            && path
                .components()
                .all(|part| matches!(part, PathComponent::Normal(_)));
        if !inside {
            return Err(format!(
                "metadata third_party path {:?} must be a relative path inside the component",
                entry.path
            ));
        }
        let identity = Identity {
            name: entry.name,
            version: entry.version,
            supplier: entry.supplier,
            license: entry.license,
            description: entry.description,
            repository: entry.repository,
            purl: entry.purl,
            cpe: entry.cpe,
        };
        identity.validate()?;
        third_party.push((path, identity));
    }
    Ok(Some(Metadata {
        identity,
        third_party,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Attribution {
    pub identity: Identity,
    pub declared_in: PathBuf,
    pub root: PathBuf,
    pub third_party: bool,
}

#[derive(Default)]
pub(crate) struct Catalogue {
    extra_files: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    files: BTreeMap<PathBuf, Option<Metadata>>,
}

impl Catalogue {
    pub(crate) fn new(config_files: &[PathBuf]) -> Self {
        let mut catalogue = Self::default();
        for file in config_files {
            if let Some(directory) = file.parent() {
                catalogue
                    .extra_files
                    .entry(directory.to_path_buf())
                    .or_default()
                    .insert(file.clone());
            }
        }
        catalogue
    }

    fn metadata(&mut self, file: &Path) -> Result<Option<Metadata>, String> {
        if let Some(known) = self.files.get(file) {
            return Ok(known.clone());
        }
        let metadata = match std::fs::read_to_string(file) {
            Ok(source) => parse(&source).map_err(|error| format!("{}: {error}", file.display()))?,
            Err(_) => None,
        };
        self.files.insert(file.to_path_buf(), metadata.clone());
        Ok(metadata)
    }

    pub(crate) fn locate(&mut self, file: &Path) -> Result<Option<Attribution>, String> {
        let mut directory = file.parent();
        while let Some(current) = directory {
            let mut candidates: Vec<PathBuf> = vec![current.join(DEFAULT_FILE)];
            if let Some(extra) = self.extra_files.get(current) {
                candidates.extend(extra.iter().cloned());
            }
            let mut own = None;
            let mut best: Option<Attribution> = None;
            for candidate in candidates {
                let Some(metadata) = self.metadata(&candidate)? else {
                    continue;
                };
                for (relative, identity) in &metadata.third_party {
                    let root = current.join(relative);
                    let deeper = best
                        .as_ref()
                        .is_none_or(|b| root.components().count() > b.root.components().count());
                    if file.starts_with(&root) && deeper {
                        best = Some(Attribution {
                            identity: identity.clone(),
                            declared_in: candidate.clone(),
                            root,
                            third_party: true,
                        });
                    }
                }
                own.get_or_insert(Attribution {
                    identity: metadata.identity,
                    declared_in: candidate,
                    root: current.to_path_buf(),
                    third_party: false,
                });
            }
            if let Some(found) = best.or(own) {
                return Ok(Some(found));
            }
            directory = current.parent();
        }
        Ok(None)
    }
}
