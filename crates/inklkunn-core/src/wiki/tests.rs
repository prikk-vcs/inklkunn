//! Creation records its first change (handoff 004 T4b), against a repository that is a double: no prikk is
//! needed, so these run in both test runs. The real repository is in `tests/create_wiki.rs` and
//! `tests/creation_kill.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Mutex;

use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, HistoryBlock, Materialized, PatchMessage, Stage, Tree,
    Verdict, Verification, View, Views,
};
use inklkunn_prikk::{ExtractBounds, Result as StorageResult};
use inklkunn_signer::{KeyIds, SealedIds, SignerError};
use inklkunn_store::{AcceptanceOutcome, MemoryStore, SystemActKind};
use inklkunn_store::{DisplayName, Slug};
use std::sync::Arc;

use super::*;
use crate::acceptance::StoreActs;
use crate::reconcile::{Finding, reconcile};

fn id(n: usize) -> String {
    format!("{n:064x}")
}

/// A repository that is a double: it seals what it is handed and then reads it back as its history.
struct Repo {
    sealed: Mutex<Option<String>>,
    fail_history: bool,
}

impl Repo {
    fn new() -> Repo {
        Repo {
            sealed: Mutex::new(None),
            fail_history: false,
        }
    }

    /// A repository whose first change was sealed with `message`.
    fn sealed_with(message: &str) -> Repo {
        let repo = Repo::new();
        *repo.sealed.lock().unwrap() = Some(message.to_owned());
        repo
    }
}

impl RepositoryBootstrap for Repo {
    fn create_repository(&self, _: &Path, _: &KeyAuthority) -> Result<KeyIds, SignerError> {
        Ok(KeyIds {
            author: "ed25519-aaaaaaaaaaaaaaaa".to_owned(),
            maintainer: "ed25519-bbbbbbbbbbbbbbbb".to_owned(),
            maintainer_public_key: "cc".repeat(32),
        })
    }

    fn commit_and_seal(&self, _: &Path, message: &str) -> Result<SealedIds, SignerError> {
        *self.sealed.lock().unwrap() = Some(message.to_owned());
        Ok(SealedIds {
            block_id: id(1),
            patch_ids: vec![id(2)],
            ref_state_id: id(3),
        })
    }
}

fn nope<T>(verb: &'static str) -> StorageResult<T> {
    Err(StorageError::Driver {
        verb,
        detail: "not in this double".to_owned(),
    })
}

impl Storage for Repo {
    fn views(&self, _: &Path) -> StorageResult<Views> {
        Ok(Views {
            schema_version: "branch-list-v1".to_owned(),
            views: vec![View {
                name: "heads/main".to_owned(),
                state_id: id(3),
                closed: false,
                current: true,
            }],
        })
    }
    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> StorageResult<Tree> {
        nope("tree")
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> StorageResult<FileMeta> {
        nope("cat")
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> StorageResult<Vec<u8>> {
        nope("cat")
    }
    fn history(&self, _: &Path, _: &Point, _: Option<u64>) -> StorageResult<History> {
        if self.fail_history {
            return nope("log");
        }
        let blocks = self
            .sealed
            .lock()
            .unwrap()
            .iter()
            .map(|message| HistoryBlock {
                block_id: id(1),
                ref_state_id: id(3),
                update_seq: 1,
                kind: "Normal".to_owned(),
                rollback_block: false,
                patch_count: 1,
                patch_messages: vec![PatchMessage {
                    patch_id: id(2),
                    message: message.clone(),
                }],
                previous_ref_state_id: None,
            })
            .collect();
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: "heads/main".to_owned(),
            blocks,
        })
    }
    fn change(&self, _: &Path, _: &str) -> StorageResult<Change> {
        nope("show")
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> StorageResult<Diff> {
        nope("diff")
    }
    fn plan(&self, _: &Path, _: &Point) -> StorageResult<Materialized> {
        nope("extract")
    }
    fn materialize(
        &self,
        _: &Path,
        _: &Point,
        _: &Path,
        _: &str,
        _: ExtractBounds,
    ) -> StorageResult<Materialized> {
        nope("extract")
    }
    fn verify(&self, _: &Path) -> StorageResult<Verification> {
        Ok(Verification {
            schema_version: "verify-report-v1".to_owned(),
            verdict: Verdict {
                ok: true,
                failed_conditions: Vec::new(),
            },
            stages: vec![Stage {
                stage: "objects".to_owned(),
                status: "evaluated".to_owned(),
                message: None,
                blocked_by: None,
            }],
        })
    }
}

struct Rig {
    _dir: crate::read::testing::TempDir,
    layout: RootLayout,
    store: Arc<MemoryStore>,
}

fn rig(tag: &str) -> Rig {
    let dir = crate::read::testing::TempDir::new(tag);
    Rig {
        layout: RootLayout::new(dir.0.join("root")),
        _dir: dir,
        store: Arc::new(MemoryStore::new()),
    }
}

fn new_wiki() -> NewWiki {
    NewWiki::new(
        Slug::new("created").unwrap(),
        DisplayName::new("Created").unwrap(),
    )
}

impl Rig {
    /// Create a wiki, keeping the first change in `acts`; every step is observed through `observe`.
    fn create(
        &self,
        repo: &Repo,
        acts: &dyn ActStore,
        observe: &dyn Fn(CreationStep),
    ) -> Result<CreatedWiki, CreateWikiError> {
        create_wiki_observed(
            &*self.store,
            acts,
            &self.layout,
            repo,
            repo,
            new_wiki(),
            "0.0.0",
            &KeyAuthority::for_tests(),
            observe,
        )
    }

    fn sweep(&self, repo: &Repo) -> SweepReport {
        sweep_unfinished_creations(
            &*self.store,
            &*self.store,
            &self.layout,
            repo,
            repo,
            false,
            &NoSweepAudit,
        )
        .unwrap()
    }

    fn findings(&self, repo: &Repo, wiki: &WikiRecord) -> Vec<Finding> {
        let acts = StoreActs::new(self.store.clone(), self.store.clone());
        reconcile(&wiki.id, &self.layout.repo_dir(&wiki.id), repo, &acts)
            .unwrap()
            .findings
    }
}

#[test]
fn the_first_change_is_kept_as_a_sealed_system_act_after_the_record_and_the_reconciliation_reports_nothing()
 {
    let rig = rig("recorded");
    let repo = Repo::new();
    let steps = RefCell::new(Vec::new());
    let created = rig
        .create(&repo, &*rig.store, &|s| steps.borrow_mut().push(s))
        .unwrap();
    assert_eq!(steps.into_inner(), CreationStep::ALL.to_vec());
    assert!(created.first_change_recorded);
    let acts = rig.store.system_acts(&created.record.id).unwrap();
    let [entry] = acts.as_slice() else {
        panic!("one act, not {}", acts.len());
    };
    assert_eq!(entry.act.kind, SystemActKind::System);
    assert_eq!(entry.act.view, created.record.default_view);
    assert_eq!(entry.act.message, creation_message(&created.record));
    assert_eq!(
        entry.end.as_ref().map(|e| &e.outcome),
        Some(&AcceptanceOutcome::Sealed {
            block_id: id(1),
            patch_id: id(2)
        })
    );
    assert_eq!(rig.findings(&repo, &created.record), Vec::<Finding>::new());
    // The mark is gone: nothing is left to sweep.
    assert!(rig.layout.creation_marks().unwrap().is_empty());
}

#[test]
fn a_store_that_cannot_take_the_act_leaves_the_mark_and_the_sweep_keeps_the_act() {
    let rig = rig("unrecorded");
    let repo = Repo::new();
    // A store that has never heard of the wiki cannot begin an act for it.
    let elsewhere = MemoryStore::new();
    let steps = RefCell::new(Vec::new());
    let created = rig
        .create(&repo, &elsewhere, &|s| steps.borrow_mut().push(s))
        .unwrap();
    assert!(!created.first_change_recorded);
    assert_eq!(steps.into_inner().last(), Some(&CreationStep::Stored));
    // The wiki is whole and stored; its first block has no record, and the reconciliation says so.
    let found = rig.findings(&repo, &created.record);
    assert!(
        matches!(found.as_slice(), [Finding::UnrecordedSystemChange { .. }]),
        "{found:?}"
    );
    assert_eq!(
        rig.layout.creation_marks().unwrap(),
        std::slice::from_ref(&created.record.id)
    );
    // The next start keeps the act, removes the mark, and reconciles clean.
    let report = rig.sweep(&repo);
    assert_eq!(report.completed, std::slice::from_ref(&created.record.id));
    assert!(report.left_behind.is_empty() && report.removed.is_empty());
    assert_eq!(rig.findings(&repo, &created.record), Vec::<Finding>::new());
    assert!(rig.layout.creation_marks().unwrap().is_empty());
    assert!(rig.sweep(&repo).is_empty());
}

#[test]
fn a_creation_killed_after_the_record_is_stored_is_made_whole_by_the_sweep_and_one_after_the_act_needs_nothing()
 {
    for (step, tag) in [
        (CreationStep::Stored, "killed-stored"),
        (CreationStep::ActRecorded, "killed-recorded"),
    ] {
        let rig = rig(tag);
        let repo = Repo::new();
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let killed = catch_unwind(AssertUnwindSafe(|| {
            rig.create(&repo, &*rig.store, &|s| {
                if s == step {
                    panic!("stopped after {s:?}");
                }
            })
        }));
        std::panic::set_hook(hook);
        assert!(killed.is_err(), "{step:?}");
        let record = rig.store.list().unwrap().remove(0);
        let before = rig.store.system_acts(&record.id).unwrap().len();
        assert_eq!(
            before,
            usize::from(step == CreationStep::ActRecorded),
            "{step:?}"
        );
        let report = rig.sweep(&repo);
        assert_eq!(
            report.completed,
            std::slice::from_ref(&record.id),
            "{step:?}"
        );
        assert!(report.left_behind.is_empty() && report.removed.is_empty());
        // Exactly one act, whichever side of the kill it was written on.
        assert_eq!(
            rig.store.system_acts(&record.id).unwrap().len(),
            1,
            "{step:?}"
        );
        assert_eq!(
            rig.findings(&repo, &record),
            Vec::<Finding>::new(),
            "{step:?}"
        );
        assert!(rig.layout.creation_marks().unwrap().is_empty());
        assert!(rig.sweep(&repo).is_empty());
    }
}

#[test]
fn the_sweep_writes_no_act_it_cannot_tie_to_a_system_first_change_and_keeps_the_mark() {
    // Three histories the sweep must not invent a creation from: none, an unreadable message, a failing log.
    type Case = (&'static str, fn() -> Repo, &'static str);
    let cases: [Case; 3] = [
        ("no-history", Repo::new, "no first change"),
        (
            "not-system",
            || Repo::sealed_with("a message that is not a patch message"),
            "not a system change",
        ),
        (
            "log-fails",
            || Repo {
                fail_history: true,
                ..Repo::sealed_with("x")
            },
            "history could not be read",
        ),
    ];
    for (tag, make, why) in cases {
        let rig = rig(tag);
        let first = Repo::new();
        let elsewhere = MemoryStore::new();
        let created = rig.create(&first, &elsewhere, &|_| {}).unwrap();
        assert!(!created.first_change_recorded);
        let other = make();
        let report = rig.sweep(&other);
        assert!(report.completed.is_empty(), "{tag}: {report:?}");
        let [(wiki, text)] = report.left_behind.as_slice() else {
            panic!("{tag}: {report:?}");
        };
        assert_eq!(wiki, &created.record.id);
        assert!(text.contains(why), "{tag}: {text}");
        // Nothing was written, and the mark stays for the next start.
        assert!(
            rig.store
                .system_acts(&created.record.id)
                .unwrap()
                .is_empty(),
            "{tag}"
        );
        assert_eq!(rig.layout.creation_marks().unwrap().len(), 1, "{tag}");
    }
}
