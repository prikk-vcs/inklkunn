#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The destination rules and the pure parts of an export. The export of a real wiki, its history and its round
//! trip are `tests/export_roundtrip.rs`, against a real prikk.

use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, Materialized, Tree, Verification, Views,
};

use std::os::unix::fs::PermissionsExt as _;

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
fn a_manifest_carries_the_fidelity_the_counts_the_block_the_interwiki_map_and_the_checksums_file() {
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
                histories_not_written: 2,
            }),
        },
    );
    for needle in [
        "  level: 3",
        "  degraded: []",
        "  pages: 3",
        "  attachments: 2",
        "  users: true",
        "exported_at: \"2023-11-14T22:13:20Z\"",
        "  history_changes_without_a_page: 4",
        "interwiki:\n  \"wp\": \"https://en.wikipedia.org/wiki/\"",
        "  histories_not_written: 2",
        "checksums: SHA256SUMS\n",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    // The creation manifest is what it was: counts at creation, no export section.
    let created = bundle::manifest(&rec, "0.0.0");
    assert!(created.contains("# Counts at creation; the export rewrites them."));
    assert!(!created.contains("checksums:") && !created.contains("exported_at"));
}

#[test]
fn only_a_leftover_of_exactly_the_shape_and_of_an_ended_process_is_swept() {
    assert_eq!(leftover_pid("export-123-456", "export-"), Some(123));
    assert_eq!(leftover_pid(".out.partial-7-8", ".out.partial-"), Some(7));
    for not in [
        "export-123",
        "export--1",
        "export-12-",
        "export-1a-2",
        "export-1-2-3",
        "export-123-456x",
        "tip-index",
        "export-99999999999999999999-1",
    ] {
        assert_eq!(leftover_pid(not, "export-"), None, "{not}");
    }
    assert!(
        !process_has_ended(std::process::id()),
        "this process is alive"
    );
    assert!(process_has_ended(4_294_967_000), "no such process");
}

#[test]
fn an_empty_directory_of_ours_is_what_the_bundle_is_renamed_over_and_a_taken_name_is_refused() {
    let tmp = std::env::temp_dir().join(format!("inklkunn-export-place-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let partial = tmp.join("partial");
    std::fs::create_dir_all(partial.join("pages")).unwrap();
    std::fs::write(partial.join("pages/A.md"), "a").unwrap();
    // The export makes its partial bundle private; the rename carries that mode to the destination.
    std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o700)).unwrap();
    // A name that is taken (by an empty directory, which a plain rename would replace) is refused untouched.
    let taken = tmp.join("taken");
    std::fs::create_dir(&taken).unwrap();
    let err = put_in_place(&partial, &taken).unwrap_err();
    assert!(matches!(err, ExportError::Destination(_)), "{err}");
    assert_eq!(std::fs::read_dir(&taken).unwrap().count(), 0);
    assert!(partial.join("pages/A.md").is_file(), "the bundle was moved");
    // A file or a link at the name is refused too.
    std::fs::write(tmp.join("file"), "x").unwrap();
    std::os::unix::fs::symlink(&taken, tmp.join("link")).unwrap();
    for name in ["file", "link"] {
        let err = put_in_place(&partial, &tmp.join(name)).unwrap_err();
        assert!(matches!(err, ExportError::Destination(_)), "{name}: {err}");
    }
    // A free name gets the bundle, private.
    let dest = tmp.join("dest");
    put_in_place(&partial, &dest).unwrap();
    assert_eq!(std::fs::read(dest.join("pages/A.md")).unwrap(), b"a");
    assert!(!partial.exists());
    assert_eq!(
        std::fs::metadata(&dest).unwrap().permissions().mode() & 0o077,
        0
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_history_file_is_written_only_for_a_safe_id_that_exactly_one_page_carries() {
    // The shared-id branch (S2): two pages with one id cannot say whose history a file holds. The import no longer
    // brings such a pair in, but a repository written by prikk directly, or before the import's check, can hold one.
    assert!(history_file_allowed("good-1", 1));
    assert!(!history_file_allowed("twin-1", 2), "a shared id");
    assert!(!history_file_allowed("twin-1", 3), "an id shared by three");
    assert!(
        !history_file_allowed("gone-1", 0),
        "an id no page of the tip carries"
    );
    for unsafe_id in [
        "",
        ".hidden",
        "../escape",
        "a/b",
        "a b",
        "tab\t",
        "nul\0",
        "ünï",
        &"x".repeat(129),
    ] {
        assert!(!history_file_allowed(unsafe_id, 1), "{unsafe_id:?}");
    }
    assert!(history_file_allowed(&"x".repeat(128), 1), "the longest id");
    assert!(history_file_allowed("A_b-c.d", 1));
}
