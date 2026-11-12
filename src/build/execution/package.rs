// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{
    BuildLock, check_build_identity, plan_freshness::ensure_plan_fresh, process, publication,
    validate_llvm_executable,
};
use crate::build::build_graph::{BuildCommand, BuildPlan, Publication, path_to_string};
use crate::build::package::uf2::{FlashImage, FlashRegion, Uf2Layout, encode_uf2, validate_uf2};
use crate::config::configuration::{PackagingKind, ResolvedProject};
use crate::config::diagnostic::{Diagnostic, Result, Sourced};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    num::NonZeroU16,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const MAX_IMAGE: usize = 64 * 1024 * 1024;
#[derive(Debug, Serialize)]
pub struct PackageReport {
    pub board: String,
    pub format: &'static str,
    pub artifacts: Vec<PathBuf>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    owner: String,
    source_sha256: String,
    files: Vec<ManifestFile>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    name: String,
    sha256: String,
}
fn prefix(p: &Publication) -> String {
    format!(".{}.cheriot-package-generation-", p.firmware_name)
}
fn pointer(p: &Publication) -> PathBuf {
    p.output_dir
        .join(format!(".{}.cheriot-package-current", p.firmware_name))
}
fn claim(p: &Publication) -> String {
    format!("cheriot-uf2-v1\n{}\n", p.owner)
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|e| Diagnostic::io(path, e))?;
    if !meta.is_file() {
        return Err(Diagnostic::new(format!(
            "expected regular file {}",
            path.display()
        )));
    }
    fs::read(path).map_err(|e| Diagnostic::io(path, e))
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| Diagnostic::io(path, e))
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| Diagnostic::io(path, e))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| Diagnostic::io(path, e))
}
fn validate_generation(p: &Publication, generation: &Path) -> Result<Manifest> {
    if !fs::symlink_metadata(generation).is_ok_and(|m| m.is_dir()) {
        return Err(Diagnostic::new("invalid package generation"));
    }
    let claim_file = generation.join(".claim");
    if read_regular(&claim_file)? != claim(p).as_bytes() {
        return Err(Diagnostic::new(
            "package generation belongs to another project",
        ));
    }
    let path = generation.join("manifest.json");
    let manifest: Manifest = serde_json::from_slice(&read_regular(&path)?)?;
    if manifest.version != 1 || manifest.owner != p.owner || manifest.files.is_empty() {
        return Err(Diagnostic::new(
            "package generation belongs to another project or has invalid metadata",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.files {
        if Path::new(&entry.name).components().count() != 1
            || !matches!(
                Path::new(&entry.name).components().next(),
                Some(Component::Normal(_))
            )
            || !seen.insert(&entry.name)
        {
            return Err(Diagnostic::new("invalid package artifact name"));
        }
        let bytes = read_regular(&generation.join(&entry.name))?;
        if digest(&bytes) != entry.sha256 {
            return Err(Diagnostic::new(format!(
                "modified package artifact {}",
                entry.name
            )));
        }
    }
    Ok(manifest)
}
fn selected(p: &Publication) -> Result<Option<(PathBuf, Manifest)>> {
    let pointer = pointer(p);
    let meta = match fs::symlink_metadata(&pointer) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Diagnostic::io(&pointer, e)),
    };
    if !meta.file_type().is_symlink() {
        return Err(Diagnostic::new(format!(
            "refusing unowned package pointer {}",
            pointer.display()
        )));
    }
    let target = fs::read_link(&pointer).map_err(|e| Diagnostic::io(&pointer, e))?;
    if target.components().count() != 1
        || !matches!(target.components().next(), Some(Component::Normal(_)))
        || !target.to_str().is_some_and(|s| s.starts_with(&prefix(p)))
    {
        return Err(Diagnostic::new("invalid package pointer"));
    }
    let generation = p.output_dir.join(target);
    let manifest = validate_generation(p, &generation)?;
    Ok(Some((generation, manifest)))
}
fn tool(project: &ResolvedProject, executable: &str, arguments: Vec<String>) -> Result<()> {
    validate_llvm_executable(&project.toolchain, executable, &project.root)?;
    let command = BuildCommand {
        executable: executable.into(),
        argv: arguments
            .into_iter()
            .map(|v| Sourced::derived(v, "native UF2 packaging"))
            .collect(),
        cwd: project.root.clone(),
        environment: Default::default(),
        stdout: None,
        stderr: None,
    };
    let output = process(&command)?
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Diagnostic::new(format!("cannot run {executable}: {e}")))?;
    if !output.status.success() {
        return Err(Diagnostic::new(format!(
            "{executable} failed: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
#[cfg(unix)]
pub fn package_uf2(project: &ResolvedProject, plan: &BuildPlan) -> Result<PackageReport> {
    plan.validate()?;
    let p = plan
        .publication
        .as_ref()
        .ok_or_else(|| Diagnostic::new("missing firmware publication"))?;
    let kind = project
        .board
        .packaging
        .ok_or_else(|| Diagnostic::new("board has no package format"))?;
    if !matches!(kind, PackagingKind::SonataLegacy | PackagingKind::SonataV1) {
        return Err(Diagnostic::new("board has no supported UF2 layout"));
    }
    let _build = BuildLock::acquire(&plan.build_dir)?;
    check_build_identity(plan, false)?;
    let _publication = publication::PublicationLock::acquire(p)?;
    publication::preflight(plan, p)?;
    ensure_plan_fresh(plan)?;
    recover(p)?;
    let source = read_regular(&p.elf)?;
    let source_sha256 = digest(&source);
    let stage = p.output_dir.join(format!(
        "{}{}-{}",
        prefix(p),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&stage).map_err(|e| Diagnostic::io(&stage, e))?;
    write_synced(&stage.join(".claim"), claim(p).as_bytes())?;
    sync_dir(&stage)?;
    let result = (|| -> Result<PackageReport> {
        let intermediate = stage.join("input.bin");
        let input = path_to_string(&p.elf)?;
        let output = path_to_string(&intermediate)?;
        match kind {
            PackagingKind::SonataLegacy => tool(
                project,
                &project.toolchain.object_copy,
                vec!["-O".into(), "binary".into(), input, output],
            )?,
            PackagingKind::SonataV1 => tool(
                project,
                &project.toolchain.strip,
                vec![input, "-o".into(), output],
            )?,
            PackagingKind::MicrosoftSafe => {
                return Err(Diagnostic::new("board has no supported UF2 layout"));
            }
        }
        let mut bytes = read_regular(&intermediate)?;
        if bytes.is_empty() || bytes.len() > MAX_IMAGE {
            return Err(Diagnostic::new("package input is empty or exceeds 64 MiB"));
        }
        if matches!(kind, PackagingKind::SonataV1) {
            use object::Object;
            let object = object::File::parse(bytes.as_slice())
                .map_err(|e| Diagnostic::new(format!("invalid stripped ELF: {e}")))?;
            if object.format() != object::BinaryFormat::Elf
                || object.architecture() != object::Architecture::Riscv32
                || object.kind() != object::ObjectKind::Executable
            {
                return Err(Diagnostic::new(
                    "stripped firmware is not executable RISC-V ELF",
                ));
            }
        }
        let padded = bytes
            .len()
            .checked_add(255)
            .ok_or_else(|| Diagnostic::new("package size overflow"))?
            / 256
            * 256;
        bytes.resize(padded, 0);
        let slots: Vec<(String, u32, Option<u32>)> = match kind {
            PackagingKind::SonataLegacy => {
                if project.board.instruction_memory.start != 0x0010_1000
                    || u64::try_from(bytes.len()).unwrap_or(u64::MAX)
                        > u64::from(project.board.instruction_memory.length)
                {
                    return Err(Diagnostic::new(
                        "legacy Sonata image does not fit the board instruction memory",
                    ));
                }
                vec![(format!("{}.uf2", p.firmware_name), 0x0010_1000, None)]
            }
            PackagingKind::SonataV1 => vec![
                (
                    format!("{}.slot1.uf2", p.firmware_name),
                    0,
                    Some(0x6ce2_9e60),
                ),
                (
                    format!("{}.slot2.uf2", p.firmware_name),
                    0x1000_0000,
                    Some(0x6ce2_9e60),
                ),
                (
                    format!("{}.slot3.uf2", p.firmware_name),
                    0x2000_0000,
                    Some(0x6ce2_9e60),
                ),
            ],
            PackagingKind::MicrosoftSafe => unreachable!(),
        };
        let mut files = Vec::new();
        for (name, address, family_id) in slots {
            let layout = Uf2Layout {
                family_id,
                payload_size: NonZeroU16::new(256)
                    .ok_or_else(|| Diagnostic::new("invalid UF2 block size"))?,
            };
            let image = FlashImage {
                regions: vec![FlashRegion {
                    address,
                    bytes: bytes.clone(),
                }],
            };
            let mut encoded = Vec::new();
            let report = encode_uf2(&image, &layout, &mut encoded)
                .map_err(|e| Diagnostic::new(e.to_string()))?;
            if validate_uf2(&encoded, &layout).map_err(|e| Diagnostic::new(e.to_string()))?
                != report
            {
                return Err(Diagnostic::new("UF2 validation mismatch"));
            }
            write_synced(&stage.join(&name), &encoded)?;
            files.push(ManifestFile {
                name,
                sha256: digest(&encoded),
            });
        }
        fs::remove_file(&intermediate).map_err(|e| Diagnostic::io(&intermediate, e))?;
        let manifest = Manifest {
            version: 1,
            owner: p.owner.clone(),
            source_sha256,
            files,
        };
        write_synced(
            &stage.join("manifest.json"),
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        sync_dir(&stage)?;
        sync_dir(&p.output_dir)?;
        publish_pointer(p, &stage)?;
        let (_, manifest) =
            selected(p)?.ok_or_else(|| Diagnostic::new("package pointer disappeared"))?;
        Ok(PackageReport {
            board: project.board.name.clone(),
            format: "uf2",
            artifacts: manifest
                .files
                .iter()
                .map(|f| pointer(p).join(&f.name))
                .collect(),
        })
    })();
    if result.is_err()
        && selected(p)
            .ok()
            .flatten()
            .is_none_or(|(current, _)| current != stage)
    {
        cleanup_stage(p, &stage);
    }
    result
}
#[cfg(test)]
fn test_barrier(phase: &str) {
    if std::env::var("CHERIOT_TEST_PACKAGE_BARRIER")
        .ok()
        .as_deref()
        == Some(phase)
    {
        println!("CHERIOT_PACKAGE_BARRIER {phase}");
        let _ = std::io::stdout().flush();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
#[cfg(not(test))]
fn test_barrier(_: &str) {}
#[cfg(unix)]
fn publish_pointer(p: &Publication, stage: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;
    validate_generation(p, stage)?;
    let temporary = p.output_dir.join(format!(
        ".{}.cheriot-package-pointer-{}-{}",
        p.firmware_name,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    symlink(
        stage
            .file_name()
            .ok_or_else(|| Diagnostic::new("invalid package generation name"))?,
        &temporary,
    )
    .map_err(|e| Diagnostic::io(&temporary, e))?;
    test_barrier("before-commit");
    if let Err(e) = fs::rename(&temporary, pointer(p)) {
        let _ = fs::remove_file(&temporary);
        return Err(Diagnostic::io(&temporary, e));
    }
    test_barrier("after-commit");
    sync_dir(&p.output_dir)?;
    mark_committed(p, stage)?;
    Ok(())
}
fn allowed_names(p: &Publication) -> Vec<String> {
    vec![
        format!("{}.uf2", p.firmware_name),
        format!("{}.slot1.uf2", p.firmware_name),
        format!("{}.slot2.uf2", p.firmware_name),
        format!("{}.slot3.uf2", p.firmware_name),
    ]
}
fn cleanup_stage(p: &Publication, dir: &Path) {
    let mut known = allowed_names(p);
    known.extend([
        "input.bin".into(),
        "manifest.json".into(),
        ".claim".into(),
        ".committed".into(),
    ]);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { return };
        if !entry.file_type().is_ok_and(|t| t.is_file())
            || !known
                .iter()
                .any(|n| entry.file_name() == std::ffi::OsStr::new(n))
        {
            return;
        }
        paths.push(entry.path());
    }
    for path in paths {
        if fs::remove_file(path).is_err() {
            return;
        }
    }
    let _ = fs::remove_dir(dir);
}
fn mark_committed(p: &Publication, dir: &Path) -> Result<()> {
    let marker = dir.join(".committed");
    match fs::symlink_metadata(&marker) {
        Ok(m) if m.is_file() && read_regular(&marker)? == claim(p).as_bytes() => (),
        Ok(_) => return Err(Diagnostic::new("invalid package commit marker")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_synced(&marker, claim(p).as_bytes())?;
            sync_dir(dir)?;
        }
        Err(e) => return Err(Diagnostic::io(&marker, e)),
    }
    Ok(())
}
fn recover(p: &Publication) -> Result<()> {
    let current = selected(p)?.map(|(dir, _)| dir);
    if let Some(dir) = &current {
        mark_committed(p, dir)?;
    }
    for entry in fs::read_dir(&p.output_dir).map_err(|e| Diagnostic::io(&p.output_dir, e))? {
        let entry = entry.map_err(|e| Diagnostic::io(&p.output_dir, e))?;
        let dir = entry.path();
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|s| s.starts_with(&prefix(p)))
            || !entry
                .file_type()
                .map_err(|e| Diagnostic::io(&dir, e))?
                .is_dir()
            || current.as_ref() == Some(&dir)
        {
            continue;
        }
        let claim_file = dir.join(".claim");
        if read_regular(&claim_file).ok().as_deref() == Some(claim(p).as_bytes())
            && !dir.join(".committed").exists()
        {
            if dir.join("manifest.json").exists() {
                validate_generation(p, &dir)?;
            }
            let pointer_prefix = format!(".{}.cheriot-package-pointer-", p.firmware_name);
            for entry in
                fs::read_dir(&p.output_dir).map_err(|e| Diagnostic::io(&p.output_dir, e))?
            {
                let entry = entry.map_err(|e| Diagnostic::io(&p.output_dir, e))?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with(&pointer_prefix))
                    && entry
                        .file_type()
                        .map_err(|e| Diagnostic::io(&entry.path(), e))?
                        .is_symlink()
                    && fs::read_link(entry.path()).ok().as_deref() == dir.file_name().map(Path::new)
                {
                    fs::remove_file(entry.path()).map_err(|e| Diagnostic::io(&entry.path(), e))?;
                }
            }
            cleanup_stage(p, &dir);
        }
    }
    sync_dir(&p.output_dir)
}
pub(super) fn clean_for(p: &Publication) -> Result<usize> {
    let current = selected(p)?;
    let mut dirs = Vec::new();
    let entries = match fs::read_dir(&p.output_dir) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(Diagnostic::io(&p.output_dir, e)),
    };
    for entry in entries {
        let entry = entry.map_err(|e| Diagnostic::io(&p.output_dir, e))?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|s| s.starts_with(&prefix(p)))
            || !entry
                .file_type()
                .map_err(|e| Diagnostic::io(&entry.path(), e))?
                .is_dir()
        {
            continue;
        }
        let dir = entry.path();
        let manifest_path = dir.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = validate_generation(p, &dir)?;
        let allowed = allowed_names(p);
        if manifest.files.iter().any(|f| !allowed.contains(&f.name)) {
            return Err(Diagnostic::new(
                "package manifest has unexpected artifact names",
            ));
        }
        for item in fs::read_dir(&dir).map_err(|e| Diagnostic::io(&dir, e))? {
            let item = item.map_err(|e| Diagnostic::io(&dir, e))?;
            if !item
                .file_type()
                .map_err(|e| Diagnostic::io(&item.path(), e))?
                .is_file()
                || (!["manifest.json", ".claim", ".committed"]
                    .iter()
                    .any(|n| item.file_name() == std::ffi::OsStr::new(n))
                    && !manifest
                        .files
                        .iter()
                        .any(|f| item.file_name() == std::ffi::OsStr::new(&f.name)))
            {
                return Err(Diagnostic::new(format!(
                    "refusing unexpected package entry {}",
                    item.path().display()
                )));
            }
        }
        dirs.push((dir, manifest));
    }
    let mut count = 0;
    if current.is_some() {
        let path = pointer(p);
        fs::remove_file(&path).map_err(|e| Diagnostic::io(&path, e))?;
        sync_dir(&p.output_dir)?;
        count += 1;
    }
    for (dir, manifest) in dirs {
        for file in manifest.files {
            let path = dir.join(file.name);
            fs::remove_file(&path).map_err(|e| Diagnostic::io(&path, e))?;
            count += 1;
        }
        for name in ["manifest.json", ".claim", ".committed"] {
            let marker = dir.join(name);
            match fs::remove_file(&marker) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(Diagnostic::io(&marker, e)),
            }
        }
        fs::remove_dir(&dir).map_err(|e| Diagnostic::io(&dir, e))?;
        count += 1;
    }
    Ok(count)
}
#[cfg(not(unix))]
pub fn package_uf2(_: &ResolvedProject, _: &BuildPlan) -> Result<PackageReport> {
    Err(Diagnostic::new("UF2 package publication requires Unix"))
}

#[cfg(all(test, unix))]
mod tests {
    const MODULE: &str = {
        let path = module_path!().as_bytes();
        let mut start = 0;
        while path[start] != b':' {
            start += 1;
        }
        match str::from_utf8(path.split_at(start + 2).1) {
            Ok(rest) => rest,
            Err(_) => panic!("module path"),
        }
    };
    use super::*;
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    fn publication(root: &Path) -> Publication {
        let build = root.join("build");
        let output = root.join("out");
        fs::create_dir_all(&build).unwrap();
        fs::create_dir_all(&output).unwrap();
        let owner = "package-test";
        let id = crate::build::build_graph::calculate_fingerprint(owner.as_bytes());
        Publication::new(&build, &output, "firmware", &id, owner, false)
    }
    fn stage(p: &Publication, tag: &str) -> PathBuf {
        let dir = p.output_dir.join(format!("{}{tag}", prefix(p)));
        fs::create_dir(&dir).unwrap();
        write_synced(&dir.join(".claim"), claim(p).as_bytes()).unwrap();
        let mut files = Vec::new();
        for slot in 1..=2 {
            let name = format!("firmware.slot{slot}.uf2");
            let data = format!("{tag} slot{slot}");
            write_synced(&dir.join(&name), data.as_bytes()).unwrap();
            files.push(ManifestFile {
                name,
                sha256: digest(data.as_bytes()),
            });
        }
        let manifest = Manifest {
            version: 1,
            owner: p.owner.clone(),
            source_sha256: digest(b"source"),
            files,
        };
        write_synced(
            &dir.join("manifest.json"),
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        sync_dir(&dir).unwrap();
        dir
    }
    fn publish_test(p: &Publication, tag: &str) {
        let _lock = publication::PublicationLock::acquire(p).unwrap();
        recover(p).unwrap();
        let dir = stage(p, tag);
        publish_pointer(p, &dir).unwrap();
    }
    fn expect(p: &Publication, tag: &str) {
        let (dir, _) = selected(p).unwrap().unwrap();
        for slot in 1..=2 {
            assert_eq!(
                fs::read_to_string(dir.join(format!("firmware.slot{slot}.uf2"))).unwrap(),
                format!("{tag} slot{slot}")
            );
        }
    }
    #[test]
    fn package_child() {
        let Some(root) = std::env::var_os("CHERIOT_TEST_PACKAGE_ROOT") else {
            return;
        };
        let p = publication(Path::new(&root));
        publish_test(&p, "new");
    }
    fn kill_at(root: &Path, phase: &str) {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{}::package_child", MODULE),
                "--nocapture",
            ])
            .env("CHERIOT_TEST_PACKAGE_ROOT", root)
            .env("CHERIOT_TEST_PACKAGE_BARRIER", phase)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        let mut ready = false;
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("CHERIOT_PACKAGE_BARRIER") {
                ready = true;
                break;
            }
            line.clear();
        }
        if !ready {
            let _ = child.kill();
            let _ = child.wait();
            panic!("package child did not reach {phase}");
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    #[test]
    fn interrupted_package_commit_never_mixes_slots_and_recovers_orphans() {
        let root = tempfile::tempdir().unwrap();
        let p = publication(root.path());
        publish_test(&p, "old");
        kill_at(root.path(), "before-commit");
        expect(&p, "old");
        {
            let _lock = publication::PublicationLock::acquire(&p).unwrap();
            recover(&p).unwrap();
        }
        assert!(!p.output_dir.join(format!("{}new", prefix(&p))).exists());
        kill_at(root.path(), "after-commit");
        expect(&p, "new");
        {
            let _lock = publication::PublicationLock::acquire(&p).unwrap();
            recover(&p).unwrap();
        }
        let (current, _) = selected(&p).unwrap().unwrap();
        assert!(current.join(".committed").is_file());
    }
}
