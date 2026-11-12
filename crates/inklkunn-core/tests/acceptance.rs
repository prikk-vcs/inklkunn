//! Acceptance under the lease against a real repository (RFC 004 D-4, D-6, D-14; handoff 004 T4): wikis created
//! and written by the real signer, read through the real driver, accepted by the service's own orchestration,
//! with the records in a real (SQLite) store. Self-skips with a visible line when prikk is not on `PATH`.
//!
//! Failures after `commit` need a prikk that fails on cue, and a race needs one that waits: a **stand-in**
//! script passes every call to the real prikk unless a control file names the verb (the signer's own tests use
//! the same device). The stand-in runs only unconfined.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use inklkunn_core::import::{plan_folder, run_import, wiki_state};
use inklkunn_core::lease::Leased;
use inklkunn_core::message::{Kind, Message};
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::reconcile::{Report, reconcile};
use inklkunn_core::{
    AcceptFailure, AcceptanceParts, Accepted, Authorizer, BusyWhy, IdentityAuthority, KeyAuthority,
    LeaseConfig, LeaseError, Maintenance, Principal, ProposalAcceptor, Purpose, ReadConfig,
    ReadService, Reconciled, Signer, SignerConfig, StoreActs, StoreAuthz, StoreLedger, SystemActs,
    WikiLeases, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceStore, Account, AccountId, AccountStore, ActStore, DisplayName, FileChange, NewWiki,
    ProposalFilter, ProposalRecord, ProposalState, RecoveryReason, Role, RoleGrant, RoleStore,
    Slug, SqliteStore, SystemActId, SystemActKind,
};

const MAIN: &str = "heads/main";

/// The process's one identity authority (it can be claimed once, and a test binary is one process).
fn authority() -> &'static IdentityAuthority {
    static AUTHORITY: std::sync::OnceLock<IdentityAuthority> = std::sync::OnceLock::new();
    AUTHORITY.get_or_init(|| IdentityAuthority::claim().expect("claimed once"))
}

fn real_prikk() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    })
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-acc-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

/// The stand-in prikk: every call goes to the real one, after logging the verb, unless a control file says
/// otherwise (`fail-<verb>` fails it; `delay-<verb>` holds it for the seconds the file names).
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
REAL="{real}"
verb="$1"
echo "$verb" >> "$CTL/calls.log"
if [ -e "$CTL/fail-$verb" ]; then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
if [ -e "$CTL/delay-$verb" ]; then : > "$CTL/inside-$verb"; sleep "$(cat "$CTL/delay-$verb")"; rm -f "$CTL/inside-$verb"; fi
exec "$REAL" "$@"
"#,
        ctl = ctl.display(),
        real = real.display()
    );
    let path = ctl.join("prikk");
    std::fs::write(&path, script).expect("stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

const HOME: &str = "---\ntitle: Home\nid: p-home\n---\n\nOne.\n\nTwo.\n\nThree.\n";
const OTHER: &str = "---\ntitle: Other\nid: p-other\n---\n\nHello.\n";

struct World {
    _tmp: Dir,
    _shm: Dir,
    ctl: PathBuf,
    store: Arc<SqliteStore>,
    storage: Arc<PrikkCli>,
    leases: Arc<WikiLeases>,
    write_leases: Arc<WriteLeases>,
    reads: Arc<ReadService>,
    proposals: Arc<ProposalService>,
    acceptor: Arc<ProposalAcceptor>,
    signer: Arc<Signer>,
    acts: SystemActs,
    standin: PathBuf,
    layout: inklkunn_store::RootLayout,
    wiki: inklkunn_store::WikiRecord,
    repo: PathBuf,
    maintainer: Principal,
    maintainer_id: AccountId,
    contributor: Principal,
    contributor_id: AccountId,
}

impl World {
    /// A wiki whose history is: the creation, and one import of `pages/Home.md` and `pages/Other.md`.
    fn new(tag: &str, lease: LeaseConfig) -> Option<World> {
        let Some(real) = real_prikk() else {
            println!("skipping: prikk not on PATH");
            return None;
        };
        if !Path::new("/dev/shm").is_dir() {
            println!("skipping: no /dev/shm here");
            return None;
        }
        let tmp = dir_in(&std::env::temp_dir(), tag);
        let shm = dir_in(Path::new("/dev/shm"), tag);
        let ctl = tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let standin = write_standin(&ctl, &real);
        let secret = tmp.0.join("secret");
        std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
        let layout = inklkunn_store::RootLayout::new(tmp.0.join("root"));
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let write_leases = WriteLeases::new(layout.clone());
        let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
        let mut cfg = SignerConfig::new(
            &standin,
            Sandbox::Unconfined,
            layout.clone(),
            tmp.0.join("keys"),
            &secret,
            &shm.0,
        );
        cfg.cheap_derivation = true;
        let signer = Arc::new(
            Signer::open_creating_store(cfg, ledger)
                .expect("signer")
                .with_leases(&write_leases),
        );
        let storage = Arc::new(
            PrikkCli::open(
                "prikk",
                Sandbox::Unconfined,
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
            )
            .expect("prikk at the floor"),
        );
        let created = create_wiki(
            &*store,
            &*store,
            &layout,
            &*signer,
            &*storage,
            NewWiki::new(
                Slug::new(tag).unwrap(),
                DisplayName::new(&format!("Wiki {tag}")).unwrap(),
            ),
            "0.0.0",
            &KeyAuthority::for_tests(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let wiki = created.record.clone();
        // `create_wiki` keeps the creation as a sealed system act, so the first block is a recorded one.

        let leases = WikiLeases::new(
            write_leases.clone(),
            store.clone(),
            "acceptance-test",
            lease,
        );
        let authorizer = || {
            Authorizer::new(
                Arc::new(StoreAuthz::new(store.clone(), store.clone())),
                store.clone(),
            )
        };
        let reads = Arc::new(ReadService::new(
            storage.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            ReadConfig::default(),
            None,
        ));
        let proposals = Arc::new(ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            authorizer(),
            reads.clone(),
            QuotaConfig::default(),
        ));
        let acceptor = Arc::new(
            ProposalAcceptor::new(AcceptanceParts {
                proposals: proposals.clone(),
                acceptances: store.clone(),
                acts: store.clone(),
                roles: store.clone(),
                accounts: store.clone(),
                storage: storage.clone(),
                layout: layout.clone(),
                signer: signer.clone(),
                leases: leases.clone(),
                reads: reads.clone(),
            })
            .with_audit(store.clone())
            .with_notifier(Arc::new(inklkunn_core::notify::Notifier::new(
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
                authorizer(),
            ))),
        );
        let acts = SystemActs::new(
            authorizer(),
            store.clone(),
            store.clone(),
            storage.clone(),
            layout.clone(),
            signer.clone(),
            leases.clone(),
        )
        .with_reads(reads.clone())
        .with_audit(store.clone());
        let authority = authority();
        let person = |name: &str, role: Role| {
            let id = AccountId::new();
            store
                .add_account(&Account {
                    id: id.clone(),
                    display_name: DisplayName::new(name).unwrap(),
                    local: None,
                    created_at: 1,
                    disabled: false,
                })
                .unwrap();
            store
                .grant(RoleGrant {
                    account: id.clone(),
                    wiki: wiki.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: 1,
                })
                .unwrap();
            id
        };
        let maintainer_id = person("Maya Maintainer", Role::Maintainer);
        let contributor_id = person("Noor Contributor", Role::Contributor);
        let world = World {
            maintainer: Principal::session(authority, maintainer_id.clone()),
            contributor: Principal::session(authority, contributor_id.clone()),
            _tmp: tmp,
            _shm: shm,
            ctl,
            store,
            storage,
            leases,
            write_leases,
            reads,
            proposals,
            acceptor,
            signer,
            acts,
            standin,
            layout,
            repo: created.repo,
            wiki,
            maintainer_id,
            contributor_id,
        };
        world.import(&[("pages/Home.md", HOME), ("pages/Other.md", OTHER)]);
        Some(world)
    }

    /// `import-folder` of these pages, through the service's own path.
    fn import(&self, pages: &[(&str, &str)]) -> inklkunn_core::import::Imported {
        let folder = dir_in(
            &std::env::temp_dir(),
            &format!("folder-{}", SystemActId::new()),
        );
        for (path, text) in pages {
            let p = folder.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let have = wiki_state(&*self.storage, &self.layout, &self.wiki).unwrap();
        let plan = plan_folder(&folder.0, &have, false).unwrap();
        assert!(plan.is_importable(), "{}", plan.report());
        run_import(&self.acts, &self.wiki, &plan).unwrap_or_else(|e| panic!("{e}"))
    }

    fn control(&self, name: &str, content: &str) {
        std::fs::write(self.ctl.join(name), content).unwrap();
    }

    fn uncontrol(&self, name: &str) {
        let _ = std::fs::remove_file(self.ctl.join(name));
    }

    /// The verbs the stand-in was asked to run, in order.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.ctl.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn history(&self) -> Vec<inklkunn_prikk::HistoryBlock> {
        self.storage
            .history(&self.repo, &Point::View(MAIN.to_owned()), None)
            .unwrap()
            .blocks
    }

    fn tip(&self) -> String {
        self.history().first().unwrap().block_id.clone()
    }

    fn page(&self, path: &str) -> String {
        String::from_utf8(
            self.storage
                .read(&self.repo, &Point::Block(self.tip()), path, 1 << 20)
                .unwrap(),
        )
        .unwrap()
    }

    fn propose(&self, summary: &str, files: &[(&str, Option<&str>)]) -> ProposalRecord {
        let edit = ProposalEdit {
            summary: summary.to_owned(),
            files: files
                .iter()
                .map(|(path, text)| FileChange {
                    path: (*path).to_owned(),
                    text: text.map(str::to_owned),
                })
                .collect(),
            ..ProposalEdit::default()
        };
        let draft = self
            .proposals
            .create(&self.contributor, &self.wiki, None, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        self.proposals
            .submit(&self.contributor, &self.wiki, &draft.id)
            .unwrap_or_else(|e| panic!("submit: {e}"))
    }

    fn accept(&self, p: &ProposalRecord) -> Result<Accepted, AcceptFailure> {
        self.acceptor.accept(&self.maintainer, &self.wiki, &p.id, 1)
    }

    fn state(&self, p: &ProposalRecord) -> ProposalState {
        self.proposals
            .get(&self.maintainer, &self.wiki, &p.id)
            .unwrap()
            .state
    }

    fn reconciled(&self) -> Report {
        let records = StoreActs::new(self.store.clone(), self.store.clone());
        reconcile(&self.wiki.id, &self.repo, &*self.storage, &records).unwrap()
    }

    fn maintenance(&self) -> Maintenance {
        Maintenance::new(&self.standin, Sandbox::Unconfined, self.layout.clone())
            .with_leases(&self.write_leases)
    }
}

fn signed(a: Accepted) -> inklkunn_core::Signed {
    match a {
        Accepted::Signed(s) => s,
        other => panic!("{other:?}"),
    }
}

fn quick(wait_ms: u64) -> LeaseConfig {
    LeaseConfig {
        max_wait: Duration::from_millis(wait_ms),
        poll: Duration::from_millis(10),
        ..LeaseConfig::default()
    }
}

#[test]
fn an_acceptance_whose_base_is_the_tip_seals_and_the_log_shows_the_message() {
    let Some(w) = World::new("tip", LeaseConfig::default()) else {
        return;
    };
    let before = w.history().len();
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let done = signed(w.accept(&p).unwrap());
    assert!(!done.reauthored);
    assert_eq!(done.reconciliation, Reconciled::Clean);
    // One new block, at the tip, and `prikk log` carries the CT-03 message the service composed.
    let log = w.history();
    assert_eq!(log.len(), before + 1);
    let newest = log.first().unwrap();
    assert_eq!(newest.block_id, done.block_id);
    let text = &newest.patch_messages.first().unwrap().message;
    let message = Message::parse(text).unwrap();
    assert_eq!(message.kind(), Kind::Proposal);
    assert_eq!(message.proposal_id(), Some(&p.id));
    assert_eq!(message.summary().as_str(), "Say it in Spanish");
    assert!(text.contains(&format!(
        "Proposed-By: {} (Noor Contributor)",
        w.contributor_id
    )));
    assert!(text.contains(&format!(
        "Accepted-By: {} (Maya Maintainer)",
        w.maintainer_id
    )));
    assert!(w.page("pages/Home.md").contains("Uno."));
    // The records agree: accepted, sealed with the block, no flag, the lease record gone.
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.leases.recorded_holder(&w.wiki.id).unwrap(), None);
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_moved_base_with_no_overlap_is_re_authored_and_seals() {
    let Some(w) = World::new("moved", LeaseConfig::default()) else {
        return;
    };
    // Two proposals against the same tip, on different paragraphs of one page and on another page.
    let first = w.propose(
        "First paragraph",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let second = w.propose(
        "Third paragraph, and another page",
        &[
            ("pages/Home.md", Some(&HOME.replace("Three.", "Tres."))),
            ("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola."))),
        ],
    );
    signed(w.accept(&first).unwrap());
    let before = w.history().len();
    // The tip has moved since the second was made: it is re-authored, without its proposer.
    let done = signed(w.accept(&second).unwrap());
    assert!(done.reauthored, "the base moved");
    assert_eq!(done.reconciliation, Reconciled::Clean);
    assert_eq!(w.history().len(), before + 1);
    let home = w.page("pages/Home.md");
    assert!(
        home.contains("Uno.") && home.contains("Tres.") && home.contains("Two."),
        "both edits are in: {home}"
    );
    assert!(w.page("pages/Other.md").contains("Hola."));
    // The message names the block the proposer saw, not the tip it landed on.
    let newest = w.history().remove(0);
    let message = Message::parse(&newest.patch_messages.first().unwrap().message).unwrap();
    let revisions = w
        .proposals
        .revisions(&w.maintainer, &w.wiki, &second.id)
        .unwrap();
    assert_eq!(
        message.base().map(|b| b.as_str().to_owned()),
        Some(revisions.first().unwrap().content.base.block_id.clone())
    );
    assert_eq!(w.state(&second), ProposalState::Accepted);
    assert!(w.reconciled().is_clean());
}

#[test]
fn an_overlap_stops_before_commit_records_both_texts_and_the_repository_is_unchanged() {
    let Some(w) = World::new("overlap", LeaseConfig::default()) else {
        return;
    };
    let mine = w.propose(
        "Uno",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let yours = w.propose(
        "Eins",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Eins.")))],
    );
    signed(w.accept(&mine).unwrap());
    let (log_before, tip_before, page_before, calls_before) =
        (w.history(), w.tip(), w.page("pages/Home.md"), w.calls());
    let err = w.accept(&yours).unwrap_err();
    assert_eq!(err, AcceptFailure::Overlap { overlaps: 1 });
    // Nothing in prikk changed: not a block, not a verb run against the repository by the signer.
    assert_eq!(w.history(), log_before);
    assert_eq!(
        (w.tip(), w.page("pages/Home.md")),
        (tip_before.clone(), page_before)
    );
    assert_eq!(w.calls(), calls_before, "the signer never ran a prikk verb");
    assert!(w.storage.verify(&w.repo).unwrap().verdict.ok);
    // Both texts are on the proposal; it needs a revision; no acceptance, no intent, no flag.
    assert_eq!(w.state(&yours), ProposalState::NeedsRevision);
    let recorded = w.store.overlaps_of(&yours.id).unwrap();
    let one = recorded.first().unwrap();
    assert_eq!(one.tip_block, tip_before);
    let text = one.texts.first().unwrap();
    assert!(text.theirs.as_deref().unwrap().contains("Uno."));
    assert!(text.ours.as_deref().unwrap().contains("Eins."));
    assert!(text.base.as_deref().unwrap().contains("One."));
    assert!(w.store.acceptances_of(&yours.id, 1).unwrap().is_empty());
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.leases.recorded_holder(&w.wiki.id).unwrap(), None);
    assert!(w.reconciled().is_clean());
}

#[test]
fn two_acceptances_racing_on_one_wiki_serialize_and_the_second_is_told_busy_with_a_depth() {
    let Some(w) = World::new("race", quick(200)) else {
        return;
    };
    let first = w.propose(
        "First",
        &[("pages/A.md", Some("---\ntitle: A\nid: pa\n---\n\na\n"))],
    );
    let second = w.propose(
        "Second",
        &[("pages/B.md", Some("---\ntitle: B\nid: pb\n---\n\nb\n"))],
    );
    // The first acceptance sits in `seal` for a while.
    w.control("delay-seal", "1.5");
    let w = Arc::new(w);
    let one = {
        let (w, p) = (w.clone(), first.clone());
        std::thread::spawn(move || w.accept(&p))
    };
    let start = Instant::now();
    while !w.ctl.join("inside-seal").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the first never reached `seal`"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let err = w.accept(&second).unwrap_err();
    let AcceptFailure::Busy(busy) = &err else {
        panic!("{err:?}")
    };
    assert_eq!((busy.depth, busy.why.clone()), (1, BusyWhy::Waited));
    assert!(busy.estimate > Duration::ZERO);
    // While it is inside, the durable record names the holder and the acceptance.
    let held = w.leases.recorded_holder(&w.wiki.id).unwrap().unwrap();
    assert_eq!(
        (
            held.purpose.as_str(),
            held.holder.rsplit_once('@').map_or("", |(name, _)| name)
        ),
        ("acceptance", "acceptance-test")
    );
    assert!(held.acceptance.is_some());
    assert!(
        w.store.acceptances_of(&second.id, 1).unwrap().is_empty(),
        "the second did nothing"
    );
    signed(one.join().unwrap().unwrap());
    assert_eq!(w.leases.recorded_holder(&w.wiki.id).unwrap(), None);
    // Told busy, nothing was lost: its turn comes.
    w.uncontrol("delay-seal");
    signed(w.accept(&second).unwrap());
    assert_eq!(w.state(&first), ProposalState::Accepted);
    assert_eq!(w.state(&second), ProposalState::Accepted);
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_retried_accept_of_the_same_revision_seals_once() {
    let Some(w) = World::new("retry", LeaseConfig::default()) else {
        return;
    };
    let before = w.history().len();
    let p = w.propose(
        "Once",
        &[(
            "pages/Once.md",
            Some("---\ntitle: Once\nid: po\n---\n\nx\n"),
        )],
    );
    let first = signed(w.accept(&p).unwrap());
    let again = w.accept(&p).unwrap();
    assert_eq!(
        again,
        Accepted::AlreadySigned {
            acceptance: first.acceptance.clone(),
            block_id: first.block_id.clone(),
            patch_id: first.patch_id.clone(),
        }
    );
    assert_eq!(w.history().len(), before + 1, "one block in the log");
    assert_eq!(
        w.calls().iter().filter(|v| *v == "commit").count(),
        3,
        "creation, import, this one"
    );
    // The same, with the retry arriving while the first is still inside: it waits its turn and finds the record.
    let q = w.propose(
        "Twice at once",
        &[(
            "pages/Twice.md",
            Some("---\ntitle: Twice\nid: pt\n---\n\ny\n"),
        )],
    );
    w.control("delay-seal", "0.8");
    let w = Arc::new(w);
    let a = {
        let (w, q) = (w.clone(), q.clone());
        std::thread::spawn(move || w.accept(&q))
    };
    let start = Instant::now();
    while !w.ctl.join("inside-seal").exists() {
        assert!(start.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(10));
    }
    let b = w.accept(&q).unwrap();
    assert!(matches!(b, Accepted::AlreadySigned { .. }), "{b:?}");
    signed(a.join().unwrap().unwrap());
    assert_eq!(
        w.history().len(),
        before + 2,
        "two proposals, two blocks, however many retries"
    );
    assert!(w.reconciled().is_clean());
}

#[test]
fn an_unauthorized_acceptor_is_refused_before_the_lease_is_taken() {
    let Some(w) = World::new("unauthorized", quick(30_000)) else {
        return;
    };
    let p = w.propose(
        "Mine",
        &[(
            "pages/Mine.md",
            Some("---\ntitle: Mine\nid: pm\n---\n\nm\n"),
        )],
    );
    // Someone else holds the lease for a long time: any taker would wait 30 s.
    let held = w.leases.acquire(&w.wiki.id, Purpose::Export, None).unwrap();
    let before = w.leases.recorded_holder(&w.wiki.id).unwrap();
    let started = Instant::now();
    for who in [&w.contributor, &Principal::Anonymous] {
        let err = w.acceptor.accept(who, &w.wiki, &p.id, 1).unwrap_err();
        assert_eq!(err, AcceptFailure::NotAllowed);
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it waited for the lease"
    );
    assert_eq!(w.leases.waiting(&w.wiki.id), 0);
    assert_eq!(
        w.leases.recorded_holder(&w.wiki.id).unwrap(),
        before,
        "the holder record is the holder's"
    );
    drop(held);
    assert!(w.store.acceptances_of(&p.id, 1).unwrap().is_empty());
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
}

#[test]
fn a_failure_injected_before_commit_leaves_the_repository_unchanged_and_the_record_failed_with_the_word()
 {
    let Some(w) = World::new("before", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Fails",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    let (log_before, page_before) = (w.history(), w.page("pages/Home.md"));
    w.control("fail-commit", "");
    let err = w.accept(&p).unwrap_err();
    let AcceptFailure::Failed {
        acceptance, word, ..
    } = &err
    else {
        panic!("{err:?}")
    };
    assert!(matches!(*word, "refused" | "unavailable"), "{word}");
    // The repository is as it was: the same history, the same page at the tip and in the worktree.
    assert_eq!(w.history(), log_before);
    assert_eq!(w.page("pages/Home.md"), page_before);
    assert_eq!(
        std::fs::read_to_string(w.repo.join("pages/Home.md")).unwrap(),
        page_before
    );
    assert!(w.storage.verify(&w.repo).unwrap().verdict.ok);
    // The record says failed, with the word; the intent is abandoned; the proposal stays open; no flag.
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert_eq!(&entry.acceptance.id, acceptance);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(inklkunn_store::AcceptanceOutcome::Failed { word: stored }) if stored == *word
    ));
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    // And the acceptor may try again once prikk is well: a failed record does not stand in the way.
    w.uncontrol("fail-commit");
    signed(w.accept(&p).unwrap());
    assert_eq!(w.store.acceptances_of(&p.id, 1).unwrap().len(), 2);
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_failure_injected_after_commit_flags_the_wiki_and_reads_still_serve() {
    let Some(w) = World::new("after", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Half done",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    let q = w.propose(
        "Another",
        &[(
            "pages/Another.md",
            Some("---\ntitle: Another\nid: pn\n---\n\na\n"),
        )],
    );
    let (log_before, page_before) = (w.history(), w.page("pages/Home.md"));
    w.control("fail-seal", "");
    let err = w.accept(&p).unwrap_err();
    let AcceptFailure::Incomplete {
        reason,
        flagged,
        acceptance,
        ..
    } = &err
    else {
        panic!("{err:?}")
    };
    assert_eq!((*reason, *flagged), (RecoveryReason::Queued, true));
    assert_eq!(err.word(), "needs-recovery");
    w.uncontrol("fail-seal");
    // Committed and not sealed: the history has nothing new, and the records say what recovery needs.
    assert_eq!(w.history(), log_before);
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert_eq!((&entry.acceptance.id, entry.result), (acceptance, None));
    assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1);
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    let flag = w.store.recovery_flag(&w.wiki.id).unwrap().unwrap();
    assert_eq!(flag.reason, RecoveryReason::Queued);
    // Reads still serve, from the history that is there.
    let home = w
        .reads
        .raw(&w.wiki, None, "Home")
        .unwrap()
        .expect("the page is served");
    assert_eq!(String::from_utf8(home.bytes).unwrap(), page_before);
    assert_eq!(
        w.reads.tip(&w.wiki, None).unwrap().block_id,
        log_before.first().unwrap().block_id
    );
    // Writes are refused with the word, for this proposal and for any other, and nothing is committed again.
    let commits = w.calls().iter().filter(|v| *v == "commit").count();
    assert!(
        matches!(w.accept(&p).unwrap_err(), AcceptFailure::NeedsRecovery(f) if f.reason == RecoveryReason::Queued)
    );
    assert!(matches!(
        w.accept(&q).unwrap_err(),
        AcceptFailure::NeedsRecovery(_)
    ));
    assert_eq!(w.calls().iter().filter(|v| *v == "commit").count(), commits);
    assert_eq!(w.history(), log_before);
    // The system's own path refuses too: one write path, one guard.
    let folder = dir_in(&std::env::temp_dir(), "after-folder");
    std::fs::create_dir_all(folder.0.join("pages")).unwrap();
    std::fs::write(
        folder.0.join("pages/New.md"),
        "---\ntitle: New\nid: pnew\n---\n\nn\n",
    )
    .unwrap();
    let have = wiki_state(&*w.storage, &w.layout, &w.wiki).unwrap();
    let plan = plan_folder(&folder.0, &have, false).unwrap();
    let err = run_import(&w.acts, &w.wiki, &plan).unwrap_err();
    assert!(
        matches!(
            err,
            inklkunn_core::import::ImportError::Act(
                inklkunn_core::SystemActFailure::NeedsRecovery(_)
            )
        ),
        "{err}"
    );
}

#[test]
fn import_folder_produces_one_block_with_kind_import() {
    let Some(w) = World::new("import", LeaseConfig::default()) else {
        return;
    };
    let before = w.history().len();
    let done = w.import(&[
        (
            "pages/Imported.md",
            "---\ntitle: Imported\nid: pimp\n---\n\nFrom a folder.\n",
        ),
        (
            "pages/Imported Two.md",
            "---\ntitle: Imported Two\nid: pimp2\n---\n\nAnd another.\n",
        ),
    ]);
    let log = w.history();
    assert_eq!(log.len(), before + 1, "one block for the whole folder");
    let newest = log.first().unwrap();
    assert_eq!(newest.block_id, done.sealed.block_id);
    assert_eq!(newest.patch_messages.len(), 1);
    let text = &newest.patch_messages.first().unwrap().message;
    let message = Message::parse(text).unwrap();
    assert_eq!(message.kind(), Kind::Import);
    assert!(text.contains("Kind: import\n"), "{text}");
    assert!(
        text.contains(&format!(
            "Imported-From: folder sha256:{}",
            done.digest.to_hex()
        )),
        "{text}"
    );
    assert!(w.page("pages/Imported.md").contains("From a folder."));
    // The act is recorded and sealed, and the reconciliation finds it.
    let acts = w.store.system_acts(&w.wiki.id).unwrap();
    let ours = acts.iter().find(|a| a.act.id == done.act).unwrap();
    assert_eq!(ours.act.kind, SystemActKind::Import);
    assert!(matches!(
        ours.end.as_ref().map(|e| &e.outcome),
        Some(inklkunn_store::AcceptanceOutcome::Sealed { block_id, .. }) if *block_id == done.sealed.block_id
    ));
    assert!(w.reconciled().is_clean());
}

#[test]
fn an_archive_export_and_a_checkpoint_refresh_hold_the_wiki_lease_and_an_acceptance_waits_for_them()
{
    let Some(w) = World::new("maintenance", quick(200)) else {
        return;
    };
    let maintenance = Arc::new(w.maintenance());
    let dest = w._tmp.0.join("export.prikk");
    // An export in progress: an acceptance is told busy, and the holder record says why.
    w.control("delay-archive", "1.5");
    let w = Arc::new(w);
    let export = {
        let (w, m, dest) = (w.clone(), maintenance.clone(), dest.clone());
        std::thread::spawn(move || {
            w.leases
                .export_archive(&m, &w.wiki.id, &dest)
                .map(|d| d.bytes)
        })
    };
    let start = Instant::now();
    while !w.ctl.join("inside-archive").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the export never started"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let p = w.propose(
        "During the export",
        &[(
            "pages/During.md",
            Some("---\ntitle: During\nid: pd\n---\n\nd\n"),
        )],
    );
    let err = w.accept(&p).unwrap_err();
    assert!(matches!(err, AcceptFailure::Busy(_)), "{err:?}");
    assert_eq!(
        w.leases
            .recorded_holder(&w.wiki.id)
            .unwrap()
            .unwrap()
            .purpose,
        "export"
    );
    // A second export or a refresh is told busy as well.
    assert!(matches!(
        w.leases.refresh_checkpoint(&maintenance, &w.wiki.id),
        Err(Leased::NotLeased(LeaseError::Busy(_)))
    ));
    let bytes = export.join().unwrap().unwrap();
    assert!(bytes > 0 && dest.is_file());
    // Free again: the refresh runs under the lease, and then the acceptance seals (no `lock conflict`).
    w.uncontrol("delay-archive");
    assert!(
        w.leases
            .refresh_checkpoint(&maintenance, &w.wiki.id)
            .unwrap()
            .verdict
            .ok
    );
    assert_eq!(w.leases.recorded_holder(&w.wiki.id).unwrap(), None);
    signed(w.accept(&p).unwrap());
    assert!(w.reconciled().is_clean());
}

#[test]
fn the_reconciliation_finds_nothing_after_every_kind_of_act() {
    let Some(w) = World::new("whole", LeaseConfig::default()) else {
        return;
    };
    // An import (World::new made one), an acceptance, a re-authored acceptance, an overlap, a failure before
    // commit that was retried, a duplicate retry, and a second import.
    let a = w.propose(
        "A",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let b = w.propose(
        "B",
        &[("pages/Home.md", Some(&HOME.replace("Three.", "Tres.")))],
    );
    let c = w.propose(
        "C",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Eins.")))],
    );
    signed(w.accept(&a).unwrap());
    assert!(signed(w.accept(&b).unwrap()).reauthored);
    assert!(matches!(
        w.accept(&c).unwrap_err(),
        AcceptFailure::Overlap { .. }
    ));
    let d = w.propose(
        "D",
        &[("pages/D.md", Some("---\ntitle: D\nid: pdd\n---\n\nd\n"))],
    );
    w.control("fail-commit", "");
    assert!(matches!(
        w.accept(&d).unwrap_err(),
        AcceptFailure::Failed { .. }
    ));
    w.uncontrol("fail-commit");
    signed(w.accept(&d).unwrap());
    assert!(matches!(
        w.accept(&d).unwrap(),
        Accepted::AlreadySigned { .. }
    ));
    w.import(&[("pages/E.md", "---\ntitle: E\nid: pe\n---\n\ne\n")]);
    let report = w.reconciled();
    assert!(report.is_clean(), "{:?}", report.findings);
    // creation, import, a, b (re-authored), d, import = 6 blocks; 3 accepted records; 3 system acts
    assert_eq!(
        (report.blocks, report.accepted, report.system_acts),
        (6, 3, 3)
    );
    // Every proposal is accounted for as the records say.
    let proposals = w
        .proposals
        .list(&w.maintainer, &w.wiki, &ProposalFilter::default())
        .unwrap();
    assert_eq!(proposals.len(), 4);
    let states: Vec<_> = proposals.iter().map(|p| p.state).collect();
    assert_eq!(
        states,
        vec![
            ProposalState::Accepted,
            ProposalState::Accepted,
            ProposalState::NeedsRevision,
            ProposalState::Accepted
        ]
    );
}

// ------------------------------------------------------------------------------------ the knowledge layer (RFC 005 part 2 D-5)

#[test]
fn a_claim_with_its_source_and_its_block_is_signed_and_an_unsound_change_is_refused_before_the_signer()
 {
    use inklkunn_core::knowledge::{
        Bearing, Claim, ClaimId, Expression, Link, Rule, Source, SourceId, SourceLink,
        block_digest, claim_path, source_path,
    };
    let Some(w) = World::new("knowledge", LeaseConfig::default()) else {
        return;
    };
    let claim_id = ClaimId::new();
    let source_id = SourceId::new();
    // The page gains a block a claim can stand on; the claim and its source are written in the same proposal.
    let home = HOME.replace("One.", "One. ^b1");
    let block = inklkunn_markup::blocks("One. ^b1\n");
    let mut claim = Claim::new(claim_id.clone());
    claim.expressions = vec![Expression::new("p-home", "b1").unwrap()];
    claim.sources = vec![SourceLink {
        source: source_id.clone(),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: block_digest(block.first().unwrap()),
    }];
    let source = Source::new(source_id.clone());
    let (claim_file, source_file) = (claim_path(&claim_id), source_path(&source_id));
    let p = w.propose(
        "A claim with its source",
        &[
            (claim_file.as_str(), Some(&claim.to_yaml())),
            (source_file.as_str(), Some(&source.to_yaml())),
            ("pages/Home.md", Some(&home)),
        ],
    );
    let done = signed(w.accept(&p).unwrap());
    assert_eq!(done.reconciliation, Reconciled::Clean);
    // The real repository has all three files, and the derived index reads them from the real extract.
    assert!(w.page(&claim_file).contains("kind: unknown"));
    assert!(w.page("pages/Home.md").contains("One. ^b1"));
    w.reads.forget_wiki(&w.wiki);
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    assert_eq!(index.claims().len(), 1);
    assert_eq!(index.sources().len(), 1);
    assert!(index.invalid().is_empty(), "{:?}", index.invalid());
    let link = index.links_of(claim_id.as_str()).next().unwrap();
    assert_eq!(link.judgement, "current");

    // A change that takes the claim's block out is refused, naming the claim; the signer is not asked and the
    // history does not grow.
    let blocks = w.history().len();
    let p = w.propose("Take the block out", &[("pages/Home.md", Some(HOME))]);
    match w.accept(&p) {
        Err(AcceptFailure::KnowledgeInvalid(refusals)) => {
            assert_eq!(refusals.len(), 1);
            let r = refusals.first().unwrap();
            assert_eq!(
                (r.file.as_str(), r.rule),
                (claim_file.as_str(), Rule::Anchor)
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(w.history().len(), blocks);
    assert_eq!(w.state(&p), ProposalState::HeldForReview);

    // An unreadable claim file is refused the same way.
    let p = w.propose(
        "A broken claim",
        &[(
            claim_path(&ClaimId::new()).as_str(),
            Some("kind: observation\nscore: 9\n"),
        )],
    );
    assert!(matches!(
        w.accept(&p),
        Err(AcceptFailure::KnowledgeInvalid(_))
    ));
    assert_eq!(w.history().len(), blocks);
}

// ------------------------------------------------------------------------------------ claims, sources and curation (K-T5)

#[test]
fn a_source_is_attached_and_a_status_curated_through_the_whole_path_on_a_real_history() {
    use inklkunn_core::knowledge::compose::{
        Attachment, Curation, LinkEdit, SourceDraft, SourceEdit, attach, curate,
    };
    use inklkunn_core::knowledge::{
        Availability, Bearing, Claim, File, Link, Rule, SourceKind, Standing, StatementKind,
        read_file,
    };
    let Some(w) = World::new("knowledge-attach", LeaseConfig::default()) else {
        return;
    };

    // 1. A source is attached to the first paragraph, in the same proposal as a page edit: the claim, the
    //    source and the page (marker and identifier already there) are one sealed change.
    let start = HOME.find("One.").unwrap();
    let edited = HOME.replace("Three.", "Three, as well.");
    let attached = attach(
        &edited,
        &(start..start + "One.".len()),
        None,
        &Attachment {
            source: SourceDraft {
                kind: SourceKind::Secondary,
                title: Some("A history".to_owned()),
                author: None,
                date: None,
                reference: None,
            },
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            kind: StatementKind::Observation,
        },
    )
    .unwrap();
    let [(claim_file, claim_text), (source_file, source_text)] = attached.files;
    let p = w.propose(
        "A source for the first paragraph",
        &[
            ("pages/Home.md", Some(&attached.page)),
            (claim_file.as_str(), Some(&claim_text)),
            (source_file.as_str(), Some(&source_text)),
        ],
    );
    let blocks = w.history().len();
    let done = signed(w.accept(&p).unwrap());
    assert_eq!(done.reconciliation, Reconciled::Clean);
    assert_eq!(w.history().len(), blocks + 1, "one proposal, one change");
    w.reads.forget_wiki(&w.wiki);
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    assert_eq!(index.claims().len(), 1);
    assert!(index.invalid().is_empty(), "{:?}", index.invalid());
    let id = attached.claim.to_string();
    assert!(
        index
            .links_of(&id)
            .all(|l| l.judgement == "current" && l.relation == "supports")
    );
    assert_eq!(index.claim(&id).unwrap().standing, "unknown");

    // 2. A curation proposal sets the status; the acceptance names itself as the decision.
    let File::Claim(claim) = read_file(&claim_file, w.page(&claim_file).as_bytes()).unwrap() else {
        panic!("a claim");
    };
    let File::Source(source) = read_file(&source_file, w.page(&source_file).as_bytes()).unwrap()
    else {
        panic!("a source");
    };
    let now = inklkunn_core::knowledge::anchor_state(&w.page("pages/Home.md"), &attached.block);
    let inklkunn_core::knowledge::AnchorState::Present(now) = now else {
        panic!("present");
    };
    let edit = Curation {
        status: Standing::Contested,
        links: vec![LinkEdit {
            source: source.id.clone(),
            relation: Link::Supports,
            bearing: Bearing::Direct,
        }],
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: "2026-11-04".to_owned(),
            availability: Availability::Available,
        }],
        confirm: false,
    };
    let files = curate(&claim, std::slice::from_ref(&source), Some(&now), &edit).unwrap();
    let refs: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    let curation = w.propose("Contested; checked today", &refs);
    signed(w.accept(&curation).unwrap());
    let File::Claim(decided) = read_file(&claim_file, w.page(&claim_file).as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    assert_eq!(decided.community_status.status, Standing::Contested);
    assert_eq!(
        decided.community_status.proposal.as_deref(),
        Some(curation.id.as_str())
    );
    assert_eq!(decided.community_status.judged_on, Some(now));
    let File::Source(checked) = read_file(&source_file, w.page(&source_file).as_bytes()).unwrap()
    else {
        panic!("a source");
    };
    assert_eq!(checked.availability, Availability::Available);
    assert_eq!(
        checked.last_reviewed.and_then(|r| r.proposal).as_deref(),
        Some(curation.id.as_str())
    );
    w.reads.forget_wiki(&w.wiki);
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    let row = index.claim(&id).unwrap();
    assert_eq!(
        (row.standing.as_str(), row.standing_judgement.as_str()),
        ("contested", "current")
    );

    // 3. No other proposal sets a status: a page edit that does is refused before the signer is asked.
    let mut forced: Claim = (*decided).clone();
    forced.community_status.status = Standing::Accepted;
    let blocks = w.history().len();
    let p = w.propose(
        "A status beside a page edit",
        &[
            (claim_file.as_str(), Some(&forced.to_yaml())),
            (
                "pages/Other.md",
                Some(&OTHER.replace("Hello.", "Hello again.")),
            ),
        ],
    );
    match w.accept(&p) {
        Err(AcceptFailure::KnowledgeInvalid(r)) => {
            assert_eq!(
                r.iter()
                    .map(|r| (r.file.as_str(), r.rule))
                    .collect::<Vec<_>>(),
                [(claim_file.as_str(), Rule::Status)]
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(w.history().len(), blocks);
    assert_eq!(w.state(&p), ProposalState::HeldForReview);

    // 4. The paragraph is reworded (its marker stays): the status and the source are now shown as judged on an
    //    earlier wording, until a curation proposal judges them again.
    let reworded = w
        .page("pages/Home.md")
        .replace("One.", "One, in other words.");
    let p = w.propose("Reword", &[("pages/Home.md", Some(&reworded))]);
    signed(w.accept(&p).unwrap());
    w.reads.forget_wiki(&w.wiki);
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    assert_eq!(index.claim(&id).unwrap().standing_judgement, "earlier");
    assert!(index.links_of(&id).all(|l| l.judgement == "earlier"));
}

// ------------------------------------------------------------------------------------ the review record (K3 b, K11)

/// A claim standing on the paragraph `words` of `pages/Home.md`, made and accepted the way the form makes it.
struct Claimed {
    claim: inklkunn_core::knowledge::ClaimId,
    claim_file: String,
    block: String,
}

fn claim_on(w: &World, words: &str) -> Claimed {
    use inklkunn_core::knowledge::compose::{Attachment, SourceDraft, attach};
    use inklkunn_core::knowledge::{Bearing, Link, SourceKind, StatementKind};
    let page = w.page("pages/Home.md");
    let start = page.find(words).unwrap();
    let attached = attach(
        &page,
        &(start..start + words.len()),
        None,
        &Attachment {
            source: SourceDraft {
                kind: SourceKind::Secondary,
                title: Some("A history".to_owned()),
                author: None,
                date: None,
                reference: None,
            },
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            kind: StatementKind::Observation,
        },
    )
    .unwrap();
    let [(claim_file, claim_text), (source_file, source_text)] = attached.files;
    let p = w.propose(
        "A source for a paragraph",
        &[
            ("pages/Home.md", Some(&attached.page)),
            (claim_file.as_str(), Some(&claim_text)),
            (source_file.as_str(), Some(&source_text)),
        ],
    );
    signed(w.accept(&p).unwrap());
    Claimed {
        claim: attached.claim,
        claim_file,
        block: attached.block,
    }
}

fn reviews(w: &World) -> Vec<(String, inklkunn_store::ClaimHow, inklkunn_store::ProposalId)> {
    use inklkunn_store::ProposalStore;
    let mut found: Vec<_> = w
        .store
        .claim_reviews(&w.wiki.id)
        .unwrap()
        .into_iter()
        .map(|r| (r.claim, r.how, r.proposal))
        .collect();
    found.sort();
    found
}

#[test]
fn the_review_record_is_kept_with_each_acceptance_and_read_only_from_one_that_sealed() {
    use inklkunn_core::knowledge::{Claim, File, read_file};
    use inklkunn_store::ClaimHow::{Dropped, Kept};
    use inklkunn_store::ProposalStore;
    let Some(w) = World::new("review-record", LeaseConfig::default()) else {
        return;
    };
    let one = claim_on(&w, "One.");
    let two = claim_on(&w, "Two.");
    assert!(
        reviews(&w).is_empty(),
        "making a claim touches none: it is judged on the wording it is made with"
    );

    // A change that rewords the paragraph, its marker staying: the claim is kept. A failed acceptance writes records
    // that are never read; the retry writes its own.
    let reworded = w
        .page("pages/Home.md")
        .replace("One.", "One, in other words.");
    let p = w.propose("Reword", &[("pages/Home.md", Some(&reworded))]);
    w.control("fail-commit", "");
    assert!(matches!(w.accept(&p), Err(AcceptFailure::Failed { .. })));
    assert!(reviews(&w).is_empty(), "nothing sealed, nothing read");
    w.uncontrol("fail-commit");
    let done = signed(w.accept(&p).unwrap());
    assert_eq!(
        reviews(&w),
        vec![(one.claim.to_string(), Kept, p.id.clone())]
    );
    let of = w.store.claim_reviews_of(&done.acceptance).unwrap();
    assert_eq!(of.len(), 1);
    assert_eq!(of.first().map(|r| r.reconfirmed_by.is_none()), Some(true));

    // A change that rewords one paragraph and removes another, with the claim's expression: one kept, one dropped.
    let page = w
        .page("pages/Home.md")
        .replace("One, in other words.", "One, once more.")
        .replace(&format!("Two. ^{}\n\n", two.block), "");
    assert!(!page.contains("Two."));
    let File::Claim(held) = read_file(&two.claim_file, w.page(&two.claim_file).as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    let mut gone: Claim = (*held).clone();
    gone.expressions.clear();
    let q = w.propose(
        "Reword one, remove the other",
        &[
            ("pages/Home.md", Some(&page)),
            (two.claim_file.as_str(), Some(&gone.to_yaml())),
        ],
    );
    signed(w.accept(&q).unwrap());
    let mut want = vec![
        (one.claim.to_string(), Kept, p.id.clone()),
        (one.claim.to_string(), Kept, q.id.clone()),
        (two.claim.to_string(), Dropped, q.id.clone()),
    ];
    want.sort();
    assert_eq!(reviews(&w), want);

    // A change to a page that holds no claim leaves nothing.
    let r = w.propose(
        "Elsewhere",
        &[(
            "pages/Other.md",
            Some(&OTHER.replace("Hello.", "Hello again.")),
        )],
    );
    signed(w.accept(&r).unwrap());
    assert_eq!(reviews(&w), want);
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_reconfirm_proposal_is_recorded_as_re_confirming_its_change_when_its_acceptance_seals() {
    use inklkunn_core::knowledge::compose::{Curation, LinkEdit, curate};
    use inklkunn_core::knowledge::{AnchorState, File, Standing, anchor_state, read_file};
    use inklkunn_store::ProposalStore;
    let Some(w) = World::new("reconfirm-record", LeaseConfig::default()) else {
        return;
    };
    let one = claim_on(&w, "One.");
    let reworded = w
        .page("pages/Home.md")
        .replace("One.", "One, in other words.");
    let p = w.propose("Reword", &[("pages/Home.md", Some(&reworded))]);
    let change = signed(w.accept(&p).unwrap());

    // The files that judge the claim again on the wording it has now.
    let judged_again = |w: &World| -> Vec<FileChange> {
        let File::Claim(claim) =
            read_file(&one.claim_file, w.page(&one.claim_file).as_bytes()).unwrap()
        else {
            panic!("a claim");
        };
        let AnchorState::Present(now) = anchor_state(&w.page("pages/Home.md"), &one.block) else {
            panic!("present");
        };
        let edit = Curation {
            status: Standing::Unknown,
            links: claim
                .sources
                .iter()
                .map(|l| LinkEdit {
                    source: l.source.clone(),
                    relation: l.relation,
                    bearing: l.bearing,
                })
                .collect(),
            sources: Vec::new(),
            confirm: true,
        };
        curate(&claim, &[], Some(&now), &edit)
            .unwrap()
            .into_iter()
            .map(|(path, text)| FileChange {
                path,
                text: Some(text),
            })
            .collect()
    };
    let propose_curation = |w: &World, reconfirming: Option<&inklkunn_store::AcceptanceId>| {
        let edit = ProposalEdit {
            summary: "Judged again".to_owned(),
            files: judged_again(w),
            ..ProposalEdit::default()
        };
        let draft = match reconfirming {
            Some(change) => {
                w.proposals
                    .create_reconfirm(&w.contributor, &w.wiki, None, edit, change)
            }
            None => w.proposals.create(&w.contributor, &w.wiki, None, edit),
        }
        .unwrap_or_else(|e| panic!("create: {e}"));
        w.proposals
            .submit(&w.contributor, &w.wiki, &draft.id)
            .unwrap()
    };
    let reconfirmed = |w: &World, change: &inklkunn_store::AcceptanceId| {
        w.store
            .claim_reviews_of(change)
            .unwrap()
            .into_iter()
            .map(|r| r.reconfirmed_by)
            .collect::<Vec<_>>()
    };
    assert_eq!(reconfirmed(&w, &change.acceptance), vec![None]);

    // The curation proposal names the change it re-confirms from its creation; until it is accepted nothing is.
    let curation = propose_curation(&w, Some(&change.acceptance));
    assert_eq!(
        w.proposals
            .reconfirm_of(&w.maintainer, &w.wiki, &curation.id)
            .unwrap(),
        Some(change.acceptance.clone())
    );
    assert_eq!(
        w.proposals
            .reconfirmations_of(&w.maintainer, &w.wiki, &change.acceptance)
            .unwrap(),
        vec![curation.id.clone()]
    );
    assert_eq!(reconfirmed(&w, &change.acceptance), vec![None]);
    // An acceptance that fails records nothing that is read ...
    w.control("fail-commit", "");
    assert!(matches!(
        w.accept(&curation),
        Err(AcceptFailure::Failed { .. })
    ));
    w.uncontrol("fail-commit");
    assert_eq!(reconfirmed(&w, &change.acceptance), vec![None]);
    // ... and the one that seals re-confirms the claim for that change, by its own acceptance.
    let sealed_curation = signed(w.accept(&curation).unwrap());
    assert_eq!(
        reconfirmed(&w, &change.acceptance),
        vec![Some(sealed_curation.acceptance.clone())]
    );

    // A curation that was not made by the reconfirm step re-confirms nothing, whatever it judges.
    let reworded_again = w
        .page("pages/Home.md")
        .replace("One, in other words.", "One, again.");
    let again = w.propose("Reword again", &[("pages/Home.md", Some(&reworded_again))]);
    let second = signed(w.accept(&again).unwrap());
    let plain = propose_curation(&w, None);
    signed(w.accept(&plain).unwrap());
    assert_eq!(reconfirmed(&w, &second.acceptance), vec![None]);
    assert!(w.reconciled().is_clean());
}

/// The proposal records, all of them the real store's, except that the review record cannot be kept.
struct CannotKeepReviews(Arc<SqliteStore>);

type Stored<T> = inklkunn_store::Result<T>;

impl inklkunn_store::ProposalStore for CannotKeepReviews {
    fn create_proposal(
        &self,
        p: &ProposalRecord,
        d: &inklkunn_store::RevisionContent,
        t: &inklkunn_store::ProposalTransition,
    ) -> Stored<()> {
        self.0.create_proposal(p, d, t)
    }
    fn create_rollback_proposal(
        &self,
        p: &ProposalRecord,
        d: &inklkunn_store::RevisionContent,
        t: &inklkunn_store::ProposalTransition,
        undoes: &str,
    ) -> Stored<()> {
        self.0.create_rollback_proposal(p, d, t, undoes)
    }
    fn rollback_of(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Option<String>> {
        self.0.rollback_of(id, v)
    }
    fn revision_flags(
        &self,
        ids: &[inklkunn_store::ProposalId],
    ) -> Stored<Vec<(inklkunn_store::ProposalId, inklkunn_store::RevisionFlags)>> {
        self.0.revision_flags(ids)
    }
    fn proposal(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Option<ProposalRecord>> {
        self.0.proposal(id, v)
    }
    fn proposals(
        &self,
        w: &inklkunn_store::WikiId,
        f: &ProposalFilter,
        v: Option<&AccountId>,
    ) -> Stored<Vec<ProposalRecord>> {
        self.0.proposals(w, f, v)
    }
    fn draft(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Option<inklkunn_store::RevisionContent>> {
        self.0.draft(id, v)
    }
    fn save_draft(
        &self,
        id: &inklkunn_store::ProposalId,
        summary: &str,
        sources: &[String],
        draft: &inklkunn_store::RevisionContent,
        at: i64,
    ) -> Stored<()> {
        self.0.save_draft(id, summary, sources, draft, at)
    }
    fn change_proposal(&self, c: &inklkunn_store::ProposalChange) -> Stored<ProposalRecord> {
        self.0.change_proposal(c)
    }
    fn revisions(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Vec<inklkunn_store::RevisionRecord>> {
        self.0.revisions(id, v)
    }
    fn transitions(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Vec<inklkunn_store::ProposalTransition>> {
        self.0.transitions(id, v)
    }
    fn add_comment(&self, c: &inklkunn_store::CommentRecord, s: &[ProposalState]) -> Stored<()> {
        self.0.add_comment(c, s)
    }
    fn comments(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Vec<inklkunn_store::CommentRecord>> {
        self.0.comments(id, v)
    }
    fn add_decision(&self, d: &inklkunn_store::DecisionRecord) -> Stored<()> {
        self.0.add_decision(d)
    }
    fn decisions(
        &self,
        id: &inklkunn_store::ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Vec<inklkunn_store::DecisionRecord>> {
        self.0.decisions(id, v)
    }
    fn wiki_decisions(
        &self,
        w: &inklkunn_store::WikiId,
    ) -> Stored<Vec<inklkunn_store::DecisionRecord>> {
        self.0.wiki_decisions(w)
    }
    fn open_count(&self, w: &inklkunn_store::WikiId, a: &AccountId) -> Stored<u64> {
        self.0.open_count(w, a)
    }
    fn opened_since(
        &self,
        w: &inklkunn_store::WikiId,
        a: Option<&AccountId>,
        since: i64,
    ) -> Stored<Vec<i64>> {
        self.0.opened_since(w, a, since)
    }
    fn submitted_since(
        &self,
        w: &inklkunn_store::WikiId,
        since: i64,
    ) -> Stored<Vec<(AccountId, i64)>> {
        self.0.submitted_since(w, since)
    }
    fn comments_since(
        &self,
        w: &inklkunn_store::WikiId,
        a: &AccountId,
        since: i64,
    ) -> Stored<Vec<i64>> {
        self.0.comments_since(w, a, since)
    }
    // The review record: all the real store's, but the claims of a change.
    fn record_claim_reviews(
        &self,
        _: &inklkunn_store::AcceptanceId,
        _: &[inklkunn_store::ClaimOutcome],
    ) -> Stored<()> {
        Err(inklkunn_store::StoreError::Backend(
            "the review record cannot be kept".to_owned(),
        ))
    }
    fn record_reconfirmations(
        &self,
        by: &inklkunn_store::AcceptanceId,
        change: &inklkunn_store::AcceptanceId,
        claims: &[String],
    ) -> Stored<()> {
        self.0.record_reconfirmations(by, change, claims)
    }
    fn claim_reviews_of(
        &self,
        c: &inklkunn_store::AcceptanceId,
    ) -> Stored<Vec<inklkunn_store::ClaimReview>> {
        self.0.claim_reviews_of(c)
    }
    fn claim_reviews(
        &self,
        w: &inklkunn_store::WikiId,
    ) -> Stored<Vec<inklkunn_store::ClaimReview>> {
        self.0.claim_reviews(w)
    }
}

#[test]
fn a_change_whose_review_record_cannot_be_kept_is_not_signed_and_a_retry_signs_it_once() {
    use inklkunn_store::ClaimHow::Kept;
    let Some(w) = World::new("review-cannot", LeaseConfig::default()) else {
        return;
    };
    let one = claim_on(&w, "One.");
    // An acceptor whose proposal service cannot keep the record of a change's claims.
    let authorizer = || {
        Authorizer::new(
            Arc::new(StoreAuthz::new(w.store.clone(), w.store.clone())),
            w.store.clone(),
        )
    };
    let broken = Arc::new(ProposalService::new(
        Arc::new(CannotKeepReviews(w.store.clone())),
        w.store.clone(),
        w.store.clone(),
        authorizer(),
        w.reads.clone(),
        QuotaConfig::default(),
    ));
    let leases = WikiLeases::new(
        w.write_leases.clone(),
        w.store.clone(),
        "acceptance-test-broken",
        LeaseConfig::default(),
    );
    let broken_acceptor = ProposalAcceptor::new(AcceptanceParts {
        proposals: broken,
        acceptances: w.store.clone(),
        acts: w.store.clone(),
        roles: w.store.clone(),
        accounts: w.store.clone(),
        storage: w.storage.clone(),
        layout: w.layout.clone(),
        signer: w.signer.clone(),
        leases,
        reads: w.reads.clone(),
    });

    let reworded = w
        .page("pages/Home.md")
        .replace("One.", "One, in other words.");
    let p = w.propose("Reword", &[("pages/Home.md", Some(&reworded))]);
    let (history, page) = (w.history(), w.page("pages/Home.md"));
    let intents = w.store.intents(&w.wiki.id).unwrap().len();
    let refused = broken_acceptor
        .accept(&w.maintainer, &w.wiki, &p.id, 1)
        .unwrap_err();
    let AcceptFailure::Failed {
        acceptance, word, ..
    } = &refused
    else {
        panic!("{refused:?}")
    };
    assert_eq!(*word, "unavailable");
    // Nothing was signed: the history and the page are as they were, the record says failed, the intent was never written
    // and the proposal is still open.
    assert_eq!(w.history(), history);
    assert_eq!(w.page("pages/Home.md"), page);
    let entries = w.store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries.first().map(|e| e.acceptance.id.clone()),
        Some(acceptance.clone())
    );
    assert!(matches!(
        entries
            .first()
            .and_then(|e| e.result.clone())
            .map(|r| r.outcome),
        Some(inklkunn_store::AcceptanceOutcome::Failed { .. })
    ));
    assert_eq!(w.store.intents(&w.wiki.id).unwrap().len(), intents);
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert!(reviews(&w).is_empty());
    assert!(!w.calls().is_empty());
    let commits = w.calls().iter().filter(|v| *v == "commit").count();

    // The real acceptor signs it once, with its record.
    signed(w.accept(&p).unwrap());
    assert_eq!(
        w.calls().iter().filter(|v| *v == "commit").count(),
        commits + 1
    );
    assert_eq!(
        reviews(&w),
        vec![(one.claim.to_string(), Kept, p.id.clone())]
    );
    assert!(w.reconciled().is_clean());
}
