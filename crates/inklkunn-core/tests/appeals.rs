//! RFC 006 acceptance criterion 9 on a real prikk (handoff G-T5): **an overturned appeal puts the action right on a real wiki.**
//! A proposal that was declined is reopened by the overturn of an appeal and then **accepted and signed** by the real signer, and
//! the page changes; a temporary protection that held an acceptance back is reversed by the overturn of an appeal and the same
//! acceptance is signed. The wikis are created and written by the real signer and read through the real driver, with their rules
//! read from the repository's default view by the real loader and the records in a real (SQLite) store. Self-skips with a visible
//! line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::Setting;
use inklkunn_core::appeal::{AppealLimits, AppealService, ContestRef, Outcome};
use inklkunn_core::governance::{PlatformLimits, Require, Target};
use inklkunn_core::message::SystemLabel;
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::tombstone::Tombstones;
use inklkunn_core::{
    AcceptFailure, AcceptStep, AcceptanceParts, Accepted, Authorizer, IdentityAuthority,
    KeyAuthority, LeaseConfig, PreparedFile, Principal, ProposalAcceptor, ReadConfig, ReadService,
    Signer, SignerConfig, StoreAuthz, StoreLedger, SystemActs, SystemRequest, WikiLeases,
    WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, FileChange, GovernanceStore, NewWiki,
    ProposalId, ProposalRecord, ProposalState, ProposalStore, Role, RoleGrant, RoleStore, Slug,
    SqliteStore, SystemActKind,
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
    appeals: AppealService,
    acceptor: Arc<ProposalAcceptor>,
    desk: Authorizer,
    #[allow(dead_code)]
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
        let appeals = AppealService::new(
            authorizer(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            Arc::new(Tombstones::new(store.clone(), store.clone())),
            store.clone(),
            AppealLimits::default(),
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
            appeals,
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

const WORDS: &str = "That change was made in good faith, and I can show it.";
const WHY: &str = "The proposal should have been reviewed.";

#[test]
fn criterion_9_an_overturned_decline_reopens_the_proposal_and_it_is_accepted_and_signed_on_a_real_wiki()
 {
    test_clock!(_OFFSET, clock);
    let Some(w) = World::new("appeal-decline", PlatformLimits::default(), clock) else {
        return;
    };
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // The maintainer declines it.
    w.proposals
        .decline(&w.maintainer.principal, &w.wiki, &page.id, "Not now.")
        .unwrap();
    assert_eq!(w.state(&page.id), ProposalState::Declined);
    let position = w
        .store
        .last_transition_position(&page.id)
        .unwrap()
        .expect("the decline's place in the log");
    let contest = ContestRef {
        kind: inklkunn_store::AppealedKind::Decline,
        id: position.to_string(),
    };
    // A declined proposal cannot be accepted.
    assert!(w.accept(&w.admin, &page.id).is_err());
    let before = w.history_len();

    // The author appeals; the one who declined is refused as the decider, an admin who did not decide it overturns it.
    let filed = w
        .appeals
        .file(&w.author.principal, &w.wiki, &contest, WORDS)
        .unwrap_or_else(|e| panic!("{e}"));
    w.store
        .grant(RoleGrant {
            account: w.maintainer.id.clone(),
            wiki: w.wiki.id.clone(),
            role: Role::Admin,
            granted_by: None,
            granted_at: 1,
        })
        .unwrap();
    // (Now an admin, the one who declined holds the rung that decides: and still does not decide.)
    assert_eq!(
        w.appeals
            .decide(
                &w.maintainer.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY
            )
            .unwrap_err(),
        inklkunn_core::appeal::AppealError::Decider(
            inklkunn_core::appeal::DeciderRefusal::TheActor
        )
    );
    assert_eq!(
        w.state(&page.id),
        ProposalState::Declined,
        "nothing changed"
    );
    let made = w
        .appeals
        .decide(
            &w.admin.principal,
            &w.wiki,
            &filed.id,
            Outcome::Overturned,
            WHY,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.reopened, Some(page.id.clone()));
    assert_eq!(w.state(&page.id), ProposalState::HeldForReview);

    // The overturn made it an ordinary proposal again: it is accepted, signed, and the page says it.
    assert_eq!(
        w.history_len(),
        before,
        "the overturn wrote nothing to the history"
    );
    signed(w.accept(&w.admin, &page.id));
    assert_eq!(w.state(&page.id), ProposalState::Accepted);
    assert_eq!(w.history_len(), before + 1);
    assert_eq!(
        w.file("pages/Home.md").as_deref(),
        Some(home_changed().as_str())
    );
}

#[test]
fn criterion_9_an_overturned_protection_ends_and_the_acceptance_it_held_back_is_signed_on_a_real_wiki()
 {
    test_clock!(_OFFSET, clock);
    let Some(w) = World::new("appeal-protection", PlatformLimits::default(), clock) else {
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
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // While it holds, the acceptance is refused and the repository is as it was.
    let failure = w
        .accept(&w.maintainer, &page.id)
        .expect_err("held back by the protection");
    assert!(
        matches!(failure, AcceptFailure::Protected { .. }),
        "{failure:?}"
    );
    assert_eq!(w.state(&page.id), ProposalState::HeldForReview);

    // The proposer, whom it holds back, appeals it: the protection's own record is the action.
    let protection = w.store.active_protections(&w.wiki.id, clock()).unwrap();
    let held = protection.first().expect("the protection in force");
    let contest = ContestRef {
        kind: inklkunn_store::AppealedKind::Protection,
        id: held.id.to_string(),
    };
    let bystander = w.reviewer.principal.clone();
    assert_eq!(
        w.appeals
            .file(&bystander, &w.wiki, &contest, WORDS)
            .unwrap_err(),
        inklkunn_core::appeal::AppealError::NotAffected,
        "someone it did not hold back cannot appeal it"
    );
    let filed = w
        .appeals
        .file(&w.author.principal, &w.wiki, &contest, WORDS)
        .unwrap_or_else(|e| panic!("{e}"));
    // The maintainer who set it is not the decider, whatever the rung: an admin is.
    w.appeals
        .decide(
            &w.admin.principal,
            &w.wiki,
            &filed.id,
            Outcome::Overturned,
            WHY,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        w.desk
            .protections_on(&w.author.principal, &w.wiki.id, "Home")
            .is_empty(),
        "the protection ended"
    );
    // The same acceptance is signed now.
    signed(w.accept(&w.maintainer, &page.id));
    assert_eq!(w.state(&page.id), ProposalState::Accepted);
    assert_eq!(
        w.file("pages/Home.md").as_deref(),
        Some(home_changed().as_str())
    );
}
