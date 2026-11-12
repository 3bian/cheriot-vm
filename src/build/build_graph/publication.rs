// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::calculate_fingerprint;
use crate::config::configuration::ResolvedProject;

pub const MARKER_VERSION: &str = "cheriot-output-v2-sha256";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Publication {
    pub identity: String,
    pub owner: String,
    pub firmware_name: String,
    pub output_dir: PathBuf,
    pub elf: PathBuf,
    pub report: PathBuf,
    pub report_enabled: bool,
    pub marker: PathBuf,
    pub registry: PathBuf,
}

impl Publication {
    pub fn registry_content(&self) -> crate::config::diagnostic::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn for_project(project: &ResolvedProject) -> Self {
        Self::new(
            &project.build_dir,
            &project.output_dir,
            &project.firmware.name,
            &project.publication_identity,
            &project.publication_owner,
            project.firmware.compartment_report,
        )
    }

    pub fn new(
        build_dir: &Path,
        output_dir: &Path,
        name: &str,
        identity: &str,
        owner: &str,
        report_enabled: bool,
    ) -> Self {
        let destination_id = calculate_fingerprint(output_dir.to_string_lossy().as_bytes());
        Self {
            identity: identity.into(),
            owner: owner.into(),
            firmware_name: name.into(),
            output_dir: output_dir.into(),
            elf: output_dir.join(format!(".{name}.cheriot-current/{name}.elf")),
            report: output_dir.join(format!(".{name}.cheriot-current/{name}.json")),
            report_enabled,
            marker: output_dir.join(format!(".{name}.cheriot-current/.owner")),
            registry: build_dir.join(format!(".publication-{identity}-{destination_id}.json")),
        }
    }

    pub fn public_elf(&self) -> PathBuf {
        self.output_dir.join(format!("{}.elf", self.firmware_name))
    }

    pub fn public_report(&self) -> PathBuf {
        self.output_dir.join(format!("{}.json", self.firmware_name))
    }

    pub fn public_links(&self) -> [(PathBuf, PathBuf); 2] {
        let name = &self.firmware_name;
        [
            (
                self.public_elf(),
                PathBuf::from(format!(".{name}.cheriot-current/{name}.elf")),
            ),
            (
                self.public_report(),
                PathBuf::from(format!(".{name}.cheriot-current/{name}.json")),
            ),
        ]
    }

    pub fn outputs(&self) -> Vec<PathBuf> {
        let mut outputs = vec![self.elf.clone()];
        if self.report_enabled {
            outputs.push(self.report.clone());
        }
        outputs.extend([self.marker.clone(), self.registry.clone()]);
        outputs
    }

    pub fn lock_path(&self) -> PathBuf {
        self.output_dir
            .join(format!(".{}.cheriot-lock", self.firmware_name))
    }
}
