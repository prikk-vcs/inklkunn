//! The reconciliation against a real wiki (RFC 002 D-7; handoff 002 T6): a repository created and written by
//! the real signer, read through the real driver, compared with records the test keeps. Self-skips with a
//! visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use inklkunn_core::message::{Actor, BlockId, Kind, Message, Summary, SystemLabel};
use inklkunn_core::reconcile::{
    AcceptedRecord, Finding, ReadBounds, RecordedActs, RecordsError, SystemActRecord, reconcile,
    reconcile_with, record_findings,
};
use inklkunn_core::{
    Accept, AcceptanceLedger, AcceptanceToken, ExpectedMessage, LedgerError, PreparedChange,
    PreparedFile, RepositoryBootstrap, Signer, SignerConfig, ViewCreationAct, ViewCreationFacts,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AccountId, AuditOutcome, DisplayName, MemoryAudit, ProposalId, RootLayout, WikiId,
};

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn dir(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-rec-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    Dir(p)
}

/// The signer's ledger and the reconciliation's records, in one place the test controls.
#[derive(Default)]
struct Books {
    pending: Mutex<HashMap<String, ExpectedMessage>>,
    accepted: Mutex<Vec<AcceptedRecord>>,
    acts: Mutex<Vec<SystemActRecord>>,
}

impl AcceptanceLedger for Books {
    fn pending(&self, t: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(self.pending.lock().unwrap().get(t.as_str()).cloned())
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

impl RecordedActs for Books {
    fn accepted(&self, _: &WikiId) -> Result<Vec<AcceptedRecord>, RecordsError> {
        Ok(self.accepted.lock().unwrap().clone())
    }
    fn system_acts(&self, _: &WikiId) -> Result<Vec<SystemActRecord>, RecordsError> {
        Ok(self.acts.lock().unwrap().clone())
    }
}

struct World {
    _tmp: Dir,
    _shm: Dir,
    books: Arc<Books>,
    signer: Signer,
    storage: PrikkCli,
    wiki: WikiId,
    repo: PathBuf,
    proposer: Actor,
    acceptor: Actor,
    first_block: String,
}

fn account(name: &str) -> Actor {
    Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new(name).unwrap(),
    }
}

fn world(tag: &str) -> Option<World> {
    if !prikk_on_path() {
        return None;
    }
    let shm_base = PathBuf::from("/dev/shm");
    if !shm_base.is_dir() {
        println!("skipping: no /dev/shm here");
        return None;
    }
    let tmp = dir(&std::env::temp_dir(), tag);
    let shm = dir(&shm_base, tag);
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
    let root = RootLayout::new(tmp.0.join("root"));
    let books = Arc::new(Books::default());
    let mut cfg = SignerConfig::new(
        "prikk",
        Sandbox::Unconfined,
        root.clone(),
        tmp.0.join("keys"),
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, books.clone()).expect("signer");
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).unwrap();
    signer
        .create_repository(&repo)
        .expect("repository and keys");
    for (path, text) in [
        (
            "wiki-bundle.yaml",
            "format: portable-wiki-bundle\nversion: '0.4'\n",
        ),
        ("pages/Home.md", "---\ntitle: Home\n---\n\nHello.\n"),
    ] {
        let p = repo.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let create = Message::system(
        Summary::from_text("Create the wiki").unwrap(),
        SystemLabel::inklkunn(),
    );
    let sealed = signer
        .commit_and_seal(&repo, &create.to_text())
        .expect("first change");
    books.acts.lock().unwrap().push(SystemActRecord {
        id: "create".into(),
        block_id: Some(sealed.block_id.clone()),
        facts: ExpectedMessage {
            wiki: wiki.clone(),
            view: "heads/main".into(),
            kind: Kind::System,
            proposal: None,
            proposed_by: Actor::System(SystemLabel::inklkunn()),
            accepted_by: Actor::System(SystemLabel::inklkunn()),
            base: None,
            imported_from: None,
        },
    });
    let storage = PrikkCli::open(
        "prikk",
        Sandbox::Unconfined,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("prikk at the floor");
    Some(World {
        _tmp: tmp,
        _shm: shm,
        books,
        signer,
        storage,
        wiki,
        repo,
        proposer: account("Aiko"),
        acceptor: account("Ben"),
        first_block: sealed.block_id,
    })
}

impl World {
    /// Accept a proposal for real and return the record the service would keep of it; the record joins the
    /// books only if `record_it`.
    fn accept(&self, n: usize, record_it: bool) -> AcceptedRecord {
        let proposal = ProposalId::new();
        let message = Message::proposal(
            Summary::from_text(&format!("Change {n}")).unwrap(),
            proposal.clone(),
            self.proposer.clone(),
            self.acceptor.clone(),
            BlockId::new(&self.first_block).unwrap(),
        )
        .unwrap();
        let facts = ExpectedMessage {
            wiki: self.wiki.clone(),
            view: "heads/main".into(),
            kind: message.kind(),
            proposal: Some(proposal.clone()),
            proposed_by: self.proposer.clone(),
            accepted_by: self.acceptor.clone(),
            base: message.base().cloned(),
            imported_from: None,
        };
        let token = AcceptanceToken::new(format!("acceptance-{proposal}"));
        self.books
            .pending
            .lock()
            .unwrap()
            .insert(token.as_str().to_owned(), facts.clone());
        let change = PreparedChange {
            files: vec![PreparedFile {
                path: PathBuf::from(format!("pages/Page{n}.md")),
                content: Some(format!("page {n}\n").into_bytes()),
            }],
            renames: vec![],
            message: message.to_text(),
        };
        let sealed = self
            .signer
            .accept(&self.wiki, "heads/main", &token, &change)
            .expect("accepted");
        let record = AcceptedRecord {
            id: format!("acc-{n}"),
            block_id: sealed.block_id,
            facts,
        };
        if record_it {
            self.books.accepted.lock().unwrap().push(record.clone());
        }
        record
    }

    fn tip(&self) -> Vec<String> {
        self.storage
            .history(&self.repo, &Point::View("heads/main".into()), None)
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.block_id)
            .collect()
    }

    fn reconcile(&self) -> inklkunn_core::reconcile::Report {
        reconcile(&self.wiki, &self.repo, &self.storage, self.books.as_ref()).expect("reconciled")
    }
}

#[test]
fn a_real_wikis_history_reconciles_cleanly_with_its_records() {
    let Some(w) = world("clean") else {
        println!("skipping: prikk not on PATH");
        return;
    };
    w.accept(1, true);
    w.accept(2, true);
    let r = w.reconcile();
    assert!(r.is_clean(), "{:?}", r.findings);
    assert_eq!((r.blocks, r.accepted, r.system_acts), (3, 2, 1));
}

#[test]
fn an_unrecorded_seal_and_an_unsealed_acceptance_are_both_reported_and_nothing_is_changed() {
    let Some(w) = world("planted") else {
        println!("skipping: prikk not on PATH");
        return;
    };
    let recorded = w.accept(1, true);
    // A seal the records know nothing about (an acceptance the books lost, or a change made around the service).
    let unrecorded = w.accept(2, false);
    // An acceptance the records say was done that is not in the history.
    let ghost_block = format!("{:064x}", 0xdead_beef_u64);
    w.books.accepted.lock().unwrap().push(AcceptedRecord {
        id: "acc-ghost".into(),
        block_id: ghost_block.clone(),
        facts: recorded.facts.clone(),
    });
    let before = w.tip();
    let branch_before = std::fs::read(w.repo.join(".prikk/current-branch")).unwrap();
    let r = w.reconcile();
    assert_eq!(r.findings.len(), 2, "{:?}", r.findings);
    assert!(
        r.findings.iter().any(|f| matches!(f, Finding::UnrecordedSeal { block_id, .. } if *block_id == unrecorded.block_id)),
        "{:?}",
        r.findings
    );
    assert!(r.findings.contains(&Finding::MissingBlock {
        record_id: "acc-ghost".into(),
        block_id: ghost_block
    }));
    // It changed nothing: the same history, the same branch, and the audit trail holds ids only.
    assert_eq!(w.tip(), before);
    assert_eq!(
        std::fs::read(w.repo.join(".prikk/current-branch")).unwrap(),
        branch_before
    );
    let audit = MemoryAudit::new();
    record_findings(&r, &[], &audit, 1_700_000_000).unwrap();
    let events = audit.events();
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|e| e.outcome == AuditOutcome::Reported && e.action == "reconcile")
    );
    assert!(
        events
            .iter()
            .all(|e| !e.detail.contains("Aiko") && !e.detail.contains("Change"))
    );
    // The findings were about the records, and nothing was repaired: put the records right and it is clean.
    {
        let mut books = w.books.accepted.lock().unwrap();
        books.retain(|a| a.id != "acc-ghost");
        books.push(unrecorded);
    }
    let again = w.reconcile();
    assert!(again.is_clean(), "{:?}", again.findings);
    assert_eq!(w.tip(), before);
}

#[test]
fn a_history_longer_than_the_first_read_is_reconciled_in_full_against_a_real_prikk() {
    // The regression this guards: paging a history by block id, which real prikk answers with an empty list
    // and exit 0, once made the reconciliation look at the newest page only and call the rest clean (or, with
    // records for the rest, call them all missing). The read here is made to grow from 64 blocks.
    let Some(w) = world("long") else {
        println!("skipping: prikk not on PATH");
        return;
    };
    for n in 1..=150 {
        w.accept(n, true);
    }
    let bounds = ReadBounds {
        first_read: 64,
        max_read: 100_000,
    };
    let read = || {
        reconcile_with(&w.wiki, &w.repo, &w.storage, w.books.as_ref(), bounds).expect("reconciled")
    };
    let r = read();
    assert!(
        r.is_clean(),
        "{} findings, the first {:?}",
        r.findings.len(),
        r.findings.first()
    );
    assert_eq!((r.blocks, r.accepted, r.system_acts), (151, 150, 1));
    // An acceptance whose record is lost, far beyond the first read, is still found.
    w.books.accepted.lock().unwrap().remove(0);
    let r = read();
    assert_eq!(r.findings.len(), 1, "{:?}", r.findings);
    assert!(
        matches!(r.findings.first(), Some(Finding::UnrecordedSeal { .. })),
        "{:?}",
        r.findings
    );
    // And a bound that is too small is an error, not a quiet cut.
    let tiny = ReadBounds {
        first_read: 16,
        max_read: 64,
    };
    assert!(matches!(
        reconcile_with(&w.wiki, &w.repo, &w.storage, w.books.as_ref(), tiny),
        Err(inklkunn_core::reconcile::ReconcileError::TooLarge { .. })
    ));
}
