// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::presentation::{Sbom, SbomComponent};
use serde_json::{Value, json};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::path::Path;

const TOOL: &str = "cheriot";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Hashed {
    name: String,
    sha1: String,
    sha256: String,
}

fn hashed_files(component: &SbomComponent) -> Vec<Hashed> {
    let root = Path::new(&component.root);
    let mut files: Vec<Hashed> = component
        .files
        .iter()
        .filter_map(|file| {
            let bytes = std::fs::read(file).ok()?;
            let path = Path::new(file);
            Some(Hashed {
                name: path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string(),
                sha1: hex(&Sha1::digest(&bytes)),
                sha256: hex(&Sha256::digest(&bytes)),
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

fn reference(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn content_digest(sbom: &Sbom, files: &[Vec<Hashed>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sbom.project.as_bytes());
    for (component, files) in sbom.components.iter().zip(files) {
        hasher.update(component.identity.name.as_bytes());
        hasher.update(
            component
                .identity
                .version
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
        );
        for file in files {
            hasher.update(file.name.as_bytes());
            hasher.update(file.sha256.as_bytes());
        }
    }
    hex(&hasher.finalize())
}

fn timestamp() -> String {
    let seconds = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs())
        });
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

fn insert(object: &mut Value, key: &str, value: Option<Value>) {
    if let (Some(map), Some(value)) = (object.as_object_mut(), value) {
        map.insert(key.into(), value);
    }
}

pub(super) fn cyclonedx(sbom: &Sbom) -> Value {
    let files: Vec<Vec<Hashed>> = sbom.components.iter().map(hashed_files).collect();
    let digest = content_digest(sbom, &files);
    let project_reference = format!("firmware:{}", reference(&sbom.project));
    let mut references = Vec::new();
    let components: Vec<Value> = sbom
        .components
        .iter()
        .zip(&files)
        .map(|(component, files)| {
            let identity = &component.identity;
            let bom_reference = format!(
                "component:{}@{}",
                reference(&identity.name),
                reference(identity.version.as_deref().unwrap_or("unversioned"))
            );
            references.push(bom_reference.clone());
            let mut entry = json!({
                "type": "library",
                "bom-ref": bom_reference,
                "name": identity.name,
                "properties": [
                    {"name": "cheriot:origin", "value": if component.third_party { "third-party" } else { "first-party" }},
                    {"name": "cheriot:files", "value": files.len().to_string()},
                ],
                "components": files.iter().map(|file| json!({
                    "type": "file",
                    "name": file.name,
                    "hashes": [{"alg": "SHA-256", "content": file.sha256}],
                })).collect::<Vec<_>>(),
            });
            insert(&mut entry, "version", identity.version.clone().map(Value::from));
            insert(&mut entry, "description", identity.description.clone().map(Value::from));
            insert(&mut entry, "supplier", identity.supplier.as_ref().map(|name| json!({"name": name})));
            insert(&mut entry, "licenses", identity.license.as_ref().map(|expression| json!([{"expression": expression}])));
            insert(&mut entry, "purl", identity.purl.clone().map(Value::from));
            insert(&mut entry, "cpe", identity.cpe.clone().map(Value::from));
            insert(&mut entry, "externalReferences", identity.repository.as_ref().map(|url| json!([{"type": "vcs", "url": url}])));
            entry
        })
        .collect();
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": format!(
            "urn:uuid:{}-{}-4{}-8{}-{}",
            &digest[0..8], &digest[8..12], &digest[13..16], &digest[17..20], &digest[20..32]
        ),
        "version": 1,
        "metadata": {
            "timestamp": timestamp(),
            "tools": [{"name": TOOL, "version": VERSION}],
            "component": {"type": "firmware", "bom-ref": project_reference, "name": sbom.project},
        },
        "components": components,
        "dependencies": [{"ref": project_reference, "dependsOn": references}],
    })
}

pub(super) fn spdx(sbom: &Sbom) -> Value {
    let all_files: Vec<Vec<Hashed>> = sbom.components.iter().map(hashed_files).collect();
    let digest = content_digest(sbom, &all_files);
    let mut packages = Vec::new();
    let mut files = Vec::new();
    let mut relationships = Vec::new();
    for (index, (component, hashed)) in sbom.components.iter().zip(&all_files).enumerate() {
        let identity = &component.identity;
        let package_id = format!("SPDXRef-Package-{index}-{}", reference(&identity.name));
        let mut digests: Vec<&str> = hashed.iter().map(|file| file.sha1.as_str()).collect();
        digests.sort_unstable();
        let verification = hex(&Sha1::digest(digests.concat().as_bytes()));
        let mut external = Vec::new();
        if let Some(purl) = &identity.purl {
            external.push(json!({"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": purl}));
        }
        if let Some(cpe) = &identity.cpe {
            external.push(json!({"referenceCategory": "SECURITY", "referenceType": "cpe23Type", "referenceLocator": cpe}));
        }
        let mut package = json!({
            "SPDXID": package_id,
            "name": identity.name,
            "downloadLocation": identity.repository.clone().unwrap_or_else(|| "NOASSERTION".into()),
            "filesAnalyzed": !hashed.is_empty(),
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": identity.license.clone().unwrap_or_else(|| "NOASSERTION".into()),
            "copyrightText": "NOASSERTION",
            "primaryPackagePurpose": if component.third_party { "LIBRARY" } else { "SOURCE" },
        });
        insert(
            &mut package,
            "versionInfo",
            identity.version.clone().map(Value::from),
        );
        insert(
            &mut package,
            "description",
            identity.description.clone().map(Value::from),
        );
        insert(
            &mut package,
            "supplier",
            identity
                .supplier
                .as_ref()
                .map(|name| format!("Organization: {name}").into()),
        );
        if !hashed.is_empty() {
            insert(
                &mut package,
                "packageVerificationCode",
                Some(json!({"packageVerificationCodeValue": verification})),
            );
        }
        if !external.is_empty() {
            insert(&mut package, "externalRefs", Some(external.into()));
        }
        packages.push(package);
        relationships.push(json!({"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "DESCRIBES", "relatedSpdxElement": package_id}));
        for (file_index, file) in hashed.iter().enumerate() {
            let file_id = format!("SPDXRef-File-{index}-{file_index}");
            files.push(json!({
                "SPDXID": file_id,
                "fileName": format!("./{}", file.name),
                "checksums": [
                    {"algorithm": "SHA1", "checksumValue": file.sha1},
                    {"algorithm": "SHA256", "checksumValue": file.sha256},
                ],
                "licenseConcluded": "NOASSERTION",
                "copyrightText": "NOASSERTION",
            }));
            relationships.push(json!({"spdxElementId": package_id, "relationshipType": "CONTAINS", "relatedSpdxElement": file_id}));
        }
    }
    json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": sbom.project,
        "documentNamespace": format!("https://spdx.org/spdxdocs/{}-{}", reference(&sbom.project), &digest[..32]),
        "creationInfo": {
            "created": timestamp(),
            "creators": [format!("Tool: {TOOL}-{VERSION}")],
        },
        "packages": packages,
        "files": files,
        "relationships": relationships,
    })
}
