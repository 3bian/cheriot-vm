// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::{guarded_output, write_if_changed};
use crate::build::build_graph::{BuildPlan, MARKER_VERSION, Publication};
use crate::config::diagnostic::{Diagnostic, Result};
use object::Object;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn current(p: &Publication) -> PathBuf {
    p.output_dir
        .join(format!(".{}.cheriot-current", p.firmware_name))
}
fn prefix(p: &Publication) -> String {
    format!(".{}.cheriot-generation-", p.firmware_name)
}
pub(super) fn claim(p: &Publication) -> String {
    format!("{MARKER_VERSION}\n{}\n", p.owner)
}
fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| Diagnostic::io(path, e))?;
    let mut h = Sha256::new();
    let mut n = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let count = file.read(&mut buf).map_err(|e| Diagnostic::io(path, e))?;
        if count == 0 {
            break;
        }
        h.update(&buf[..count]);
        n = n
            .checked_add(count as u64)
            .ok_or_else(|| Diagnostic::new("artifact too large"))?;
    }
    Ok(format!("{:x} {n}", h.finalize()))
}
fn write_synced(path: &Path, content: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Diagnostic::io(path, e))?;
    f.write_all(content).map_err(|e| Diagnostic::io(path, e))?;
    f.sync_all().map_err(|e| Diagnostic::io(path, e))
}
pub(super) fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| Diagnostic::io(path, e))
}
fn no_symlinks(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Diagnostic::new(format!(
                    "refusing symlink in output path: {}",
                    parent.display()
                )));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(Diagnostic::io(parent, e)),
        }
    }
    Ok(())
}
fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(true),
        Ok(_) => Err(Diagnostic::new(format!(
            "output is not a regular file: {}",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Diagnostic::io(path, e)),
    }
}
fn marker_text(p: &Publication, elf: &str, report: Option<&str>) -> String {
    format!(
        "{MARKER_VERSION}\n{}\n{elf}\n{}\n",
        p.owner,
        report.unwrap_or("-")
    )
}
pub(super) fn validate_generation(p: &Publication, generation: &Path) -> Result<()> {
    no_symlinks(generation)?;
    let claim_path = generation.join(".claim");
    if !regular(&claim_path)?
        || fs::read_to_string(&claim_path).map_err(|e| Diagnostic::io(&claim_path, e))? != claim(p)
    {
        return Err(Diagnostic::new(format!(
            "unowned output generation {}",
            generation.display()
        )));
    }
    let marker = generation.join(".owner");
    if !regular(&marker)? {
        return Err(Diagnostic::new(format!(
            "incomplete output generation {}",
            generation.display()
        )));
    }
    let contents = fs::read_to_string(&marker).map_err(|e| Diagnostic::io(&marker, e))?;
    let lines: Vec<_> = contents.lines().collect();
    if lines.len() != 4 || lines[0] != MARKER_VERSION || lines[1] != p.owner {
        return Err(Diagnostic::new(format!(
            "invalid output marker {}",
            marker.display()
        )));
    }
    let elf = generation.join(format!("{}.elf", p.firmware_name));
    if !regular(&elf)? || hash(&elf)? != lines[2] {
        return Err(Diagnostic::new(format!(
            "modified or missing firmware {}",
            elf.display()
        )));
    }
    let report = generation.join(format!("{}.json", p.firmware_name));
    if lines[3] != "-" {
        if !regular(&report)? || hash(&report)? != lines[3] {
            return Err(Diagnostic::new(format!(
                "modified or missing report {}",
                report.display()
            )));
        }
    } else if regular(&report)? {
        return Err(Diagnostic::new(format!(
            "unexpected report {}",
            report.display()
        )));
    }
    Ok(())
}
pub(super) fn selected_generation(p: &Publication) -> Result<Option<PathBuf>> {
    no_symlinks(&p.output_dir)?;
    let pointer = current(p);
    let metadata = match fs::symlink_metadata(&pointer) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Diagnostic::io(&pointer, e)),
    };
    if !metadata.file_type().is_symlink() {
        return Err(Diagnostic::new(format!(
            "refusing unowned publication pointer {}",
            pointer.display()
        )));
    }
    let target = fs::read_link(&pointer).map_err(|e| Diagnostic::io(&pointer, e))?;
    if target.components().count() != 1
        || !matches!(target.components().next(), Some(Component::Normal(_)))
        || !target.to_str().is_some_and(|s| s.starts_with(&prefix(p)))
    {
        return Err(Diagnostic::new(format!(
            "invalid publication pointer {}",
            pointer.display()
        )));
    }
    let generation = p.output_dir.join(target);
    validate_generation(p, &generation)?;
    Ok(Some(generation))
}
fn is_public_link(link: &Path, target: &Path) -> Result<bool> {
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let actual = fs::read_link(link).map_err(|e| Diagnostic::io(link, e))?;
            Ok(actual == target)
        }
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Diagnostic::io(link, e)),
    }
}
#[cfg(unix)]
fn create_public_link(link: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).map_err(|e| Diagnostic::io(link, e))
}
#[cfg(not(unix))]
fn create_public_link(_: &Path, _: &Path) -> Result<()> {
    Err(Diagnostic::new("atomic build publication requires Unix"))
}
fn public_links(p: &Publication, create: bool) -> Result<()> {
    for (index, (link, target)) in p.public_links().into_iter().enumerate() {
        let wanted = index == 0 || p.report_enabled;
        if is_public_link(&link, &target)? {
            if !wanted && create {
                fs::remove_file(&link).map_err(|e| Diagnostic::io(&link, e))?;
            }
        } else if fs::symlink_metadata(&link).is_ok() {
            if wanted {
                return Err(Diagnostic::new(format!(
                    "refusing to replace {} with a firmware link; move it or choose another --output-dir",
                    link.display()
                )));
            }
        } else if wanted && create {
            create_public_link(&link, &target)?;
        }
    }
    Ok(())
}
fn remove_public_links(p: &Publication) -> Result<usize> {
    let mut count = 0;
    for (link, target) in p.public_links() {
        if is_public_link(&link, &target)? {
            fs::remove_file(&link).map_err(|e| Diagnostic::io(&link, e))?;
            count += 1;
        }
    }
    Ok(count)
}
fn validate_registry(plan: &BuildPlan, p: &Publication) -> Result<()> {
    guarded_output(&p.registry, &plan.build_dir)?;
    let contents = match fs::read(&p.registry) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Diagnostic::io(&p.registry, e)),
    };
    let mut alternate = p.clone();
    alternate.report_enabled = !p.report_enabled;
    if contents != p.registry_content()?.as_bytes()
        && contents != alternate.registry_content()?.as_bytes()
    {
        return Err(Diagnostic::new(
            "publication registry belongs to another destination",
        ));
    }
    Ok(())
}
pub(super) fn preflight(plan: &BuildPlan, p: &Publication) -> Result<()> {
    for path in [&p.elf, &p.report, &p.marker, &p.registry] {
        if plan
            .operations
            .iter()
            .any(|o| o.inputs.contains(path) && plan.artifact_producer(path).is_none())
        {
            return Err(Diagnostic::new(format!(
                "final output would overwrite an input: {}",
                path.display()
            )));
        }
    }
    validate_registry(plan, p)?;
    selected_generation(p)?;
    public_links(p, false)?;
    Ok(())
}
pub(super) struct PublicationLock {
    _lock: crate::file_lock::FileLock,
}
impl PublicationLock {
    pub(super) fn acquire(p: &Publication) -> Result<Self> {
        no_symlinks(&p.output_dir)?;
        fs::create_dir_all(&p.output_dir).map_err(|e| Diagnostic::io(&p.output_dir, e))?;
        let path = p.lock_path();
        no_symlinks(&path)?;
        let lock = crate::file_lock::FileLock::try_exclusive(&path).map_err(|e| {
            Diagnostic::new(format!(
                "final output is locked or inaccessible: {}: {e}",
                path.display()
            ))
        })?;
        super::recovery::recover_incomplete(p)?;
        Ok(Self { _lock: lock })
    }
}
pub(super) fn managed_entries(p: &Publication) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    match fs::read_dir(&p.output_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| Diagnostic::io(&p.output_dir, e))?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with(&prefix(p)))
                {
                    found.push(entry.path());
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(Diagnostic::io(&p.output_dir, e)),
    }
    Ok(found)
}
pub(super) fn remove_generation(p: &Publication, path: &Path) -> Result<usize> {
    no_symlinks(path)?;
    let known = [
        ".claim".to_owned(),
        ".owner".to_owned(),
        ".committed".to_owned(),
        format!("{}.elf", p.firmware_name),
        format!("{}.json", p.firmware_name),
    ];
    for entry in fs::read_dir(path).map_err(|e| Diagnostic::io(path, e))? {
        let entry = entry.map_err(|e| Diagnostic::io(path, e))?;
        if !known
            .iter()
            .any(|s| entry.file_name() == std::ffi::OsStr::new(s))
            || !entry
                .file_type()
                .map_err(|e| Diagnostic::io(&entry.path(), e))?
                .is_file()
        {
            return Err(Diagnostic::new(format!(
                "refusing to remove unexpected generation entry {}",
                entry.path().display()
            )));
        }
    }
    let mut count = 0;
    for name in known {
        let file = path.join(name);
        match fs::remove_file(&file) {
            Ok(()) => count += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(Diagnostic::io(&file, e)),
        }
    }
    fs::remove_dir(path).map_err(|e| Diagnostic::io(path, e))?;
    Ok(count)
}
pub(super) fn mark_committed(p: &Publication, path: &Path) -> Result<()> {
    let marker = path.join(".committed");
    match fs::symlink_metadata(&marker) {
        Ok(meta)
            if meta.is_file()
                && fs::read_to_string(&marker).ok().as_deref() == Some(claim(p).as_str()) => {}
        Ok(_) => {
            return Err(Diagnostic::new(format!(
                "invalid generation commit marker {}",
                marker.display()
            )));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_synced(&marker, claim(p).as_bytes())?;
            sync_dir(path)?;
        }
        Err(e) => return Err(Diagnostic::io(&marker, e)),
    }
    Ok(())
}
fn new_generation(p: &Publication) -> Result<PathBuf> {
    for _ in 0..64 {
        let path = p.output_dir.join(format!(
            "{}{}-{}",
            prefix(p),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                write_synced(&path.join(".claim"), claim(p).as_bytes())?;
                sync_dir(&path)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(Diagnostic::io(&path, e)),
        }
    }
    Err(Diagnostic::new("cannot allocate an output generation"))
}
#[cfg(test)]
fn test_barrier(phase: &str) {
    if std::env::var("CHERIOT_TEST_PUBLISH_BARRIER")
        .ok()
        .as_deref()
        == Some(phase)
    {
        println!("CHERIOT_PUBLISH_BARRIER {phase}");
        let _ = std::io::stdout().flush();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
#[cfg(not(test))]
fn test_barrier(_: &str) {}
#[cfg(unix)]
pub(super) fn publish(plan: &BuildPlan, inputs: &[PathBuf]) -> Result<()> {
    use std::os::unix::fs::symlink;
    let p = plan
        .publication
        .as_ref()
        .ok_or_else(|| Diagnostic::new("missing publication destination"))?;
    selected_generation(p)?;
    let generation = new_generation(p)?;
    let elf = generation.join(format!("{}.elf", p.firmware_name));
    fs::copy(&inputs[0], &elf).map_err(|e| Diagnostic::io(&inputs[0], e))?;
    let bytes = fs::read(&elf).map_err(|e| Diagnostic::io(&elf, e))?;
    let object = object::File::parse(bytes.as_slice())
        .map_err(|e| Diagnostic::new(format!("invalid firmware ELF {}: {e}", elf.display())))?;
    if object.format() != object::BinaryFormat::Elf
        || object.architecture() != object::Architecture::Riscv32
        || !object.is_little_endian()
        || object.kind() != object::ObjectKind::Executable
    {
        return Err(Diagnostic::new(
            "firmware must be a little-endian executable RISC-V 32-bit ELF",
        ));
    }
    File::open(&elf)
        .and_then(|f| f.sync_all())
        .map_err(|e| Diagnostic::io(&elf, e))?;
    let elf_hash = hash(&elf)?;
    let report_hash = if p.report_enabled {
        let report = generation.join(format!("{}.json", p.firmware_name));
        fs::copy(&inputs[1], &report).map_err(|e| Diagnostic::io(&inputs[1], e))?;
        File::open(&report)
            .and_then(|f| f.sync_all())
            .map_err(|e| Diagnostic::io(&report, e))?;
        let report_data = fs::read(&report).map_err(|e| Diagnostic::io(&report, e))?;
        serde_json::from_slice::<serde_json::Value>(&report_data).map_err(|e| {
            Diagnostic::new(format!(
                "invalid compartment report {}: {e}",
                report.display()
            ))
        })?;
        Some(hash(&report)?)
    } else {
        None
    };
    write_synced(
        &generation.join(".owner"),
        marker_text(p, &elf_hash, report_hash.as_deref()).as_bytes(),
    )?;
    sync_dir(&generation)?;
    sync_dir(&p.output_dir)?;
    validate_registry(plan, p)?;
    write_if_changed(&p.registry, p.registry_content()?.as_bytes())?;
    sync_dir(&plan.build_dir)?;
    let temporary = p.output_dir.join(format!(
        ".{}.cheriot-pointer-{}-{}",
        p.firmware_name,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let target = generation
        .file_name()
        .ok_or_else(|| Diagnostic::new("generation has no name"))?;
    symlink(target, &temporary).map_err(|e| Diagnostic::io(&temporary, e))?;
    test_barrier("before-commit");
    let result = fs::rename(&temporary, current(p)).map_err(|e| Diagnostic::io(&temporary, e));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    test_barrier("after-commit");
    sync_dir(&p.output_dir)?;
    mark_committed(p, &generation)?;
    selected_generation(p)?;
    public_links(p, true)?;
    sync_dir(&p.output_dir)?;
    Ok(())
}
#[cfg(not(unix))]
pub(super) fn publish(_: &BuildPlan, _: &[PathBuf]) -> Result<()> {
    Err(Diagnostic::new("atomic build publication requires Unix"))
}
pub(super) fn clean(p: &Publication) -> Result<usize> {
    let selected = selected_generation(p)?;
    let mut generations = Vec::new();
    for dir in managed_entries(p)? {
        if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let c = dir.join(".claim");
        if fs::symlink_metadata(&c).is_ok_and(|m| m.is_file())
            && fs::read_to_string(&c).ok().as_deref() == Some(claim(p).as_str())
        {
            if dir.join(".owner").exists() {
                validate_generation(p, &dir)?;
            }
            generations.push(dir);
        }
    }
    let mut count = super::package::clean_for(p)?;
    count += remove_public_links(p)?;
    if selected.is_some() {
        let pointer = current(p);
        fs::remove_file(&pointer).map_err(|e| Diagnostic::io(&pointer, e))?;
        sync_dir(&p.output_dir)?;
        count += 1;
    }
    for dir in generations {
        count += remove_generation(p, &dir)?;
    }
    if p.output_dir.exists() {
        sync_dir(&p.output_dir)?;
    }
    Ok(count)
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
        collections::BTreeSet,
        io::BufRead,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };
    fn plan(root: &Path) -> BuildPlan {
        let build = root.join("build");
        let output = root.join("output");
        fs::create_dir_all(&build).unwrap();
        fs::create_dir_all(&output).unwrap();
        let owner = "test-project";
        let identity = crate::build::build_graph::calculate_fingerprint(owner.as_bytes());
        let publication = Publication::new(&build, &output, "firmware", &identity, owner, true);
        BuildPlan {
            schema: "test".into(),
            profile: "test".into(),
            notes: vec![],
            toolchain: None,
            build_dir: build,
            state_key: Some(identity),
            state_owner: Some(owner.into()),
            publication: Some(publication),
            operations: vec![],
            probes: BTreeSet::new(),
        }
    }
    fn inputs(root: &Path, tag: &str) -> Vec<PathBuf> {
        let elf = root.join(format!("{tag}.elf"));
        let report = root.join(format!("{tag}.json"));
        let mut bytes = include_bytes!("../../tests/fixtures/hello_world.elf").to_vec();
        bytes.extend_from_slice(tag.as_bytes());
        fs::write(&elf, bytes).unwrap();
        fs::write(&report, format!("{{\"tag\":\"{tag}\"}}")).unwrap();
        vec![elf, report]
    }
    fn publish_one(plan: &BuildPlan, inputs: &[PathBuf]) {
        let _build = super::super::build_lock::BuildLock::acquire(&plan.build_dir).unwrap();
        let p = plan.publication.as_ref().unwrap();
        let _lock = PublicationLock::acquire(p).unwrap();
        preflight(plan, p).unwrap();
        publish(plan, inputs).unwrap();
    }
    fn expect_pair(plan: &BuildPlan, tag: &str) {
        let p = plan.publication.as_ref().unwrap();
        let bytes = fs::read(&p.elf).unwrap();
        assert!(bytes.ends_with(tag.as_bytes()));
        assert_eq!(
            fs::read_to_string(&p.report).unwrap(),
            format!("{{\"tag\":\"{tag}\"}}")
        );
        selected_generation(p).unwrap().unwrap();
    }
    #[test]
    fn generation_publish_checks_integrity_and_preserves_owned_outputs() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan(root.path());
        publish_one(&plan, &inputs(root.path(), "old"));
        expect_pair(&plan, "old");
        publish_one(&plan, &inputs(root.path(), "new"));
        expect_pair(&plan, "new");
        let p = plan.publication.as_ref().unwrap();
        assert_eq!(managed_entries(p).unwrap().len(), 2);
        fs::write(&p.report, b"changed").unwrap();
        assert!(selected_generation(p).is_err());
    }
    #[test]
    fn stable_paths_follow_the_current_generation_and_refuse_foreign_files() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan(root.path());
        let p = plan.publication.as_ref().unwrap();
        publish_one(&plan, &inputs(root.path(), "old"));
        assert!(fs::read(p.public_elf()).unwrap().ends_with(b"old"));
        assert_eq!(
            fs::read_to_string(p.public_report()).unwrap(),
            "{\"tag\":\"old\"}"
        );
        publish_one(&plan, &inputs(root.path(), "new"));
        assert!(fs::read(p.public_elf()).unwrap().ends_with(b"new"));
        assert_eq!(
            fs::read_to_string(p.public_report()).unwrap(),
            "{\"tag\":\"new\"}"
        );
        assert_eq!(remove_public_links(p).unwrap(), 2);
        assert!(fs::symlink_metadata(p.public_elf()).is_err());
        fs::write(p.public_elf(), b"mine").unwrap();
        assert!(preflight(&plan, p).is_err());
        assert_eq!(fs::read(p.public_elf()).unwrap(), b"mine");
    }
    #[test]
    fn invalid_firmware_cannot_replace_a_committed_generation() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan(root.path());
        publish_one(&plan, &inputs(root.path(), "old"));
        let mut invalid = inputs(root.path(), "invalid");
        fs::write(&invalid[0], b"not an ELF").unwrap();
        let p = plan.publication.as_ref().unwrap();
        {
            let _lock = PublicationLock::acquire(p).unwrap();
            assert!(publish(&plan, &invalid).is_err());
        }
        expect_pair(&plan, "old");
        {
            let _lock = PublicationLock::acquire(p).unwrap();
        }
        expect_pair(&plan, "old");
        invalid.clear();
        assert_eq!(managed_entries(p).unwrap().len(), 1);
    }
    #[test]
    fn publication_child() {
        let Some(root) = std::env::var_os("CHERIOT_TEST_PUBLISH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let plan = plan(&root);
        publish_one(&plan, &inputs(&root, "new"));
    }
    fn kill_at(root: &Path, phase: &str) {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{}::publication_child", MODULE),
                "--nocapture",
            ])
            .env("CHERIOT_TEST_PUBLISH_ROOT", root)
            .env("CHERIOT_TEST_PUBLISH_BARRIER", phase)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        let mut ready = false;
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("CHERIOT_PUBLISH_BARRIER") {
                ready = true;
                break;
            }
            line.clear();
        }
        if !ready {
            let _ = child.kill();
            let _ = child.wait();
            panic!("publication child did not reach {phase}");
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    #[test]
    fn interrupted_publish_exposes_a_single_generation_and_recovers_orphan() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan(root.path());
        let p = plan.publication.as_ref().unwrap();
        publish_one(&plan, &inputs(root.path(), "old"));
        kill_at(root.path(), "before-commit");
        expect_pair(&plan, "old");
        assert_eq!(managed_entries(p).unwrap().len(), 2);
        {
            let _lock = PublicationLock::acquire(p).unwrap();
        }
        assert_eq!(managed_entries(p).unwrap().len(), 1);
        kill_at(root.path(), "after-commit");
        expect_pair(&plan, "new");
        {
            let _lock = PublicationLock::acquire(p).unwrap();
        }
        expect_pair(&plan, "new");
        assert!(
            selected_generation(p)
                .unwrap()
                .unwrap()
                .join(".committed")
                .is_file()
        );
    }
    #[test]
    fn lock_child() {
        let Some(root) = std::env::var_os("CHERIOT_TEST_BUILD_LOCK_ROOT") else {
            return;
        };
        let path = PathBuf::from(root);
        let _lock = super::super::build_lock::BuildLock::acquire(&path).unwrap();
        let mut grandchild = Command::new("sleep")
            .arg("2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        thread::spawn(move || {
            let _ = grandchild.wait();
        });
        println!("CHERIOT_BUILD_LOCKED");
        std::io::stdout().flush().unwrap();
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    #[test]
    fn lock_owner_death_and_child_exec_do_not_keep_a_build_locked() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("build");
        fs::create_dir(&dir).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("{}::lock_child", MODULE), "--nocapture"])
            .env("CHERIOT_TEST_BUILD_LOCK_ROOT", &dir)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        let mut ready = false;
        while reader.read_line(&mut line).unwrap() > 0 {
            if line.contains("CHERIOT_BUILD_LOCKED") {
                ready = true;
                break;
            }
            line.clear();
        }
        if !ready {
            let _ = child.kill();
            let _ = child.wait();
            panic!("lock child failed");
        }
        assert!(super::super::build_lock::BuildLock::acquire(&dir).is_err());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(dir.join(".cheriot.lock").is_file());
        assert!(super::super::build_lock::BuildLock::acquire(&dir).is_ok());
    }
}
