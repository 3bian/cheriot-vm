// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn diagnose_checks_the_toolchain_and_is_actionable() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    let o = run(&["diagnose", "--toolchain-dir", d, "--format", "json"]);
    assert!(!o.status.success());
    let report: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(report["project"]["status"], "skipped");
    assert_eq!(report["toolchain"]["target_supported"], false);
    assert!(
        report["toolchain"]["tools"][0]["error"]
            .as_str()
            .unwrap()
            .contains("not found")
    );

    let plain = stdout(&run(&["diagnose", "--toolchain-dir", d]));
    for section in ["Project\n", "Toolchain\n", "CHERIoT target\n", "Problems\n"] {
        assert!(plain.contains(section), "{plain}");
    }
    assert!(plain.contains("TOOL"), "{plain}");
    assert!(!plain.contains('\u{1b}'), "{plain}");
    let coloured = stdout(&run(&[
        "diagnose",
        "--toolchain-dir",
        d,
        "--color",
        "always",
    ]));
    assert!(
        coloured.contains("\u{1b}[31mmissing\u{1b}[0m"),
        "{coloured}"
    );
    let never = stdout(&run(&[
        "--color",
        "never",
        "diagnose",
        "--toolchain-dir",
        d,
    ]));
    assert_eq!(never, plain);
}
