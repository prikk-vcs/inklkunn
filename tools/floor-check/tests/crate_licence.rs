//! M63 (0.5.0's R-0): **every crate that is published carries the licence's text** (Apache-2.0 §4(a) asks a redistributor to give
//! it), and the text is the workspace's own, byte for byte. A crate's `LICENSE` is a copy of the root's (cargo packages a copy; a
//! symlink would be packaged by its content too), and `cargo package --list` is what says it is **in the package**, not only in
//! the tree. A new crate under `crates/` without its `LICENSE` fails here, before any tag.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use inklkunn_floor_check::workspace_root;

/// The published crates: every directory under `crates/` that has a `Cargo.toml` (the `tools/` crates are `publish = false`).
fn published() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(workspace_root().join("crates")).expect("crates/") {
        let dir = entry.unwrap().path();
        if dir.join("Cargo.toml").is_file() {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, dir));
        }
    }
    out.sort();
    out
}

#[test]
fn there_are_nine_published_crates_and_each_has_the_workspaces_licence_byte_for_byte() {
    let root = fs::read(workspace_root().join("LICENSE")).expect("the root LICENSE");
    let crates = published();
    assert_eq!(
        crates.len(),
        9,
        "the published crates are nine; a new one is added to this count and to release.yml's reading of the workspace: {:?}",
        crates.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
    );
    for (name, dir) in &crates {
        let copy = fs::read(dir.join("LICENSE"))
            .unwrap_or_else(|e| panic!("{name} has no LICENSE ({e}): copy the root's"));
        assert!(
            copy == root,
            "{name}'s LICENSE is not the workspace's, byte for byte"
        );
    }
}

#[test]
fn each_package_lists_the_licence() {
    for (name, _) in published() {
        let out = Command::new(env!("CARGO"))
            .args([
                "package",
                "--list",
                "--allow-dirty",
                "--locked",
                "-p",
                &name,
            ])
            .current_dir(workspace_root())
            .output()
            .expect("cargo runs");
        assert!(
            out.status.success(),
            "cargo package --list -p {name} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(
            list.lines().any(|l| l.trim() == "LICENSE"),
            "{name}'s package does not list LICENSE:\n{list}"
        );
    }
}

/// Shown able to fail: a changed byte and a missing file are what the first test refuses (the comparison is on bytes, and a
/// missing file is a panic naming the crate).
#[test]
fn the_comparison_is_byte_exact() {
    let root = fs::read(workspace_root().join("LICENSE")).expect("the root LICENSE");
    let mut changed = root.clone();
    changed.push(b'\n');
    assert!(changed != root);
    assert!(fs::read(workspace_root().join("crates/inklkunn-nonexistent/LICENSE")).is_err());
}
