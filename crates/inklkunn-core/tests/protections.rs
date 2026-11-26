//! RFC 006 acceptance criterion 7 on a real prikk (handoff G-T3): **protections**, standing and temporary, enforced by the
//! service's own orchestration over wikis created and written by the real signer, read through the real driver, with their
//! rules read from the repository's default view by the real loader and the records in a real (SQLite) store. Self-skips with a
//! visible line when prikk is not on `PATH`.
//!
//! - A **standing** protection is the policy's `protect` list: proposed and accepted as a real governance change, then enforced.
//! - A **temporary** protection is a record of the `moderate` rung: enforced, and ended by the clock with nothing written.
//! - Both at once: the stricter decides and both are listed.
//! - A protection that ends between the early judgement and the lease no longer applies, and one set in between does.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::Setting;
use inklkunn_core::governance::{PlatformLimits, Require, Target};
use inklkunn_core::message::SystemLabel;
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::protection::{Kind, Lack, Notice};
use inklkunn_core::{
    AcceptFailure, AcceptStep, AcceptanceParts, Accepted, Authorizer, IdentityAuthority,
    KeyAuthority, LeaseConfig, PreparedFile, Principal, ProposalAcceptor, ReadConfig, ReadService,
    Signer, SignerConfig, StoreAuthz, StoreLedger, SystemActs, SystemRequest, WikiLeases,
    WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceRecord, AcceptanceStore, Account, AccountId, AccountStore, DecisionKind, DisplayName,
    FileChange, GovernanceStore, NewWiki, ProposalId, ProposalRecord, ProposalState, ProposalStore,
    Role, RoleGrant, RoleStore, Slug, SqliteStore, SystemActKind,
};

/// Every question this crate asks of the prikk on `PATH`: the only place a test reaches a capability (maintenance M39).
#[path = "support/prikk_probes.rs"]
mod prikk_probes;

/// A clock of a test's own (tests run side by side in one process): the real time, moved by the test. It tells the authorizer and
/// the acceptor what time it is, and nothing else.
macro_rules! test_clock {
    ($offset:ident, $clock:ident) => {
        static $offset: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

        fn $clock() -> i64 {
            inklkunn_store::record::now_secs() + $offset.load(std::sync::atomic::Ordering::SeqCst)
        }
    };
}

const HOME: &str = "---\ntitle: Home\nid: p-home\n---\n\nOne.\n\nTwo.\n\nThree.\n";

/// The process's one identity authority (it can be claimed once, and a test binary is one process).
fn authority() -> &'static IdentityAuthority {
    static AUTHORITY: std::sync::OnceLock<IdentityAuthority> = std::sync::OnceLock::new();
    AUTHORITY.get_or_init(|| IdentityAuthority::claim().expect("claimed once"))
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-gov-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

/// What a test does when the acceptance reaches a step (held by the world's acceptor, which tells it every step).
type Hook = Arc<std::sync::Mutex<Option<Arc<dyn Fn(AcceptStep) + Send + Sync>>>>;

struct Person {
    #[allow(dead_code)]
    id: AccountId,
    principal: Principal,
}

struct World {
    _tmp: Dir,
    _shm: Dir,
    store: Arc<SqliteStore>,
    storage: Arc<PrikkCli>,
    reads: Arc<ReadService>,
    proposals: Arc<ProposalService>,
    acceptor: Arc<ProposalAcceptor>,
    desk: Authorizer,
    hook: Hook,
    acts: SystemActs,
    wiki: inklkunn_store::WikiRecord,
    repo: PathBuf,
    admin: Person,
    maintainer: Person,
    reviewer: Person,
    author: Person,
}

impl World {
    /// A wiki with the creation and one import of `pages/Home.md`, under the operator's `limits`.
    fn new(tag: &str, limits: PlatformLimits, clock: fn() -> i64) -> Option<World> {
        let real = prikk_probes::prikk_first()?;
        if !Path::new("/dev/shm").is_dir() {
            println!("skipping: no /dev/shm here");
            return None;
        }
        let tmp = dir_in(&std::env::temp_dir(), tag);
        let shm = dir_in(Path::new("/dev/shm"), tag);
        let secret = tmp.0.join("secret");
        std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
        let layout = inklkunn_store::RootLayout::new(tmp.0.join("root"));
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let write_leases = WriteLeases::new(layout.clone());
        let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
        let mut cfg = SignerConfig::new(
            &real,
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
        let leases = WikiLeases::new(
            write_leases,
            store.clone(),
            "governance-test",
            LeaseConfig::default(),
        );
        // The same wiring as `serve`: the rules of each wiki read by the read service, through the derived store, under the
        // operator's limits; the authorizer reads them through it and nowhere else.
        let reads = Arc::new(
            ReadService::new(
                storage.clone(),
                store.clone(),
                store.clone(),
                layout.clone(),
                ReadConfig::default(),
                None,
            )
            .with_tombstones(Arc::new(inklkunn_core::tombstone::Tombstones::new(
                store.clone(),
                store.clone(),
            )))
            .with_governance_snapshots(store.clone())
            .with_platform_limits(limits),
        );
        let authorizer = || {
            Authorizer::new(
                Arc::new(StoreAuthz::new(store.clone(), store.clone()).with_rules(reads.clone())),
                store.clone(),
            )
            .with_clock(clock)
            .with_protections(store.clone(), limits)
        };
        let proposals = Arc::new(
            ProposalService::new(
                store.clone(),
                store.clone(),
                store.clone(),
                authorizer(),
                reads.clone(),
                QuotaConfig::default(),
            )
            .with_roles(store.clone()),
        );
        let hook: Hook = Arc::new(std::sync::Mutex::new(None));
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
            .with_clock(clock)
            .with_observer({
                let hook = hook.clone();
                Arc::new(move |step| {
                    let held = hook.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if let Some(act) = held {
                        act(step);
                    }
                })
            }),
        );
        let acts = SystemActs::new(
            Authorizer::new(
                Arc::new(StoreAuthz::new(store.clone(), store.clone())),
                store.clone(),
            ),
            store.clone(),
            store.clone(),
            storage.clone(),
            layout.clone(),
            signer.clone(),
            leases.clone(),
        )
        .with_reads(reads.clone())
        .with_audit(store.clone());
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
            Person {
                principal: Principal::session(authority(), id.clone()),
                id,
            }
        };
        let desk = authorizer();
        let world = World {
            admin: person("Ada Admin", Role::Admin),
            maintainer: person("Maya Maintainer", Role::Maintainer),
            reviewer: person("Rin Reviewer", Role::Reviewer),
            author: person("Noor Contributor", Role::Contributor),
            _tmp: tmp,
            _shm: shm,
            store,
            storage,
            reads,
            proposals,
            acceptor,
            desk,
            hook,
            acts,
            repo: created.repo,
            wiki,
        };
        world.write_outside(&[("pages/Home.md", HOME)]);
        Some(world)
    }

    /// Write files into the default view **outside inklkunn's proposals**: a system act, as an import or a hand edit
    /// would. Nothing judges them.
    fn write_outside(&self, files: &[(&str, &str)]) -> String {
        let request = SystemRequest {
            kind: SystemActKind::System,
            summary: "Written outside the proposals".to_owned(),
            files: files
                .iter()
                .map(|(path, text)| PreparedFile {
                    path: PathBuf::from(path),
                    content: Some(text.as_bytes().to_vec()),
                })
                .collect(),
            renames: Vec::new(),
            imported_from: None,
            source: None,
            planned_against: None,
        };
        let signed = self
            .acts
            .run(&self.wiki, &SystemLabel::inklkunn(), request)
            .unwrap_or_else(|e| panic!("{e}"));
        self.reads.forget_wiki(&self.wiki);
        signed.block_id
    }

    fn tip(&self) -> String {
        self.tip_in("heads/main")
    }

    fn tip_in(&self, view: &str) -> String {
        self.storage
            .history(&self.repo, &Point::View(view.to_owned()), None)
            .unwrap()
            .blocks
            .first()
            .unwrap()
            .block_id
            .clone()
    }

    fn history_len(&self) -> usize {
        self.storage
            .history(&self.repo, &Point::View("heads/main".to_owned()), None)
            .unwrap()
            .blocks
            .len()
    }

    fn file(&self, path: &str) -> Option<String> {
        self.storage
            .read(&self.repo, &Point::Block(self.tip()), path, 1 << 20)
            .ok()
            .map(|b| String::from_utf8(b).unwrap())
    }

    fn propose_as(
        &self,
        who: &Person,
        summary: &str,
        files: &[(&str, Option<&str>)],
    ) -> ProposalRecord {
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
            .create(&who.principal, &self.wiki, None, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        self.proposals
            .submit(&who.principal, &self.wiki, &draft.id)
            .unwrap_or_else(|e| panic!("submit: {e}"))
    }

    fn propose(&self, summary: &str, files: &[(&str, Option<&str>)]) -> ProposalRecord {
        self.propose_as(&self.author, summary, files)
    }

    fn accept(&self, who: &Person, id: &ProposalId) -> Result<Accepted, AcceptFailure> {
        self.acceptor.accept(&who.principal, &self.wiki, id, 1)
    }

    fn approve(&self, who: &Person, id: &ProposalId) {
        self.proposals
            .decide(&who.principal, &self.wiki, id, 1, DecisionKind::Approve, "")
            .unwrap_or_else(|e| panic!("approve: {e}"));
    }

    fn record(&self, id: &ProposalId) -> AcceptanceRecord {
        self.store
            .acceptances_of(id, 1)
            .unwrap()
            .pop()
            .unwrap()
            .acceptance
    }

    fn state(&self, id: &ProposalId) -> ProposalState {
        self.store.proposal(id, None).unwrap().unwrap().state
    }
}

fn home_changed() -> String {
    HOME.replace("One.", "Uno.")
}

fn signed(done: Result<Accepted, AcceptFailure>) -> String {
    match done {
        Ok(Accepted::Signed(s)) => s.block_id,
        other => panic!("not signed: {other:?}"),
    }
}

fn setting(title: &str, require: Require, reason: &str, days: u32) -> Setting {
    Setting {
        target: Target::path(title).expect("a title"),
        require,
        reason: reason.to_owned(),
        days,
    }
}

/// The refusal of an acceptance that was held back by a protection: every protection that applies, and what was lacking.
fn protected(failure: AcceptFailure) -> (Vec<Notice>, Lack) {
    match failure {
        AcceptFailure::Protected { notices, lack } => (notices, lack),
        other => panic!("not protected: {other:?}"),
    }
}

/// An acceptance `who` makes that is refused, with the repository and the proposal as they were.
fn refused_untouched(w: &World, p: &ProposalRecord, who: &Person) -> AcceptFailure {
    let before = (w.history_len(), w.state(&p.id));
    let failure = match w.accept(who, &p.id) {
        Err(failure) => failure,
        Ok(done) => panic!("accepted: {done:?}"),
    };
    assert_eq!((w.history_len(), w.state(&p.id)), before);
    failure
}

// ------------------------------------------------------------------------------------------------- criterion 7

#[test]
fn criterion_7_a_standing_protection_is_proposed_and_accepted_as_a_real_change_to_the_rules_and_then_enforced()
 {
    test_clock!(OFFSET, clock);
    let Some(w) = World::new("prot-standing", PlatformLimits::default(), clock) else {
        return;
    };
    let help = "---\ntitle: Start\nid: p-start\n---\n\nBegin here.\n";
    w.write_outside(&[("pages/Help/Start.md", help)]);
    // The rules say what is protected: a page by its title and a namespace, each with its reason.
    let policy = "protect:\n  - path: Home\n    require:\n      rung: admin\n    reason: \"Only the admin edits the front page.\"\n  - namespace: Help\n    require: review\n    reason: \"The help is read before it changes.\"\n";
    let rules = w.propose(
        "Protect the front page and the help",
        &[("governance/policy.yaml", Some(policy))],
    );
    signed(w.accept(&w.maintainer, &rules.id));
    assert_eq!(w.file("governance/policy.yaml").as_deref(), Some(policy));
    let notices = w
        .desk
        .protections_on(&w.author.principal, &w.wiki.id, "Home");
    let [notice] = notices.as_slice() else {
        panic!("{notices:?}");
    };
    assert_eq!(notice.kind(), Kind::Standing);
    assert_eq!(notice.reason(), "Only the admin edits the front page.");

    // The front page: a maintainer is refused, naming the protection, and nothing is written.
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    let (named, lack) = protected(refused_untouched(&w, &page, &w.maintainer));
    assert_eq!(lack, Lack::Rung(Role::Admin));
    assert_eq!(named.len(), 1);
    // An admin accepts it: the rung asked is the admin's, and the record says who accepted.
    signed(w.accept(&w.admin, &page.id));
    assert_eq!(w.state(&page.id), ProposalState::Accepted);
    assert_eq!(w.record(&page.id).acceptor_role, Role::Admin);

    // The help: a review first. The approval of someone other than the proposer is what is asked.
    let text = help.replace("Begin here.", "Start here.");
    let help_change = w.propose("Reword the help", &[("pages/Help/Start.md", Some(&text))]);
    let (_, lack) = protected(refused_untouched(&w, &help_change, &w.maintainer));
    assert_eq!(lack, Lack::Review);
    w.approve(&w.reviewer, &help_change.id);
    signed(w.accept(&w.maintainer, &help_change.id));
    assert_eq!(w.state(&help_change.id), ProposalState::Accepted);

    // A protection is a page's, so it never holds a change to the rules: the maintainer takes the front page's protection away,
    // and from the next decision the maintainer accepts a change to the front page.
    let lifted = w.propose(
        "Protect the help only",
        &[(
            "governance/policy.yaml",
            Some(
                "protect:\n  - namespace: Help\n    require: review\n    reason: \"The help is read before it changes.\"\n",
            ),
        )],
    );
    signed(w.accept(&w.maintainer, &lifted.id));
    assert!(
        w.desk
            .protections_on(&w.author.principal, &w.wiki.id, "Home")
            .is_empty()
    );
    let again = w.propose(
        "Say it in Italian",
        &[(
            "pages/Home.md",
            Some(&home_changed().replace("Uno.", "Una.")),
        )],
    );
    signed(w.accept(&w.maintainer, &again.id));
}

#[test]
fn criterion_7_a_temporary_protection_is_enforced_and_ends_by_itself_by_the_clock_on_a_real_wiki() {
    test_clock!(OFFSET, clock);
    let Some(w) = World::new("prot-temporary", PlatformLimits::default(), clock) else {
        return;
    };
    w.desk
        .protect(
            &w.maintainer.principal,
            &w.wiki.id,
            &setting(
                "Home",
                Require::Rung(Role::Admin),
                "A dispute this week.",
                3,
            ),
        )
        .unwrap();
    let notices = w
        .desk
        .protections_on(&w.author.principal, &w.wiki.id, "Home");
    let until = notices.first().and_then(Notice::until).expect("an end");
    let now = clock();
    assert!(
        until > now + 2 * 86_400 && until <= now + 3 * 86_400 + 5,
        "{until} {now}"
    );
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // While it holds, the maintainer is refused and the repository is as it was.
    let (named, lack) = protected(refused_untouched(&w, &page, &w.maintainer));
    assert_eq!(lack, Lack::Rung(Role::Admin));
    assert_eq!(
        named.first().map(Notice::reason),
        Some("A dispute this week.")
    );
    // Nothing is written for it to end: the clock moves past its end, and the same acceptance is signed.
    let records = w.store.protection_events(&w.wiki.id, 50).unwrap().len();
    OFFSET.store(until - now + 60, std::sync::atomic::Ordering::SeqCst);
    assert!(
        w.desk
            .protections_on(&w.author.principal, &w.wiki.id, "Home")
            .is_empty()
    );
    signed(w.accept(&w.maintainer, &page.id));
    assert_eq!(
        w.store.protection_events(&w.wiki.id, 50).unwrap().len(),
        records
    );
    assert_eq!(w.state(&page.id), ProposalState::Accepted);
}

#[test]
fn criterion_7_a_protection_set_after_the_early_judgement_and_before_the_signer_is_seen_under_the_lease()
 {
    test_clock!(_OFFSET, clock);
    let Some(w) = World::new("prot-lease", PlatformLimits::default(), clock) else {
        return;
    };
    // Set after the early judgement, before the signer. The lease's judgement sees it, and nothing is written.
    let second = w.propose(
        "Say it in Italian",
        &[(
            "pages/Home.md",
            Some(&home_changed().replace("Uno.", "Una.")),
        )],
    );
    let desk = w.desk.clone();
    let (wiki, maya) = (w.wiki.id.clone(), w.maintainer.principal.clone());
    *w.hook.lock().unwrap() = Some(Arc::new(move |step| {
        if step == AcceptStep::LeaseTaken {
            desk.protect(
                &maya,
                &wiki,
                &setting(
                    "Home",
                    Require::Rung(Role::Admin),
                    "Set at the last moment.",
                    1,
                ),
            )
            .unwrap();
        }
    }));
    let (_, lack) = protected(refused_untouched(&w, &second, &w.maintainer));
    assert_eq!(lack, Lack::Rung(Role::Admin));
}

#[test]
fn criterion_7_both_kinds_at_once_the_stricter_decides_and_both_are_listed() {
    test_clock!(OFFSET, clock);
    let Some(w) = World::new("prot-both", PlatformLimits::default(), clock) else {
        return;
    };
    let policy = "protect:\n  - path: Home\n    require:\n      rung: maintainer\n    reason: \"Standing.\"\n";
    let rules = w.propose(
        "Protect the front page",
        &[("governance/policy.yaml", Some(policy))],
    );
    signed(w.accept(&w.maintainer, &rules.id));
    w.desk
        .protect(
            &w.admin.principal,
            &w.wiki.id,
            &setting("Home", Require::Rung(Role::Admin), "Temporary.", 5),
        )
        .unwrap();
    let notices = w
        .desk
        .protections_on(&w.author.principal, &w.wiki.id, "Home");
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert_eq!(notices.first().map(Notice::kind), Some(Kind::Standing));
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    let (named, lack) = protected(refused_untouched(&w, &page, &w.maintainer));
    assert_eq!(lack, Lack::Rung(Role::Admin), "the higher rung decides");
    assert_eq!(named.len(), 2, "both are listed");
    signed(w.accept(&w.admin, &page.id));
}
