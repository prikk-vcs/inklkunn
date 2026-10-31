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
        assert!(created.first_change_recorded);

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
            signer,
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
        (held.purpose.as_str(), held.holder.as_str()),
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
