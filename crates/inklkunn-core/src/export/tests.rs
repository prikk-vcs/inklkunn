#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The destination rules and the pure parts of an export. The export of a real wiki, its history and its round
//! trip are `tests/export_roundtrip.rs`, against a real prikk.

use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, Materialized, Tree, Verification, Views,
};

use super::*;

/// A storage that must not be called: the rules under test refuse before any read.
struct Unreachable;

fn nope<T>() -> inklkunn_prikk::Result<T> {
    Err(StorageError::Driver {
        verb: "test",
        detail: "storage must not be reached".to_owned(),
    })
}

impl Storage for Unreachable {
    fn views(&self, _: &Path) -> inklkunn_prikk::Result<Views> {
        nope()
    }
    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> inklkunn_prikk::Result<Tree> {
        nope()
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> inklkunn_prikk::Result<FileMeta> {
        nope()
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> inklkunn_prikk::Result<Vec<u8>> {
        nope()
    }
    fn history(&self, _: &Path, _: &Point, _: Option<u64>) -> inklkunn_prikk::Result<History> {
        nope()
    }
    fn change(&self, _: &Path, _: &str) -> inklkunn_prikk::Result<Change> {
        nope()
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> inklkunn_prikk::Result<Diff> {
        nope()
    }
    fn plan(&self, _: &Path, _: &Point) -> inklkunn_prikk::Result<Materialized> {
        nope()
    }
    fn materialize(
        &self,
        _: &Path,
        _: &Point,
        _: &Path,
        _: &str,
        _: ExtractBounds,
    ) -> inklkunn_prikk::Result<Materialized> {
        nope()
    }
    fn verify(&self, _: &Path) -> inklkunn_prikk::Result<Verification> {
        nope()
    }
}

fn record() -> WikiRecord {
    use inklkunn_store::{DisplayName, NewWiki, Slug, WikiId};
    WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(Slug::new("exp").unwrap(), DisplayName::new("Exp").unwrap()),
        1,
    )
    .unwrap()
}

#[test]
fn a_destination_that_is_relative_exists_or_has_no_directory_is_refused_before_anything_is_read() {
    let tmp = std::env::temp_dir().join(format!("inklkunn-export-dest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::create_dir(tmp.join("taken")).unwrap();
    let layout = RootLayout::new(tmp.join("root"));
    for (dest, needle) in [
        (PathBuf::from("rel/dir"), "absolute"),
        (tmp.join("taken"), "already exists"),
        (tmp.join("nowhere/out"), "directory does not exist"),
    ] {
        let err = export_bundle(&Unreachable, &layout, &record(), &dest, "0.0.0", 1).unwrap_err();
        assert!(
            matches!(&err, ExportError::Destination(why) if why.contains(needle)),
            "{dest:?}: {err}"
        );
    }
    // Nothing was made beside the destination either.
    let names: Vec<String> = std::fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["taken".to_owned()]);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn only_the_not_recorded_refusal_is_the_checkpoint_one() {
    let refusal = |kind: &str, reason: Option<&str>| StorageError::Refused {
        verb: "extract",
        message: "x".to_owned(),
        kind: Some(kind.to_owned()),
        reason: reason.map(str::to_owned),
    };
    assert!(is_checkpoint_not_recorded(&refusal(
        "no-checkpoint-in-reach",
        Some("not-recorded")
    )));
    assert!(!is_checkpoint_not_recorded(&refusal(
        "no-checkpoint-in-reach",
        Some("signer-not-adopted")
    )));
    assert!(!is_checkpoint_not_recorded(&refusal(
        "precondition-not-met",
        None
    )));
}

#[test]
fn a_manifest_carries_the_fidelity_the_counts_the_block_the_interwiki_map_and_every_checksum() {
    let rec = record();
    let text = bundle::manifest_with(
        &rec,
        "0.0.0",
        &ManifestFacts {
            exported: Some(ExportFacts {
                pages: 3,
                attachments: 2,
                exported_at: 1_700_000_000,
                tip: "ab".repeat(32),
                interwiki: vec![("wp".to_owned(), "https://en.wikipedia.org/wiki/".to_owned())],
                unattributed_changes: 4,
                checksums: vec![("pages/A.md".to_owned(), "00ff".to_owned())],
            }),
        },
    );
    for needle in [
        "  level: 3",
        "  degraded: []",
        "  pages: 3",
        "  attachments: 2",
        "  users: true",
        "  at: 1700000000",
        "  history_changes_without_a_page: 4",
        "interwiki:\n  \"wp\": \"https://en.wikipedia.org/wiki/\"",
        "  algorithm: sha256",
        "    \"pages/A.md\": \"00ff\"",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    // The creation manifest is what it was: counts at creation, no export section.
    let created = bundle::manifest(&rec, "0.0.0");
    assert!(created.contains("# Counts at creation; the export rewrites them."));
    assert!(!created.contains("checksums:") && !created.contains("export:"));
}
