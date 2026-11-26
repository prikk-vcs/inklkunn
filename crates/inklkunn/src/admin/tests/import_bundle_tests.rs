//! `wiki import-bundle`'s run and `wiki discard-unfinished` on the command line (RFC 007 D-1 – D-3; handoff I-T3), with a real prikk, a real key
//! store and a real SQLite store, in this process. Each test self-skips with a visible line when prikk is absent (`world` returns `None`).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inklkunn_core::CreationStep;
use inklkunn_prikk::Storage;
use inklkunn_store::{ActStore, AuditLog, WikiStore};

use super::world::{World, world};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../inklkunn-core/tests/fixtures/interchange")
        .join(rel)
}

/// A member of a JSON value, by pointer (`/inklkunn/path`); `null` when it is not there.
fn j<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    static NULL: serde_json::Value = serde_json::Value::Null;
    v.pointer(pointer).unwrap_or(&NULL)
}

/// The operator's audit events of an action: `(outcome, detail)`.
fn events(w: &World, action: &str) -> Vec<(String, String)> {
    w.store
        .read(0, 2000)
        .unwrap()
        .into_iter()
        .filter(|r| r.event.principal == "system:operator-cli" && r.event.action == action)
        .map(|r| (r.event.outcome.as_str().to_owned(), r.event.detail.clone()))
        .collect()
}

#[test]
fn a_run_makes_the_wiki_with_its_history_one_block_a_revision_and_audits_it() {
    // `world` says `skipping: prikk not on PATH` itself when there is none.
    let Some(w) = world("ib-run") else {
        return;
    };
    let bundle = fixture("mediawiki/bundle");
    let (status, out, err) = w.cli(&[
        "wiki",
        "import-bundle",
        "garden",
        bundle.to_str().unwrap(),
        "--name",
        "Garden Notes",
        "--bundle-license",
        "CC-BY-SA-4.0",
    ]);
    assert_eq!(status, 0, "{out}{err}");
    // The plan first (what would happen), then what did.
    assert!(out.contains("licence of the wiki: CC-BY-SA-4.0"), "{out}");
    assert!(
        out.contains("imported wiki garden") && out.contains("one block each"),
        "{out}"
    );
    assert!(!out.contains("not built yet") && !err.contains("not built yet"));
    // The wiki is there, whole: its licence is the bundle's, and every block has its act.
    let record = w.record("garden");
    assert_eq!(record.default_license, "CC-BY-SA-4.0");
    let blocks = w.blocks("garden");
    assert!(blocks >= 8, "{blocks}");
    let acts = w.store.system_acts(&record.id).unwrap();
    assert_eq!(acts.len(), blocks);
    assert!(acts.iter().all(|a| a.end.is_some()));
    // The report is kept beside the repository, with the act.
    let kept = std::fs::read_to_string(w.layout().import_report(&record.id)).unwrap();
    assert!(
        kept.contains("revision(s) would be replayed"),
        "the report: {kept}"
    );
    // One act, audited before it is done and again when it ends.
    assert_eq!(
        events(&w, "wiki-import-bundle"),
        vec![
            ("allowed".to_owned(), "begun".to_owned()),
            ("allowed".to_owned(), "imported".to_owned())
        ]
    );
    // The seeds of the signer's calls are gone.
    assert!(
        std::fs::read_dir(&w.settings.seed_dir)
            .unwrap()
            .next()
            .is_none()
    );
    assert!(w.layout().creation_marks().unwrap().is_empty());
}

/// **The trailers round trip** (acceptance criterion 3): an inklkunn wiki's own export, imported and exported again, carries the
/// source's times as the source's (`at_from: source`), the same pages, and the authors as the source's names.
#[test]
fn an_exported_wiki_imported_and_exported_again_keeps_its_times_as_the_sources_and_its_authors_as_names()
 {
    let Some(w) = world("ib-roundtrip") else {
        return;
    };
    let bundle = fixture("inklkunn/bundle");
    let (status, out, err) = w.cli(&[
        "wiki",
        "import-bundle",
        "orchard",
        bundle.to_str().unwrap(),
        "--name",
        "Orchard Notes",
    ]);
    assert_eq!(status, 0, "{out}{err}");
    let target = w.tmp.0.join("again");
    let (status, out, err) = w.cli(&[
        "wiki",
        "export",
        "orchard",
        "--bundle",
        target.to_str().unwrap(),
    ]);
    assert_eq!(status, 0, "{out}{err}");
    // The same pages as the bundle had.
    for page in ["Grafting.md", "Pruning.md", "Help.md"] {
        assert!(target.join("pages").join(page).is_file(), "{page}");
    }
    assert_eq!(
        std::fs::read(target.join("pages/Pruning.md")).unwrap(),
        std::fs::read(bundle.join("pages/Pruning.md")).unwrap()
    );
    // Each record the replay made says the time is the source's, and it names the source's revision: its author (as a name and a kind,
    // never an account), its summary and its time are what the bundle's record of that revision had.
    let mut had = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(bundle.join("history")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for line in text.lines() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            let key = format!(
                "{}@{}",
                j(&record, "/rev").as_str().unwrap(),
                j(&record, "/inklkunn/path").as_str().unwrap()
            );
            had.insert(key, record);
        }
    }
    let mut from_source = 0;
    for entry in std::fs::read_dir(target.join("history")).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for line in text.lines() {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            if j(&record, "/inklkunn/at_from") != "source" {
                continue;
            }
            from_source += 1;
            let rev = j(&record, "/inklkunn/imported_rev").as_str().unwrap();
            let key = format!("{rev}@{}", j(&record, "/inklkunn/path").as_str().unwrap());
            let original = had.get(&key).unwrap_or_else(|| panic!("{key}: {record}"));
            assert_eq!(j(&record, "/at"), j(original, "/at"), "{record}");
            assert_eq!(j(&record, "/author"), j(original, "/author"), "{record}");
            assert_eq!(j(&record, "/summary"), j(original, "/summary"), "{record}");
            assert_eq!(j(&record, "/content"), j(original, "/content"), "{record}");
            // The wiki signed it: the block is its own, and the record says the change was an import.
            assert_eq!(j(&record, "/inklkunn/kind"), "import");
            assert_ne!(j(&record, "/rev").as_str().unwrap(), rev);
        }
    }
    assert!(
        from_source >= 5,
        "{from_source} revisions carry the source's time"
    );
}

#[test]
fn discard_unfinished_asks_for_the_id_again_removes_exactly_what_the_start_would_not_and_is_audited()
 {
    let Some(w) = world("ib-discard") else {
        return;
    };
    // A creation killed after its first block was sealed: a history the start-up sweep reports and does not remove.
    w.killed_creation_after("sealed", CreationStep::Sealed);
    let id = w.layout().creation_marks().unwrap().remove(0);
    let repo = w.layout().repo_dir(&id);
    assert!(repo.is_dir());
    // The sweep's line names the command, with the id twice.
    let (status, _, err) = w.cli(&["wiki", "create", "other", "--name", "Other"]);
    assert_eq!(status, 0, "{err}");
    assert!(
        err.contains(&format!(
            "inklkunn wiki discard-unfinished {id} --confirm {id}"
        )),
        "{err}"
    );
    assert!(repo.is_dir(), "the sweep removed a history");
    // Without the id again: said, and nothing removed.
    let (status, out, err) = w.cli(&["wiki", "discard-unfinished", id.as_str()]);
    assert_eq!(status, 1);
    assert!(
        out.contains(&format!("an unfinished creation of wiki {id}")),
        "{out}"
    );
    assert!(
        err.contains("nothing was removed")
            && err.contains(&format!("--confirm {id}"))
            && err.contains("for good"),
        "{err}"
    );
    assert!(repo.is_dir());
    // The wrong id typed: refused, nothing removed.
    let other = inklkunn_store::WikiId::new();
    let (status, _, err) = w.cli(&[
        "wiki",
        "discard-unfinished",
        id.as_str(),
        "--confirm",
        other.as_str(),
    ]);
    assert_eq!(status, 1);
    assert!(err.contains("not the id of the creation"), "{err}");
    assert!(repo.is_dir());
    // Not an id at all.
    let (status, _, err) = w.cli(&[
        "wiki",
        "discard-unfinished",
        "sealed",
        "--confirm",
        "sealed",
    ]);
    assert_eq!(status, 1);
    assert!(err.contains("is not a wiki id"), "{err}");
    // An id that has no unfinished creation.
    let (status, _, err) = w.cli(&[
        "wiki",
        "discard-unfinished",
        other.as_str(),
        "--confirm",
        other.as_str(),
    ]);
    assert_eq!(status, 1);
    assert!(err.contains("no unfinished creation"), "{err}");
    // A wiki whose record was stored is never discarded, whatever its id is typed twice.
    let kept = w.record("other");
    let (status, _, err) = w.cli(&[
        "wiki",
        "discard-unfinished",
        kept.id.as_str(),
        "--confirm",
        kept.id.as_str(),
    ]);
    assert_eq!(status, 1);
    assert!(
        err.contains("no unfinished creation") || err.contains("record of that wiki is stored"),
        "{err}"
    );
    assert!(w.layout().repo_dir(&kept.id).is_dir());
    assert!(repo.is_dir());
    assert!(events(&w, "wiki-discard-unfinished").is_empty());
    // The id typed twice: removed, and nothing of it is left; the other wiki is untouched.
    let (status, out, err) = w.cli(&[
        "wiki",
        "discard-unfinished",
        id.as_str(),
        "--confirm",
        id.as_str(),
    ]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("removed: nothing of the unfinished creation is left"),
        "{out}"
    );
    assert!(!repo.exists());
    assert!(w.layout().creation_marks().unwrap().is_empty());
    assert_eq!(w.store.list().unwrap().len(), 1);
    assert!(
        w.storage
            .verify(&w.layout().repo_dir(&kept.id))
            .unwrap()
            .verdict
            .ok
    );
    // Audited before it was done, and again when it was.
    assert_eq!(
        events(&w, "wiki-discard-unfinished"),
        vec![
            ("allowed".to_owned(), "removal begun".to_owned()),
            (
                "allowed".to_owned(),
                "unfinished creation removed".to_owned()
            )
        ]
    );
    // The wiki's key record went with it, and the other wiki's stayed.
    let keys = std::fs::read_dir(&w.settings.key_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".key.json"))
        .count();
    assert_eq!(keys, 1);
}

/// **V-8**: the command asks the mark and the store before it describes anything, so a stored wiki's id is never called *an unfinished
/// creation*, with or without `--confirm`.
#[test]
fn discard_unfinished_checks_the_mark_and_the_store_before_it_says_what_it_found() {
    let Some(w) = world("ib-discard-first") else {
        return;
    };
    // A creation killed after its record was stored: a wiki, and only its mark is left.
    w.killed_creation_after("whole", CreationStep::Stored);
    let id = w.layout().creation_marks().unwrap().remove(0);
    assert!(w.store.get(&id).is_ok(), "the record is stored");
    let (status, out, err) = w.cli(&["wiki", "discard-unfinished", id.as_str()]);
    assert_eq!(status, 1, "{out}{err}");
    assert!(!out.contains("unfinished creation"), "{out}");
    assert!(
        err.contains("is stored") || err.contains("no unfinished creation"),
        "{err}"
    );
    assert!(w.layout().repo_dir(&id).is_dir());
    assert!(w.store.get(&id).is_ok());
    // An id with no mark at all is not described either.
    let none = inklkunn_store::WikiId::new();
    let (status, out, err) = w.cli(&["wiki", "discard-unfinished", none.as_str()]);
    assert_eq!(status, 1, "{out}{err}");
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("no unfinished creation"), "{err}");
}

/// **V-12**: the usage names the visibilities the parser accepts, for `create` and for `import-bundle`, and a refusal says the same.
#[test]
fn the_usage_and_the_refusal_name_the_visibilities_the_parser_accepts() {
    let Some(w) = world("ib-visibility") else {
        return;
    };
    assert!(
        crate::admin::USAGE.contains(
            "wiki create <slug> --name <name> [--license <spdx>] [--lang <tag>]... [--visibility public|internal|restricted]"
        ),
        "{}",
        crate::admin::USAGE
    );
    for v in ["public", "internal", "restricted"] {
        assert!(inklkunn_store::Visibility::parse(v).is_some(), "{v}");
    }
    let (status, _, err) = w.cli(&[
        "wiki",
        "create",
        "x",
        "--name",
        "X",
        "--visibility",
        "private",
    ]);
    assert_eq!(status, 1);
    assert!(
        err.contains("is not a visibility (public, internal or restricted)"),
        "{err}"
    );
}

/// **V-11**: a converted MediaWiki wiki (its pages' ids are `mw:101`, which no file can be named) exported again keeps its history:
/// the file is named by the page's path, and the import reads that name back.
#[test]
fn a_converted_mediawiki_wiki_exported_again_keeps_its_history_under_the_names_of_its_paths() {
    let Some(w) = world("ib-mw-roundtrip") else {
        return;
    };
    let bundle = fixture("mediawiki/bundle");
    let (status, out, err) = w.cli(&[
        "wiki",
        "import-bundle",
        "garden",
        bundle.to_str().unwrap(),
        "--name",
        "Garden Notes",
        "--bundle-license",
        "CC-BY-SA-4.0",
    ]);
    assert_eq!(status, 0, "{out}{err}");
    let target = w.tmp.0.join("mw-again");
    let (status, out, err) = w.cli(&[
        "wiki",
        "export",
        "garden",
        "--bundle",
        target.to_str().unwrap(),
    ]);
    assert_eq!(status, 0, "{out}{err}");
    let manifest = std::fs::read_to_string(target.join("wiki-bundle.yaml")).unwrap();
    assert!(
        manifest.contains("  histories_not_written: 0\n"),
        "{manifest}"
    );
    let names: Vec<String> = std::fs::read_dir(target.join("history"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "~%43omposting.jsonl"),
        "{names:?}"
    );
    assert!(names.iter().all(|n| !n.contains(':')), "{names:?}");
    // The page whose id is `mw:101` has the revisions the fixture's history has, each a record with its text.
    let kept = std::fs::read_to_string(target.join("history/~%43omposting.jsonl")).unwrap();
    let had = std::fs::read_to_string(bundle.join("history/mw:101.jsonl")).unwrap();
    assert!(kept.lines().count() >= had.lines().count(), "{kept}");
    // And the exported bundle is one this import takes in again, its history with it.
    let (status, out, err) = w.cli(&[
        "wiki",
        "import-bundle",
        "garden-again",
        target.to_str().unwrap(),
        "--name",
        "Garden again",
        "--dry-run",
    ]);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.contains("pages/Composting.md:"), "{out}");
}
