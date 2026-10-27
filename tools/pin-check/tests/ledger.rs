//! RFC 010 D-6 / handoff 010 T3: the dependency ledger is derivable from the lock. Every crate in
//! `Cargo.lock` that is not a workspace member must appear in the lock inventory appendix of
//! `docs/dependency-policy.md`, and the appendix must name no crate the lock no longer has.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn lock_crates() -> BTreeSet<String> {
    let lock = fs::read_to_string(root().join("Cargo.lock")).expect("Cargo.lock");
    let mut out = BTreeSet::new();
    for line in lock.lines() {
        if let Some(name) = line
            .strip_prefix("name = \"")
            .and_then(|r| r.strip_suffix('"'))
        {
            out.insert(name.to_owned());
        }
    }
    out
}

fn workspace_members() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for dir in ["crates", "tools"] {
        for entry in fs::read_dir(root().join(dir)).expect("dir").flatten() {
            let manifest = entry.path().join("Cargo.toml");
            if let Ok(text) = fs::read_to_string(&manifest) {
                for line in text.lines() {
                    if let Some(name) = line
                        .trim()
                        .strip_prefix("name = \"")
                        .and_then(|r| r.strip_suffix('"'))
                    {
                        out.insert(name.to_owned());
                        break;
                    }
                }
            }
        }
    }
    out
}

/// The crate names in the appendix table: the first backticked cell of each row after the heading.
fn inventory() -> BTreeSet<String> {
    let policy = fs::read_to_string(root().join("docs/dependency-policy.md")).expect("policy");
    let section = policy
        .split("## Appendix — lock inventory")
        .nth(1)
        .expect("the lock inventory appendix");
    let mut out = BTreeSet::new();
    for line in section.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        if let Some((name, _)) = rest.split_once('`') {
            out.insert(name.to_owned());
        }
    }
    out
}

#[test]
fn every_locked_crate_has_a_ledger_row_and_no_row_is_stale() {
    let members = workspace_members();
    let locked: BTreeSet<String> = lock_crates().difference(&members).cloned().collect();
    let listed = inventory();
    let missing: Vec<&String> = locked.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&locked).collect();
    assert!(
        missing.is_empty(),
        "crates in Cargo.lock without a row in the lock inventory of docs/dependency-policy.md: {missing:?}\n\
         Measure the change (RFC 010 D-6), add an accepted-table row, and regenerate the appendix \
         (`python3 tools/spikes/lock-inventory.py`)."
    );
    assert!(
        stale.is_empty(),
        "inventory rows for crates no longer in Cargo.lock: {stale:?}"
    );
}
