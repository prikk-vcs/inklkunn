//! RFC 006 acceptance criteria 1 – 5 on a real prikk (handoff G-T1, part C): wikis created and written by the real signer,
//! read through the real driver, their rules read from the repository's default view by the real loader, accepted by the
//! service's own orchestration, with the records in a real (SQLite) store. Self-skips with a visible line when prikk is not
//! on `PATH`.
//!
//! - **Criterion 2**: a policy change by proposal changes who may accept, from the next decision on; the decision record
//!   names the new block.
//! - **Criterion 3**: each refusal of D-4 by name, through the real acceptance, leaving the repository and the proposal as
//!   they were.
//! - **Criterion 4**: a policy that raises `approvals` beyond who could give them is accepted, with the warning; an admin
//!   then accepts a change back with the hatch, and the record names it.
//! - **Criterion 5**: a governance file of the wrong shape written outside inklkunn (a system act, as an import would write
//!   it) makes every write refuse, naming the file and the line, while reads work; and the repair works end to end.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::governance::{Entry, PlatformLimits};
use inklkunn_core::message::SystemLabel;
use inklkunn_core::proposal::{ProposalEdit, ProposalError, ProposalService, QuotaConfig};
use inklkunn_core::{
    AcceptFailure, AcceptanceParts, Accepted, Authorizer, IdentityAuthority, KeyAuthority,
    LeaseConfig, PreparedFile, Principal, ProposalAcceptor, ReadConfig, ReadService, Signer,
    SignerConfig, StoreAuthz, StoreLedger, SystemActs, SystemRequest, WikiLeases, WriteLeases,
    create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceRecord, AcceptanceStore, Account, AccountId, AccountStore, ActStore, AuditLog,
    DecisionKind, DisplayName, FileChange, NewWiki, ProposalChange, ProposalId, ProposalOp,
    ProposalRecord, ProposalState, ProposalStore, ProposalTransition, RevisionContent,
    RevisionRecord, Role, RoleGrant, RoleStore, Slug, SqliteStore, SystemActKind,
};

/// Every question this crate asks of the prikk on `PATH`: the only place a test reaches a capability (maintenance M39).
#[path = "support/prikk_probes.rs"]
mod prikk_probes;

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

struct Person {
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
    acts: SystemActs,
    wiki: inklkunn_store::WikiRecord,
    repo: PathBuf,
    admin: Person,
    maintainer: Person,
    reviewer: Person,
    reviewer_two: Person,
    author: Person,
}

impl World {
    /// A wiki with the creation and one import of `pages/Home.md`, under the operator's `limits`.
    fn new(tag: &str, limits: PlatformLimits) -> Option<World> {
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
            .with_audit(store.clone()),
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
        let world = World {
            admin: person("Ada Admin", Role::Admin),
            maintainer: person("Maya Maintainer", Role::Maintainer),
            reviewer: person("Rin Reviewer", Role::Reviewer),
            reviewer_two: person("Sam Reviewer", Role::Reviewer),
            author: person("Noor Contributor", Role::Contributor),
            _tmp: tmp,
            _shm: shm,
            store,
            storage,
            reads,
            proposals,
            acceptor,
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
        let bytes: Vec<(&str, &[u8])> = files.iter().map(|(p, t)| (*p, t.as_bytes())).collect();
        self.write_outside_bytes(&bytes)
    }

    /// [`World::write_outside`] for files that are not text.
    fn write_outside_bytes(&self, files: &[(&str, &[u8])]) -> String {
        let request = SystemRequest {
            kind: SystemActKind::System,
            summary: "Written outside the proposals".to_owned(),
            files: files
                .iter()
                .map(|(path, bytes)| PreparedFile {
                    path: PathBuf::from(path),
                    content: Some(bytes.to_vec()),
                })
                .collect(),
            renames: Vec::new(),
            imported_from: None,
            source: None,
            notice: None,
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

    fn make_other_view(&self) {
        self.acceptor
            .create_view(
                &self.admin.principal,
                &self.wiki,
                "other",
                None,
                inklkunn_core::views::ViewKind::Experimental,
            )
            .unwrap_or_else(|e| panic!("{e}"));
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

// ------------------------------------------------------------------------------------------------- criterion 2

#[test]
fn criterion_2_a_policy_change_by_proposal_changes_who_may_accept_from_the_next_decision_on_and_the_record_names_the_new_block()
 {
    let Some(w) = World::new("crit2", PlatformLimits::default()) else {
        return;
    };
    // A reviewer may not accept under the defaults.
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    assert_eq!(
        w.accept(&w.reviewer, &page.id),
        Err(AcceptFailure::NotAllowed)
    );
    assert_eq!(w.state(&page.id), ProposalState::HeldForReview);
    assert_eq!(w.reads.rules_of(&w.wiki).unwrap().block(), None);

    // The rules say reviewers accept. A maintainer accepts that change, under the defaults.
    let rules = w.propose(
        "Let reviewers accept",
        &[(
            "governance/policy.yaml",
            Some("rungs:\n  accept: reviewer\n"),
        )],
    );
    let before = w.history_len();
    let block = signed(w.accept(&w.maintainer, &rules.id));
    assert_eq!(w.history_len(), before + 1);
    assert_eq!(
        w.file("governance/policy.yaml").as_deref(),
        Some("rungs:\n  accept: reviewer\n")
    );
    // The record of that decision names the rules it was decided under: the defaults, and no hatch.
    let first = w.record(&rules.id);
    assert_eq!((first.policy_block.clone(), first.hatch), (None, false));
    assert_eq!(first.policy_version, "bootstrap-1");

    // From the next decision on, the reviewer accepts, and the record names the block that set the new rules.
    let accepted = signed(w.accept(&w.reviewer, &page.id));
    assert_ne!(accepted, block);
    let second = w.record(&page.id);
    assert_eq!(second.acceptor_role, Role::Reviewer);
    assert_eq!(second.policy_block.as_deref(), Some(block.as_str()));
    assert_eq!(second.policy_version, "governance-1");
    assert_eq!(w.state(&page.id), ProposalState::Accepted);
    assert!(w.file("pages/Home.md").unwrap().contains("Uno."));
    // The authorizer's own trail names the rung and the block, and the proposal records the rung of a review.
    let events = w.store.read(0, 1000).unwrap();
    let short = &block[..12];
    assert!(
        events.iter().any(|e| e.event.action == "accept"
            && e.event.detail == format!("rung=reviewer; policy={short}")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.event.action == "accept"
                && e.event.detail == "rung=reviewer; policy=defaults"),
        "the reviewer's earlier, denied attempt was decided under the defaults"
    );
}

// ------------------------------------------------------------------------------------------------- criterion 3

/// What was refused, with the repository and the proposal as they were.
fn refused_untouched(w: &World, p: &ProposalRecord, who: &Person) -> AcceptFailure {
    let (tip, len) = (w.tip(), w.history_len());
    let failure = match w.accept(who, &p.id) {
        Err(e) => e,
        Ok(done) => panic!("accepted: {done:?}"),
    };
    assert_eq!((w.tip(), w.history_len()), (tip, len), "the history moved");
    assert_eq!(w.state(&p.id), ProposalState::HeldForReview);
    assert!(
        w.store.acceptances_of(&p.id, 1).unwrap().is_empty(),
        "no acceptance began"
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    failure
}

fn names(failure: &AcceptFailure) -> Vec<&'static str> {
    match failure {
        AcceptFailure::GovernanceInvalid(faults) => faults
            .iter()
            .map(inklkunn_core::governance::Fault::name)
            .collect(),
        other => panic!("not a governance refusal: {other:?}"),
    }
}

#[test]
fn criterion_3_each_refusal_of_a_change_to_the_rules_is_by_name_and_leaves_the_repository_and_the_proposal_alone()
 {
    let Some(w) = World::new("crit3", PlatformLimits::default()) else {
        return;
    };
    for (text, name) in [
        ("rungs: accept\n", "wrong-shape"),
        ("rungs:\n  fly: admin\n", "unknown-key"),
        (
            "rungs:\n  propose: maintainer\n  review: reviewer\n  accept: maintainer\n",
            "ordering-propose-above-review",
        ),
        (
            "rungs:\n  review: maintainer\n  accept: reviewer\n",
            "ordering-review-above-accept",
        ),
        (
            "rungs:\n  moderate: contributor\n",
            "ordering-moderate-below-reviewer",
        ),
        (
            "rungs:\n  moderate: admin\n  decide_appeal: maintainer\n",
            "ordering-decide-appeal-below-moderate",
        ),
    ] {
        let p = w.propose("Rules", &[("governance/policy.yaml", Some(text))]);
        let failure = refused_untouched(&w, &p, &w.maintainer);
        assert_eq!(names(&failure), [name], "{text:?}");
        assert!(w.file("governance/policy.yaml").is_none());
    }
    // A governance file in an alternative view. The service refuses such a proposal when it is made, saved, sent and revised
    // (P-6); the acceptance's own refusal holds against a record that reached it some other way (made before the rule, a store
    // written by hand): a page proposal for the view, its second revision written to the store as it is.
    w.make_other_view();
    let rules_elsewhere = |files: Vec<FileChange>| ProposalEdit {
        summary: "Rules, elsewhere".to_owned(),
        files,
        ..ProposalEdit::default()
    };
    let rules = || {
        vec![FileChange {
            path: "governance/policy.yaml".to_owned(),
            text: Some("entry: none\n".to_owned()),
        }]
    };
    assert!(matches!(
        w.proposals.create(
            &w.author.principal,
            &w.wiki,
            Some("heads/other"),
            rules_elsewhere(rules())
        ),
        Err(ProposalError::GovernanceInvalid(_))
    ));
    let id = ProposalId::new();
    w.proposals
        .create_named(
            id.clone(),
            &w.author.principal,
            &w.wiki,
            Some("heads/other"),
            rules_elsewhere(vec![FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(home_changed()),
            }]),
        )
        .unwrap();
    w.proposals
        .submit(&w.author.principal, &w.wiki, &id)
        .unwrap();
    let first = w.store.revisions(&id, None).unwrap().pop().unwrap();
    w.store
        .change_proposal(&ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op: ProposalOp::Revise,
                from: Some(ProposalState::HeldForReview),
                to: ProposalState::HeldForReview,
                actor: w.author.id.clone(),
                reason: None,
                at: first.created_at + 1,
            },
            text: None,
            revision: Some(RevisionRecord {
                proposal: id.clone(),
                seq: first.seq + 1,
                content: RevisionContent {
                    files: rules(),
                    ..first.content.clone()
                },
                created_at: first.created_at + 1,
            }),
            superseded_by: None,
        })
        .unwrap();
    let other_tip = w.tip_in("heads/other");
    let failure = match w.acceptor.accept(&w.maintainer.principal, &w.wiki, &id, 2) {
        Err(e) => e,
        Ok(done) => panic!("accepted: {done:?}"),
    };
    assert_eq!(names(&failure), ["outside-default-view"]);
    assert_eq!(
        w.tip_in("heads/other"),
        other_tip,
        "the other view did not move"
    );
    assert_eq!(w.state(&id), ProposalState::HeldForReview);
    assert!(w.store.acceptances_of(&id, 2).unwrap().is_empty());
}

#[test]
fn criterion_3_a_platform_limit_exceeded_is_refused_naming_it_and_a_lowered_limit_reads_an_older_policy_at_the_limit()
 {
    // The operator allows `entry` up to observer.
    let Some(w) = World::new(
        "crit3-limit",
        PlatformLimits {
            entry_max: Entry::Observer,
            ..PlatformLimits::default()
        },
    ) else {
        return;
    };
    let p = w.propose(
        "Everyone contributes",
        &[("governance/policy.yaml", Some("entry: contributor\n"))],
    );
    let failure = refused_untouched(&w, &p, &w.maintainer);
    assert_eq!(names(&failure), ["platform-limit-entry-max"]);
    // A policy written before the operator lowered the limit (here: outside the proposals) is read as if at the limit.
    w.write_outside(&[("governance/policy.yaml", "entry: contributor\n")]);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert_eq!(rules.policy().entry(), Entry::Observer);
    assert_eq!(rules.lowered().len(), 1);
}

// ------------------------------------------------------------------------------------------------- criterion 4

#[test]
fn criterion_4_a_policy_asking_for_more_approvals_than_anyone_can_give_is_accepted_with_a_warning_and_an_admin_repairs_it_by_the_hatch()
 {
    let Some(w) = World::new("crit4", PlatformLimits::default()) else {
        return;
    };
    // Two people hold the review rung besides the proposer; the policy asks for four approvals. It is accepted, not refused.
    let ask = w.propose(
        "Four approvals before anything is accepted",
        &[(
            "governance/policy.yaml",
            Some("acceptance:\n  approvals: 4\n"),
        )],
    );
    let block = signed(w.accept(&w.maintainer, &ask.id));
    assert_eq!(
        w.reads
            .rules_of(&w.wiki)
            .unwrap()
            .policy()
            .acceptance()
            .approvals(),
        4
    );
    // The warning is computed on read, for the next proposal: four asked, and only the reviewers (two reviewers, the
    // maintainer and the admin: four holders of the review rung besides the author) could give them. Make it five.
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    let warning = w.proposals.approvals_warning(&w.wiki, &page).unwrap();
    assert_eq!(warning, None, "four people could give four approvals");
    // Raise the ask beyond the people: five approvals.
    let more = w.propose(
        "Five",
        &[(
            "governance/policy.yaml",
            Some("acceptance:\n  approvals: 5\n"),
        )],
    );
    for who in [&w.reviewer, &w.reviewer_two, &w.maintainer, &w.admin] {
        w.approve(who, &more.id);
    }
    signed(w.accept(&w.maintainer, &more.id));
    let page2 = w.propose(
        "Another",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    let warning = w
        .proposals
        .approvals_warning(&w.wiki, &page2)
        .unwrap()
        .unwrap();
    assert_eq!((warning.needed, warning.available), (5, 4));
    // No proposal can be accepted now: the maintainer's acceptance is refused for want of approvals.
    assert_eq!(
        w.accept(&w.maintainer, &page2.id),
        Err(AcceptFailure::ApprovalsMissing { needed: 5, have: 0 })
    );
    // An admin accepts a change back, with the hatch: no approvals, and the record names it.
    let back = w.propose(
        "Back to none",
        &[(
            "governance/policy.yaml",
            Some("acceptance:\n  approvals: 0\n"),
        )],
    );
    let hatch_block = signed(w.accept(&w.admin, &back.id));
    let record = w.record(&back.id);
    assert!(record.hatch, "the acceptance names the hatch");
    assert_eq!(record.acceptor_role, Role::Admin);
    assert_ne!(hatch_block, block);
    assert_eq!(
        w.reads
            .rules_of(&w.wiki)
            .unwrap()
            .policy()
            .acceptance()
            .approvals(),
        0
    );
    // The wiki can be used again.
    signed(w.accept(&w.maintainer, &page2.id));
}

// ------------------------------------------------------------------------------------------------- criterion 5

#[test]
fn criterion_5_a_file_of_the_wrong_shape_written_outside_inklkunn_refuses_every_write_naming_the_file_and_line_while_reads_work()
 {
    let Some(w) = World::new("crit5", PlatformLimits::default()) else {
        return;
    };
    // A proposal made while the wiki was well, to be accepted after it breaks.
    let open = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // The file arrives as an import would bring it: nothing judges it.
    let broke = w.write_outside(&[(
        "governance/policy.yaml",
        "entry: none\nrungs:\n  accept: maintainer\n  fly: admin\n",
    )]);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    let fault = rules.fault().expect("the wiki's governance cannot be read");
    assert_eq!(
        fault.to_string().split(':').next(),
        Some("governance/policy.yaml, line 4")
    );
    assert_eq!(rules.block(), Some(broke.as_str()));

    // Every write refuses, naming the file and the line.
    let refused = |e: &dyn std::fmt::Display| {
        let said = e.to_string();
        assert!(said.contains("governance/policy.yaml, line 4"), "{said}");
    };
    let before = (w.tip(), w.history_len());
    for who in [&w.admin, &w.maintainer, &w.reviewer, &w.author] {
        let e = w.accept(who, &open.id).unwrap_err();
        assert!(matches!(e, AcceptFailure::RulesUnreadable(_)), "{e:?}");
        refused(&e);
        let e = w
            .proposals
            .create(
                &who.principal,
                &w.wiki,
                None,
                ProposalEdit {
                    summary: "Another".to_owned(),
                    files: vec![FileChange {
                        path: "pages/Other.md".to_owned(),
                        text: Some("---\ntitle: Other\n---\n\nHi.\n".to_owned()),
                    }],
                    ..ProposalEdit::default()
                },
            )
            .unwrap_err();
        assert!(matches!(e, ProposalError::RulesUnreadable(_)), "{e:?}");
        refused(&e);
        let e = w
            .proposals
            .decide(
                &who.principal,
                &w.wiki,
                &open.id,
                1,
                DecisionKind::Approve,
                "",
            )
            .unwrap_err();
        assert!(matches!(e, ProposalError::RulesUnreadable(_)), "{e:?}");
    }
    assert_eq!((w.tip(), w.history_len()), before, "nothing was written");
    assert_eq!(w.state(&open.id), ProposalState::HeldForReview);
    // Reads work: the page is served as it was, and the proposal can be read.
    assert!(w.file("pages/Home.md").unwrap().contains("One."));
    assert!(
        w.proposals
            .get(&w.reviewer.principal, &w.wiki, &open.id)
            .is_ok()
    );
    assert!(
        w.proposals
            .revisions(&w.reviewer.principal, &w.wiki, &open.id)
            .is_ok()
    );
}

#[test]
fn criterion_5_the_repair_by_an_admin_works_end_to_end_and_nothing_else_does() {
    let Some(w) = World::new("crit5-repair", PlatformLimits::default()) else {
        return;
    };
    let open = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    let broke = w.write_outside(&[("governance/constitution.yaml", "policy_change: [1\n")]);
    // The admin proposes the repair: the two files and nothing else. A maintainer's proposal of it is refused to begin with.
    let e = w
        .proposals
        .create(
            &w.maintainer.principal,
            &w.wiki,
            None,
            ProposalEdit {
                summary: "Mend".to_owned(),
                files: vec![FileChange {
                    path: "governance/constitution.yaml".to_owned(),
                    text: Some("policy_change:\n  approvals: 0\n".to_owned()),
                }],
                ..ProposalEdit::default()
            },
        )
        .unwrap_err();
    assert!(matches!(e, ProposalError::RulesUnreadable(_)), "{e:?}");
    let repair = w.propose_as(
        &w.admin,
        "Mend the constitution",
        &[(
            "governance/constitution.yaml",
            Some("policy_change:\n  approvals: 0\n"),
        )],
    );
    // A maintainer cannot accept it: the repair is an admin's.
    assert!(matches!(
        w.accept(&w.maintainer, &repair.id),
        Err(AcceptFailure::RulesUnreadable(_))
    ));
    // The admin cannot use the repair for anything else: a page, alone or with the rules, is refused.
    assert!(matches!(
        w.accept(&w.admin, &open.id),
        Err(AcceptFailure::RulesUnreadable(_))
    ));
    let e = w
        .proposals
        .create(
            &w.admin.principal,
            &w.wiki,
            None,
            ProposalEdit {
                summary: "A page and the rules".to_owned(),
                files: vec![
                    FileChange {
                        path: "pages/Home.md".to_owned(),
                        text: Some(home_changed()),
                    },
                    FileChange {
                        path: "governance/constitution.yaml".to_owned(),
                        text: Some("policy_change:\n  approvals: 0\n".to_owned()),
                    },
                ],
                ..ProposalEdit::default()
            },
        )
        .unwrap_err();
    assert!(
        matches!(e, ProposalError::RulesUnreadable(_)),
        "a proposal of a page beside the rules is not the repair: {e:?}"
    );
    // The admin accepts the repair, by the hatch, under the default policy.
    let healed = signed(w.accept(&w.admin, &repair.id));
    let record = w.record(&repair.id);
    assert!(record.hatch);
    assert_eq!(record.policy_block.as_deref(), Some(broke.as_str()));
    assert_eq!(
        w.file("governance/constitution.yaml").as_deref(),
        Some("policy_change:\n  approvals: 0\n")
    );
    // The wiki is well again: no fault, a block that names the repair, and the writes it refused are done.
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert!(rules.fault().is_none());
    assert_eq!(rules.block(), Some(healed.as_str()));
    signed(w.accept(&w.maintainer, &open.id));
    assert!(w.file("pages/Home.md").unwrap().contains("Uno."));
}

#[test]
fn criterion_5_a_directory_where_a_governance_file_belongs_refuses_every_write_naming_the_path() {
    let Some(w) = World::new("crit5-dir", PlatformLimits::default()) else {
        return;
    };
    let open = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // A directory is where `governance/policy.yaml` belongs: written outside inklkunn, as an import or a hand edit would.
    // (prikk keeps no links, so a directory is the one non-file a repository holds there.) It is no file, so it was read as
    // the defaults before this check.
    let broke = w.write_outside(&[("governance/policy.yaml/inner.txt", "x\n")]);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    let fault = rules.fault().expect("the wiki's governance cannot be read");
    assert_eq!(fault.file().path(), "governance/policy.yaml");
    assert_eq!(
        fault.refusal(),
        inklkunn_core::governance::Refusal::NotAFile
    );
    assert!(
        fault.to_string().starts_with("governance/policy.yaml: "),
        "{fault}"
    );
    assert_eq!(rules.block(), Some(broke.as_str()));
    // Every write refuses, naming the path; a read works.
    let before = (w.tip(), w.history_len());
    for who in [&w.admin, &w.maintainer] {
        let e = w.accept(who, &open.id).unwrap_err();
        assert!(matches!(e, AcceptFailure::RulesUnreadable(_)), "{e:?}");
        assert!(e.to_string().contains("governance/policy.yaml"), "{e}");
    }
    assert_eq!((w.tip(), w.history_len()), before, "nothing was written");
    assert_eq!(w.state(&open.id), ProposalState::HeldForReview);
    assert!(w.file("pages/Home.md").unwrap().contains("One."));
}

#[test]
fn a_wiki_whose_files_are_fine_has_no_such_state_and_the_repair_is_not_the_hatch_of_a_good_wiki() {
    let Some(w) = World::new("crit5-fine", PlatformLimits::default()) else {
        return;
    };
    w.write_outside(&[("governance/policy.yaml", "entry: observer\n")]);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert!(rules.fault().is_none());
    assert_eq!(rules.policy().entry(), Entry::Observer);
    // Writes are not refused: an admin's page change is an ordinary proposal accepted by an ordinary acceptance.
    let page = w.propose_as(
        &w.admin,
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    signed(w.accept(&w.maintainer, &page.id));
    let record = w.record(&page.id);
    assert!(!record.hatch);
}

// ------------------------------------------------------------------------------------------------- correction D-1

/// The derived store's row for the default view's tip: the rules as the loader built them, keyed by the tip.
fn derived_row(w: &World) -> inklkunn_store::GovernanceSnapshot {
    use inklkunn_store::GovernanceSnapshotStore;
    let tip = w.reads.tip(&w.wiki, None).expect("tip");
    w.store
        .governance_snapshot(&w.wiki.id, tip.state_id())
        .expect("store")
        .expect("a row")
}

fn verified(w: &World) {
    assert!(w.storage.verify(&w.repo).expect("verify").verdict.ok);
}

fn edit(summary: &str, files: &[(&str, Option<&str>)]) -> ProposalEdit {
    ProposalEdit {
        summary: summary.to_owned(),
        files: files
            .iter()
            .map(|(path, text)| FileChange {
                path: (*path).to_owned(),
                text: text.map(str::to_owned),
            })
            .collect(),
        ..ProposalEdit::default()
    }
}

#[test]
fn d1_a_directory_where_the_policy_belongs_is_cleared_and_the_file_written_by_one_admins_proposal()
{
    let Some(w) = World::new("d1-repair", PlatformLimits::default()) else {
        return;
    };
    let open = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    // A directory stands where `governance/policy.yaml` belongs, with files in it and in a directory of its own.
    let broke = w.write_outside(&[
        ("governance/policy.yaml/inner.txt", "x\n"),
        ("governance/policy.yaml/sub/deeper.txt", "y\n"),
    ]);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert_eq!(rules.block(), Some(broke.as_str()));
    assert!(rules.fault().is_some());
    // What the page tells the admin to remove (E-1) is what the loader found at and below the path in the repository: the names
    // are read from the fault the unreadable-state page and the API problem are built from, never from this test's own
    // fixture, and an admin's repair is the change that removes exactly these and writes the file.
    let fault = rules.fault().expect("a fault");
    assert_eq!(
        fault.found(),
        [
            "governance/policy.yaml/inner.txt",
            "governance/policy.yaml/sub/deeper.txt"
        ],
        "what the page lists"
    );
    assert_eq!(fault.more(), 0);
    let removals: Vec<String> = fault.found().to_vec();
    // The repair, an admin's: remove what is below the path, write the file, in one proposal. A maintainer may not propose it.
    let mut files: Vec<(&str, Option<&str>)> =
        removals.iter().map(|p| (p.as_str(), None)).collect();
    files.push(("governance/policy.yaml", Some("entry: observer\n")));
    let files = files.as_slice();
    let refused = w
        .proposals
        .create(
            &w.maintainer.principal,
            &w.wiki,
            None,
            edit("Clear the directory", files),
        )
        .unwrap_err();
    assert!(
        matches!(refused, ProposalError::RulesUnreadable(_)),
        "{refused:?}"
    );
    let repair = w.propose_as(&w.admin, "Clear the directory", files);
    // A maintainer cannot accept it, and nothing else is accepted meanwhile.
    assert!(matches!(
        w.accept(&w.maintainer, &repair.id),
        Err(AcceptFailure::RulesUnreadable(_))
    ));
    assert!(matches!(
        w.accept(&w.admin, &open.id),
        Err(AcceptFailure::RulesUnreadable(_))
    ));
    let before = w.history_len();
    let healed = signed(w.accept(&w.admin, &repair.id));
    assert_eq!(w.history_len(), before + 1, "one block");
    let record = w.record(&repair.id);
    assert!(record.hatch, "an admin's repair is the hatch");
    assert_eq!(record.policy_block.as_deref(), Some(broke.as_str()));
    assert_eq!(
        w.file("governance/policy.yaml").as_deref(),
        Some("entry: observer\n")
    );
    assert_eq!(w.file("governance/policy.yaml/inner.txt"), None);
    assert_eq!(w.file("governance/policy.yaml/sub/deeper.txt"), None);
    verified(&w);
    // The rules are read again: no fault, the block that names the repair, and the derived row says so.
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert!(rules.fault().is_none());
    assert_eq!(rules.block(), Some(healed.as_str()));
    assert_eq!(rules.policy().entry(), Entry::Observer);
    let row = derived_row(&w);
    assert!(row.fault.is_none());
    assert_eq!(row.policy_block.as_deref(), Some(healed.as_str()));
    // The wiki takes writes again: the page proposal made before, and one made now by a contributor.
    signed(w.accept(&w.maintainer, &open.id));
    assert!(w.file("pages/Home.md").unwrap().contains("Uno."));
    let again = w.propose(
        "Say it in Italian",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno!")))],
    );
    signed(w.accept(&w.maintainer, &again.id));
    verified(&w);
}

#[test]
fn d1_removing_only_what_is_below_the_path_is_a_repair_too_and_the_wiki_is_then_on_the_defaults() {
    let Some(w) = World::new("d1-clear", PlatformLimits::default()) else {
        return;
    };
    w.write_outside(&[("governance/constitution.yaml/inner.txt", "x\n")]);
    assert!(w.reads.rules_of(&w.wiki).unwrap().fault().is_some());
    let repair = w.propose_as(
        &w.admin,
        "Clear the directory",
        &[("governance/constitution.yaml/inner.txt", None)],
    );
    signed(w.accept(&w.admin, &repair.id));
    assert!(w.record(&repair.id).hatch);
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert!(rules.fault().is_none());
    assert_eq!(
        rules.block(),
        None,
        "no governance file: the defaults, with no block that set them"
    );
    assert_eq!(w.file("governance/constitution.yaml"), None);
    verified(&w);
}

#[test]
fn d1_an_unknown_file_under_governance_in_a_sound_wiki_is_removed_like_any_file_and_never_written()
{
    let Some(w) = World::new("d1-unknown", PlatformLimits::default()) else {
        return;
    };
    // Written outside inklkunn; the loader reads the two files only, so the wiki is sound.
    w.write_outside(&[("governance/x/y.yaml", "z: 1\n")]);
    assert!(w.reads.rules_of(&w.wiki).unwrap().fault().is_none());
    // The API does not write another path: refused by name, as before.
    let e = w
        .proposals
        .create(
            &w.admin.principal,
            &w.wiki,
            None,
            edit("Write it", &[("governance/x/other.yaml", Some("z: 2\n"))]),
        )
        .unwrap_err();
    assert!(
        matches!(
            e,
            ProposalError::Invalid(inklkunn_core::proposal::Invalid::GovernancePath)
        ),
        "{e:?}"
    );
    // It is removed by a proposal like any file, by the ordinary acceptance (it is no rule).
    let removal = w.propose_as(&w.admin, "Remove it", &[("governance/x/y.yaml", None)]);
    signed(w.accept(&w.maintainer, &removal.id));
    assert!(!w.record(&removal.id).hatch);
    assert_eq!(w.file("governance/x/y.yaml"), None);
    verified(&w);
}

#[test]
fn d1_while_the_rules_cannot_be_read_the_repair_is_the_two_files_and_what_is_below_them_and_a_stray_file_waits()
 {
    let Some(w) = World::new("d1-stray", PlatformLimits::default()) else {
        return;
    };
    w.write_outside(&[
        ("governance/policy.yaml/inner.txt", "x\n"),
        ("governance/x/y.yaml", "z: 1\n"),
    ]);
    // A removal of the stray file beside the repair is not the repair: refused to begin with, in that state.
    let e = w
        .proposals
        .create(
            &w.admin.principal,
            &w.wiki,
            None,
            edit(
                "Both",
                &[
                    ("governance/policy.yaml/inner.txt", None),
                    ("governance/x/y.yaml", None),
                ],
            ),
        )
        .unwrap_err();
    assert!(matches!(e, ProposalError::RulesUnreadable(_)), "{e:?}");
    // The repair first, then the stray file.
    let repair = w.propose_as(
        &w.admin,
        "Clear the directory",
        &[("governance/policy.yaml/inner.txt", None)],
    );
    signed(w.accept(&w.admin, &repair.id));
    let stray = w.propose_as(&w.admin, "Remove it", &[("governance/x/y.yaml", None)]);
    signed(w.accept(&w.maintainer, &stray.id));
    assert_eq!(w.file("governance/x/y.yaml"), None);
    verified(&w);
}

#[test]
fn d1_removing_a_governance_file_returns_the_wiki_to_the_defaults_and_is_judged_like_any_change_to_the_rules()
 {
    let Some(w) = World::new("d1-defaults", PlatformLimits::default()) else {
        return;
    };
    // The rules say: a reviewer accepts.
    let set = w.propose(
        "Let reviewers accept",
        &[(
            "governance/policy.yaml",
            Some("rungs:\n  accept: reviewer\n"),
        )],
    );
    signed(w.accept(&w.maintainer, &set.id));
    let page = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&home_changed()))],
    );
    assert!(w.reads.rules_of(&w.wiki).unwrap().block().is_some());
    // Removing the policy is a change to the policy: the constitution's rung, not a page's.
    let removal = w.propose_as(
        &w.admin,
        "Back to the defaults",
        &[("governance/policy.yaml", None)],
    );
    assert_eq!(
        w.accept(&w.reviewer, &removal.id),
        Err(AcceptFailure::GovernanceRung(Role::Maintainer))
    );
    let before = w.history_len();
    signed(w.accept(&w.maintainer, &removal.id));
    assert_eq!(w.history_len(), before + 1);
    assert_eq!(w.file("governance/policy.yaml"), None);
    verified(&w);
    // A missing file is its defaults: the rules are read, and with no governance file left no block set them; a reviewer may not
    // accept.
    let rules = w.reads.rules_of(&w.wiki).unwrap();
    assert!(rules.fault().is_none());
    assert_eq!(rules.block(), None);
    assert_eq!(
        w.accept(&w.reviewer, &page.id),
        Err(AcceptFailure::NotAllowed)
    );
    signed(w.accept(&w.maintainer, &page.id));
    // The constitution is judged the same way: its removal needs an admin (a maintainer is refused), and the admin's is the
    // hatch, whatever the constitution asked.
    let strict = w.propose_as(
        &w.admin,
        "Ask for approvals",
        &[(
            "governance/constitution.yaml",
            Some("policy_change:\n  approvals: 2\n"),
        )],
    );
    signed(w.accept(&w.admin, &strict.id));
    let drop = w.propose_as(
        &w.admin,
        "Back to the defaults",
        &[("governance/constitution.yaml", None)],
    );
    assert_eq!(
        w.accept(&w.maintainer, &drop.id),
        Err(AcceptFailure::GovernanceRung(Role::Admin))
    );
    signed(w.accept(&w.admin, &drop.id));
    assert!(w.record(&drop.id).hatch);
    assert_eq!(w.file("governance/constitution.yaml"), None);
    verified(&w);
}

#[test]
fn d1_a_repair_made_against_an_older_tip_is_applied_when_nothing_it_names_has_changed_and_stops_when_something_has()
 {
    let Some(w) = World::new("d1-moved", PlatformLimits::default()) else {
        return;
    };
    w.write_outside(&[("governance/policy.yaml/inner.txt", "x\n")]);
    let files = [
        ("governance/policy.yaml/inner.txt", None),
        ("governance/policy.yaml", Some("entry: observer\n")),
    ];
    let repair = w.propose_as(&w.admin, "Clear the directory", &files);
    let second = w.propose_as(&w.admin, "Clear it again", &files);
    // The wiki moves on by something the repair does not name (a system act, as an import would make).
    w.write_outside(&[(
        "pages/Other.md",
        "---\ntitle: Other\nid: p-other\n---\n\nHi.\n",
    )]);
    signed(w.accept(&w.admin, &repair.id));
    assert_eq!(
        w.file("governance/policy.yaml").as_deref(),
        Some("entry: observer\n")
    );
    assert_eq!(w.file("governance/policy.yaml/inner.txt"), None);
    verified(&w);
    // The second was made against the old tip, and what it names has changed since: a stop, not a merge.
    let result = w.accept(&w.admin, &second.id);
    assert!(
        matches!(result, Err(AcceptFailure::Overlap { overlaps: 1 })),
        "{result:?}"
    );
    assert_eq!(
        w.file("governance/policy.yaml").as_deref(),
        Some("entry: observer\n")
    );
}

#[test]
fn d1_what_is_removed_need_not_be_text_and_a_repair_made_against_an_older_tip_still_applies() {
    let Some(w) = World::new("d1-binary", PlatformLimits::default()) else {
        return;
    };
    // A file that is not text stands below the path (an import or a hand edit may leave anything there).
    w.write_outside_bytes(&[("governance/policy.yaml/blob.bin", &[0xff, 0xfe, 0x00, 0x80])]);
    assert!(w.reads.rules_of(&w.wiki).unwrap().fault().is_some());
    let repair = w.propose_as(
        &w.admin,
        "Clear the directory",
        &[
            ("governance/policy.yaml/blob.bin", None),
            ("governance/policy.yaml", Some("entry: observer\n")),
        ],
    );
    // The wiki moves on by something the repair does not name; the merge could not read the file as a page, and this is not one.
    w.write_outside(&[(
        "pages/Other.md",
        "---\ntitle: Other\nid: p-other\n---\n\nHi.\n",
    )]);
    signed(w.accept(&w.admin, &repair.id));
    assert_eq!(w.file("governance/policy.yaml/blob.bin"), None);
    assert_eq!(
        w.file("governance/policy.yaml").as_deref(),
        Some("entry: observer\n")
    );
    assert!(w.reads.rules_of(&w.wiki).unwrap().fault().is_none());
    verified(&w);
}

#[test]
fn d1_a_path_below_the_policy_that_changed_since_the_repair_was_made_stops_it() {
    let Some(w) = World::new("d1-changed", PlatformLimits::default()) else {
        return;
    };
    w.write_outside(&[("governance/policy.yaml/inner.txt", "x\n")]);
    let repair = w.propose_as(
        &w.admin,
        "Clear the directory",
        &[
            ("governance/policy.yaml/inner.txt", None),
            ("governance/policy.yaml", Some("entry: observer\n")),
        ],
    );
    // Someone changes the file since: the repair did not see that, and it is not the repair any more.
    w.write_outside(&[("governance/policy.yaml/inner.txt", "changed\n")]);
    let before = (w.tip(), w.history_len());
    let result = w.accept(&w.admin, &repair.id);
    assert!(
        matches!(result, Err(AcceptFailure::Overlap { overlaps: 1 })),
        "{result:?}"
    );
    assert_eq!((w.tip(), w.history_len()), before, "nothing was written");
    assert_eq!(
        w.file("governance/policy.yaml/inner.txt").as_deref(),
        Some("changed\n")
    );
}
