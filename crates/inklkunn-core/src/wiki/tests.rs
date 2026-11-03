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

    /// What a build that stored the record and the act in two steps left when a kill came between them: the
    /// wiki's record stored, its sealed first change in the history, no act, and the mark. (The creation now
    /// stores both in one transaction, so this state can only come from an earlier build; the sweep's repair
    /// is its second line.)
    fn legacy_wiki_without_its_act(&self, repo: &Repo) -> WikiRecord {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let killed = catch_unwind(AssertUnwindSafe(|| {
            self.create(repo, &*self.store, &|s| {
                if s == CreationStep::Verified {
                    panic!("stopped after {s:?}");
                }
            })
        }));
        std::panic::set_hook(hook);
        assert!(killed.is_err());
        let [wiki] = self.layout.creation_marks().unwrap().try_into().unwrap();
        let record = WikiRecord::from_new(wiki, new_wiki(), 1).unwrap();
        self.store.insert(record).unwrap()
    }

    fn findings(&self, repo: &Repo, wiki: &WikiRecord) -> Vec<Finding> {
        let acts = StoreActs::new(self.store.clone(), self.store.clone());
        reconcile(&wiki.id, &self.layout.repo_dir(&wiki.id), repo, &acts)
            .unwrap()
            .findings
    }
}

#[test]
fn the_first_change_is_kept_as_a_sealed_system_act_with_the_record_and_the_reconciliation_reports_nothing()
 {
    let rig = rig("recorded");
    let repo = Repo::new();
    let steps = RefCell::new(Vec::new());
    let created = rig
        .create(&repo, &*rig.store, &|s| steps.borrow_mut().push(s))
        .unwrap();
    assert_eq!(steps.into_inner(), CreationStep::ALL.to_vec());
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
fn a_wiki_stored_without_its_act_by_an_earlier_build_leaves_the_mark_and_the_sweep_keeps_the_act() {
    let rig = rig("unrecorded");
    let repo = Repo::new();
    let record = rig.legacy_wiki_without_its_act(&repo);
    // The wiki is stored; its first block has no record, and the reconciliation says so.
    let found = rig.findings(&repo, &record);
    assert!(
        matches!(found.as_slice(), [Finding::UnrecordedSystemChange { .. }]),
        "{found:?}"
    );
    assert_eq!(
        rig.layout.creation_marks().unwrap(),
        std::slice::from_ref(&record.id)
    );
    // The next start keeps the act, removes the mark, and reconciles clean.
    let report = rig.sweep(&repo);
    assert_eq!(report.completed, std::slice::from_ref(&record.id));
    assert!(report.left_behind.is_empty() && report.removed.is_empty());
    assert_eq!(rig.findings(&repo, &record), Vec::<Finding>::new());
    assert!(rig.layout.creation_marks().unwrap().is_empty());
    assert!(rig.sweep(&repo).is_empty());
}

#[test]
fn a_creation_killed_after_the_record_is_stored_has_its_act_already_and_the_sweep_only_removes_the_mark()
 {
    let rig = rig("killed-stored");
    let repo = Repo::new();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let killed = catch_unwind(AssertUnwindSafe(|| {
        rig.create(&repo, &*rig.store, &|s| {
            if s == CreationStep::Stored {
                panic!("stopped after {s:?}");
            }
        })
    }));
    std::panic::set_hook(hook);
    assert!(killed.is_err());
    // The record and the act were stored together: a kill right after leaves both.
    let record = rig.store.list().unwrap().remove(0);
    assert_eq!(rig.store.system_acts(&record.id).unwrap().len(), 1);
    let report = rig.sweep(&repo);
    assert_eq!(report.completed, std::slice::from_ref(&record.id));
    assert!(report.left_behind.is_empty() && report.removed.is_empty());
    // Exactly one act: the sweep wrote none.
    assert_eq!(rig.store.system_acts(&record.id).unwrap().len(), 1);
    assert_eq!(rig.findings(&repo, &record), Vec::<Finding>::new());
    assert!(rig.layout.creation_marks().unwrap().is_empty());
    assert!(rig.sweep(&repo).is_empty());
}

/// A wiki store whose plain `insert` must never be used by a creation: everything else is the memory store's.
struct NoPlainInsert<'a>(&'a MemoryStore);

impl WikiStore for NoPlainInsert<'_> {
    fn insert(&self, _: WikiRecord) -> Result<WikiRecord, StoreError> {
        panic!("a creation stored the record by itself, outside the transaction with its act");
    }
    fn get(&self, id: &WikiId) -> Result<WikiRecord, StoreError> {
        self.0.get(id)
    }
    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord, StoreError> {
        self.0.by_slug(slug)
    }
    fn list(&self) -> Result<Vec<WikiRecord>, StoreError> {
        self.0.list()
    }
    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        name: Option<DisplayName>,
    ) -> Result<WikiRecord, StoreError> {
        self.0.rename(id, slug, name)
    }
    fn set_verification(
        &self,
        id: &WikiId,
        v: inklkunn_store::Verification,
    ) -> Result<WikiRecord, StoreError> {
        self.0.set_verification(id, v)
    }
    fn set_verification_explained(
        &self,
        id: &WikiId,
        v: inklkunn_store::Verification,
        explained_by: &[String],
    ) -> Result<WikiRecord, StoreError> {
        self.0.set_verification_explained(id, v, explained_by)
    }
    fn record_attempt(
        &self,
        id: &WikiId,
        a: inklkunn_store::Attempt,
    ) -> Result<WikiRecord, StoreError> {
        self.0.record_attempt(id, a)
    }
    fn set_keys(
        &self,
        id: &WikiId,
        a: Option<String>,
        m: Option<String>,
        f: Option<String>,
    ) -> Result<WikiRecord, StoreError> {
        self.0.set_keys(id, a, m, f)
    }
    fn request_checkpoint_refresh(&self, id: &WikiId, at: i64) -> Result<WikiRecord, StoreError> {
        self.0.request_checkpoint_refresh(id, at)
    }
    fn record_checkpoint_refresh(
        &self,
        id: &WikiId,
        prikk: &inklkunn_store::PrikkVersion,
    ) -> Result<WikiRecord, StoreError> {
        self.0.record_checkpoint_refresh(id, prikk)
    }
}

/// Corrections P8 (D6): a creation stores the record and the act through the one transaction, never the record
/// alone (a kill between the two is what the transaction rules out).
#[test]
fn a_creation_never_stores_the_record_outside_the_transaction_with_its_act() {
    let rig = rig("one-transaction");
    let repo = Repo::new();
    let created = create_wiki_observed(
        &NoPlainInsert(&rig.store),
        &*rig.store,
        &rig.layout,
        &repo,
        &repo,
        new_wiki(),
        "0.0.0",
        &KeyAuthority::for_tests(),
        &|_| {},
    )
    .unwrap();
    assert_eq!(rig.store.system_acts(&created.record.id).unwrap().len(), 1);
    assert_eq!(rig.store.get(&created.record.id).unwrap(), created.record);
}

/// Corrections P8 (D6): a store that refuses the creation's transaction leaves no wiki and no act, and the
/// attempt is removed whole.
#[test]
fn a_store_that_refuses_the_record_and_act_together_leaves_neither_and_nothing_on_disk() {
    let rig = rig("refused");
    let repo = Repo::new();
    // The slug is taken by the time the transaction runs (a race the early check cannot see).
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let raced = rig.create(&repo, &*rig.store, &|s| {
        if s == CreationStep::Verified {
            rig.store
                .create(NewWiki::new(
                    Slug::new("created").unwrap(),
                    DisplayName::new("First").unwrap(),
                ))
                .unwrap();
        }
    });
    std::panic::set_hook(hook);
    assert!(matches!(
        raced,
        Err(CreateWikiError::Store(StoreError::SlugTaken(_)))
    ));
    let [winner] = rig.store.list().unwrap().try_into().unwrap();
    assert_eq!(winner.display_name.as_str(), "First");
    assert!(
        rig.store.system_acts(&winner.id).unwrap().is_empty(),
        "the loser's act was not stored"
    );
    assert!(rig.layout.creation_marks().unwrap().is_empty());
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
        let record = rig.legacy_wiki_without_its_act(&first);
        let other = make();
        let report = rig.sweep(&other);
        assert!(report.completed.is_empty(), "{tag}: {report:?}");
        let [(wiki, text)] = report.left_behind.as_slice() else {
            panic!("{tag}: {report:?}");
        };
        assert_eq!(wiki, &record.id);
        assert!(text.contains(why), "{tag}: {text}");
        // Nothing was written, and the mark stays for the next start.
        assert!(
            rig.store.system_acts(&record.id).unwrap().is_empty(),
            "{tag}"
        );
        assert_eq!(rig.layout.creation_marks().unwrap().len(), 1, "{tag}");
    }
}
