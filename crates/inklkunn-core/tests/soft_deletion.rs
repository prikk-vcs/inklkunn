//! RFC 006 acceptance criteria 10 and 11 on a real prikk (handoff G-T6): **soft deletion** and **retiring knowledge** (the second part of this file). Soft deletion first: A page is removed by an accepted proposal
//! and the read service says so from the page's own history ([`ReadService::removed_page`]): when (the change), which proposal and
//! why. The way back is **a rollback of that change when it merges cleanly over the tip, and the page's last text when later
//! changes block it**; each is a draft proposal, accepted like any other, and the page is back with the history showing both
//! changes. A title that was never a page, and a page that was moved, are not removed pages. Self-skips with a visible line when
//! prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::governance::PlatformLimits;
use inklkunn_core::message::SystemLabel;
use inklkunn_core::proposal::rollback::restore_edit;
use inklkunn_core::proposal::{Invalid, ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::read::{PageEvent, Restore, RollbackOutcome};
use inklkunn_core::{
    AcceptFailure, AcceptStep, AcceptanceParts, Accepted, Authorizer, IdentityAuthority,
    KeyAuthority, LeaseConfig, PreparedFile, Principal, ProposalAcceptor, ReadConfig, ReadService,
    Signer, SignerConfig, StoreAuthz, StoreLedger, SystemActs, SystemRequest, WikiLeases,
    WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceRecord, AcceptanceStore, Account, AccountId, AccountStore, DecisionKind, DisplayName,
    FileChange, NewWiki, ProposalId, ProposalRecord, ProposalState, ProposalStore, Role, RoleGrant,
    RoleStore, Slug, SqliteStore, SystemActKind,
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
    layout: inklkunn_store::RootLayout,
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
            layout: layout.clone(),
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

const GUIDE: &str = "---\ntitle: Guide\nid: p-guide\n---\n\nThe guide.\n\nSecond paragraph.\n";

fn signed(done: Result<Accepted, AcceptFailure>) -> String {
    match done {
        Ok(Accepted::Signed(s)) => s.block_id,
        other => panic!("not signed: {other:?}"),
    }
}

fn now() -> i64 {
    inklkunn_store::record::now_secs()
}

fn exists(w: &World, title: &str) -> bool {
    w.file(&format!("pages/{title}.md")).is_some()
}

#[test]
fn criterion_10_a_removed_page_is_found_by_its_history_and_is_restored_by_a_rollback_when_it_merges_cleanly()
 {
    let Some(w) = World::new("soft-rollback", PlatformLimits::default(), now) else {
        return;
    };
    w.write_outside(&[("pages/Guide.md", GUIDE)]);
    // Never a page: no removal, whatever the history.
    assert_eq!(
        w.reads.removed_page(&w.wiki, None, "Nowhere").unwrap(),
        None
    );
    // A page that exists is not a removed one.
    assert_eq!(w.reads.removed_page(&w.wiki, None, "Guide").unwrap(), None);

    let removal = w.propose(
        "Remove the guide: it is out of date",
        &[("pages/Guide.md", None)],
    );
    let removing_block = signed(w.accept(&w.maintainer, &removal.id));
    assert!(!exists(&w, "Guide"));
    w.reads.forget_wiki(&w.wiki);

    let removed = w
        .reads
        .removed_page(&w.wiki, None, "Guide")
        .unwrap()
        .expect("the page was removed");
    assert_eq!(removed.title, "Guide");
    assert_eq!(removed.path, "pages/Guide.md");
    assert_eq!(removed.removal.block_id, removing_block);
    assert_eq!(
        removed.removal.proposal.as_deref(),
        Some(removal.id.as_str())
    );
    // The reason is the proposal's summary, read through the one accessor.
    assert_eq!(
        removed.reason(),
        Some("Remove the guide: it is out of date")
    );
    let Restore::Rollback { undoes } = removed.restore.clone() else {
        panic!("a clean rollback was expected: {:?}", removed.restore);
    };
    assert_eq!(undoes, removing_block);
    // A title that was never a page stays so.
    assert_eq!(
        w.reads.removed_page(&w.wiki, None, "Nowhere").unwrap(),
        None
    );

    // The way back: a draft rollback, sent, accepted; the page is back, and the history shows the three changes.
    let RollbackOutcome::Plan(plan) = w.reads.rollback_plan(&w.wiki, None, &undoes).unwrap() else {
        panic!("a plan");
    };
    let draft = w
        .proposals
        .create_rollback(&w.author.principal, &w.wiki, None, &plan)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(draft.state, ProposalState::Draft);
    let held = w
        .proposals
        .submit(&w.author.principal, &w.wiki, &draft.id)
        .unwrap_or_else(|e| panic!("{e}"));
    signed(w.accept(&w.maintainer, &held.id));
    assert_eq!(w.file("pages/Guide.md").as_deref(), Some(GUIDE));
    w.reads.forget_wiki(&w.wiki);
    assert_eq!(w.reads.removed_page(&w.wiki, None, "Guide").unwrap(), None);
    let history = match w.reads.page_history(&w.wiki, None, "Guide", None).unwrap() {
        inklkunn_core::read::HistoryOutcome::Page(history) => history,
        other => panic!("{other:?}"),
    };
    let mut events: Vec<PageEvent> = history.rows.iter().flat_map(|r| r.events.clone()).collect();
    // Newest first: restored, removed, made.
    events.reverse();
    assert_eq!(
        events,
        [PageEvent::Created, PageEvent::Deleted, PageEvent::Created],
        "the history shows the page made, removed and restored"
    );
}

#[test]
fn criterion_10_when_later_changes_block_the_rollback_the_page_comes_back_with_its_last_text() {
    let Some(w) = World::new("soft-last", PlatformLimits::default(), now) else {
        return;
    };
    w.write_outside(&[("pages/Guide.md", GUIDE)]);
    // One change removes the guide and rewords the front page; a later change rewords the same place of the front page again.
    let removal = w.propose(
        "Remove the guide and reword the front page",
        &[
            ("pages/Guide.md", None),
            ("pages/Home.md", Some(&home_changed())),
        ],
    );
    let removing_block = signed(w.accept(&w.maintainer, &removal.id));
    let later = w.propose(
        "Reword the front page again",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Eins.")))],
    );
    signed(w.accept(&w.maintainer, &later.id));
    w.reads.forget_wiki(&w.wiki);

    let removed = w
        .reads
        .removed_page(&w.wiki, None, "Guide")
        .unwrap()
        .expect("the page was removed");
    assert_eq!(removed.removal.block_id, removing_block);
    let Restore::LastText { from } = removed.restore.clone() else {
        panic!(
            "the rollback is blocked, so the last text is offered: {:?}",
            removed.restore
        );
    };
    let text = w
        .reads
        .source_at(&w.wiki, &from, &removed.path)
        .unwrap()
        .expect("the text the page had last");
    assert_eq!(text, GUIDE);
    let edit = restore_edit(&removed.title, &removed.path, text, &removed.tip).expect("an edit");
    assert_eq!(edit.summary, "Restore: Guide");
    let draft = w
        .proposals
        .create(&w.author.principal, &w.wiki, None, edit)
        .unwrap_or_else(|e| panic!("{e}"));
    let held = w
        .proposals
        .submit(&w.author.principal, &w.wiki, &draft.id)
        .unwrap_or_else(|e| panic!("{e}"));
    signed(w.accept(&w.maintainer, &held.id));
    // The page is back as it was, and what the later change did to the front page stays.
    assert_eq!(w.file("pages/Guide.md").as_deref(), Some(GUIDE));
    assert!(w.file("pages/Home.md").unwrap().contains("Eins."));
    w.reads.forget_wiki(&w.wiki);
    assert_eq!(w.reads.removed_page(&w.wiki, None, "Guide").unwrap(), None);
}

#[test]
fn the_newest_change_that_touched_the_path_decides_whether_it_is_a_removed_page() {
    let Some(w) = World::new("soft-newest", PlatformLimits::default(), now) else {
        return;
    };
    w.write_outside(&[("pages/Guide.md", GUIDE)]);
    let removal = w.propose("Remove the guide", &[("pages/Guide.md", None)]);
    signed(w.accept(&w.maintainer, &removal.id));
    w.reads.forget_wiki(&w.wiki);
    assert!(
        w.reads
            .removed_page(&w.wiki, None, "Guide")
            .unwrap()
            .is_some()
    );
    // The page is made again by a change nobody proposed: it is a page, and so no removed one.
    w.write_outside(&[("pages/Guide.md", GUIDE)]);
    assert_eq!(w.reads.removed_page(&w.wiki, None, "Guide").unwrap(), None);
    // Removed once more: the newest change decides, and it is the second removal.
    let again = w.propose("Remove the guide again", &[("pages/Guide.md", None)]);
    let second = signed(w.accept(&w.maintainer, &again.id));
    w.reads.forget_wiki(&w.wiki);
    let removed = w
        .reads
        .removed_page(&w.wiki, None, "Guide")
        .unwrap()
        .unwrap();
    assert_eq!(removed.removal.block_id, second);
    assert_eq!(removed.reason(), Some("Remove the guide again"));
}

// ------------------------------------------------------------------------------------ criterion 11: retiring

/// A page paragraph with a source attached, in one accepted proposal: the claim, the source and the page. Returns the claim file and
/// the source file (paths and texts), the claim's id and the paragraph's block.
struct Attached {
    claim_path: String,
    source_path: String,
    claim: inklkunn_core::knowledge::ClaimId,
    source: inklkunn_core::knowledge::SourceId,
}

fn attach_a_source(w: &World) -> Attached {
    use inklkunn_core::knowledge::compose::{Attachment, SourceDraft, attach};
    use inklkunn_core::knowledge::{Bearing, Link, SourceKind, StatementKind};
    let start = HOME.find("One.").unwrap();
    let attached = attach(
        HOME,
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
    let [(claim_path, claim_text), (source_path, source_text)] = attached.files;
    let p = w.propose(
        "A source for the first paragraph",
        &[
            ("pages/Home.md", Some(&attached.page)),
            (claim_path.as_str(), Some(&claim_text)),
            (source_path.as_str(), Some(&source_text)),
        ],
    );
    signed(w.accept(&w.maintainer, &p.id));
    w.reads.forget_wiki(&w.wiki);
    Attached {
        claim_path,
        source_path,
        claim: attached.claim,
        source: attached.source,
    }
}

/// The curation `edit` of the claim and its source as the tip has them, made a proposal by the contributor.
fn curate_with(
    w: &World,
    a: &Attached,
    summary: &str,
    retire: inklkunn_core::knowledge::compose::Retire,
    source_retire: inklkunn_core::knowledge::compose::Retire,
) -> Result<ProposalRecord, inklkunn_core::knowledge::compose::Mistake> {
    use inklkunn_core::knowledge::compose::{Curation, LinkEdit, SourceEdit, curate};
    use inklkunn_core::knowledge::{File, Standing, read_file};
    let File::Claim(claim) =
        read_file(&a.claim_path, w.file(&a.claim_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    let File::Source(source) =
        read_file(&a.source_path, w.file(&a.source_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a source");
    };
    let links = claim
        .sources
        .iter()
        .map(|l| LinkEdit {
            source: l.source.clone(),
            relation: l.relation,
            bearing: l.bearing,
        })
        .collect();
    let edit = Curation {
        status: Standing::Unknown,
        links,
        removed: Vec::new(),
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: String::new(),
            availability: source.availability,
            work: None,
            checked: None,
            retire: source_retire,
        }],
        confirm: false,
        retire,
        proposal: None,
    };
    let files = curate(&claim, std::slice::from_ref(&source), None, &edit)?;
    let refs: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), Some(t.as_str())))
        .collect();
    Ok(w.propose(summary, &refs))
}

#[test]
fn criterion_11_a_claim_is_retired_by_curation_with_a_reason_leaves_the_paragraph_and_is_taken_back()
 {
    use inklkunn_core::knowledge::compose::{Mistake, Retire};
    use inklkunn_core::knowledge::index::RETIRED;
    use inklkunn_core::knowledge::{File, Lifecycle, read_file};
    let Some(w) = World::new("retire-claim", PlatformLimits::default(), now) else {
        return;
    };
    let a = attach_a_source(&w);
    let id = a.claim.to_string();
    let offered = |w: &World| {
        w.reads.forget_wiki(&w.wiki);
        let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
        index.claims_on_page("p-home").count()
    };
    assert_eq!(offered(&w), 1, "a claim is offered beside its paragraph");

    // A retirement says why: the core writes nothing without a reason, and nothing hand-written without one is accepted.
    assert_eq!(
        curate_with(
            &w,
            &a,
            "Retire it",
            Retire::Retire {
                reason: "  ".to_owned()
            },
            Retire::Leave
        )
        .unwrap_err(),
        Mistake::ReasonMissing
    );
    let refused_at_submit = |summary: &str, files: &[(&str, Option<&str>)]| {
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
        let draft = w
            .proposals
            .create(&w.author.principal, &w.wiki, None, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        w.proposals
            .submit(&w.author.principal, &w.wiki, &draft.id)
            .expect_err("a proposal the acceptance would refuse is not sent")
    };
    let File::Claim(claim) =
        read_file(&a.claim_path, w.file(&a.claim_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    let mut bare = claim.clone();
    bare.lifecycle = Lifecycle::Retired;
    assert!(matches!(
        refused_at_submit(
            "Retire it, with no reason",
            &[(a.claim_path.as_str(), Some(&bare.to_yaml()))]
        ),
        inklkunn_core::ProposalError::Invalid(Invalid::KnowledgeRetiredUnexplained)
    ));
    // Nor beside a page edit: retiring is curation's, so a proposal that is not a curation proposal is refused for it.
    let mut with_reason = claim.clone();
    with_reason.lifecycle = Lifecycle::Retired;
    with_reason.retired_reason = Some("It was withdrawn.".to_owned());
    assert!(matches!(
        refused_at_submit(
            "Retire it, and reword the front page",
            &[
                (a.claim_path.as_str(), Some(&with_reason.to_yaml())),
                ("pages/Home.md", Some(&HOME.replace("Two.", "Dos."))),
            ]
        ),
        inklkunn_core::ProposalError::Invalid(Invalid::KnowledgeCurated)
    ));
    assert_eq!(offered(&w), 1);

    // The curation proposal retires it, with its reason; it is accepted like any other.
    let retire = curate_with(
        &w,
        &a,
        "Retire the claim: the survey was withdrawn",
        Retire::Retire {
            reason: "The survey was withdrawn.".to_owned(),
        },
        Retire::Leave,
    )
    .unwrap();
    signed(w.accept(&w.maintainer, &retire.id));
    let File::Claim(retired) =
        read_file(&a.claim_path, w.file(&a.claim_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    assert_eq!(retired.lifecycle, Lifecycle::Retired);
    assert_eq!(
        retired.retired_reason.as_deref(),
        Some("The survey was withdrawn.")
    );
    w.reads.forget_wiki(&w.wiki);
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    let row = index.claim(&id).unwrap();
    assert_eq!(row.lifecycle, RETIRED);
    assert_eq!(
        row.retired_reason.as_deref(),
        Some("The survey was withdrawn.")
    );
    // It left the paragraph's link and kept its anchors, its page and its source.
    assert_eq!(index.claims_on_page("p-home").count(), 0);
    assert_eq!(index.anchors_of(&id).count(), 1);
    assert_eq!(index.links_of(&id).count(), 1);

    // Taking the retirement back is a curation proposal too: the claim is put forward again, and offered again.
    let back = curate_with(
        &w,
        &a,
        "Take the retirement back",
        Retire::Reinstate,
        Retire::Leave,
    )
    .unwrap();
    signed(w.accept(&w.maintainer, &back.id));
    let File::Claim(again) =
        read_file(&a.claim_path, w.file(&a.claim_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a claim");
    };
    assert_eq!(again.lifecycle, Lifecycle::Proposed);
    assert_eq!(again.retired_reason, None);
    assert_eq!(offered(&w), 1);
}

#[test]
fn criterion_11_a_source_is_retired_stays_on_the_claims_that_cite_it_and_leaves_the_sources_to_review()
 {
    use inklkunn_core::knowledge::compose::Retire;
    use inklkunn_core::knowledge::freshness::sources_to_review;
    use inklkunn_core::knowledge::index::RETIRED;
    use inklkunn_core::knowledge::{File, SourceLife, read_file};
    let Some(w) = World::new("retire-source", PlatformLimits::default(), now) else {
        return;
    };
    let a = attach_a_source(&w);
    let listed = |w: &World| {
        w.reads.forget_wiki(&w.wiki);
        let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
        sources_to_review(&index, now(), 1096)
            .iter()
            .any(|s| s.source.id == a.source.to_string())
    };
    // Never reviewed: it is listed.
    assert!(listed(&w), "a source nobody checked is to be reviewed");

    let retire = curate_with(
        &w,
        &a,
        "Retire the source: a better edition is cited",
        Retire::Leave,
        Retire::Retire {
            reason: "A better edition is cited.".to_owned(),
        },
    )
    .unwrap();
    signed(w.accept(&w.maintainer, &retire.id));
    let File::Source(retired) =
        read_file(&a.source_path, w.file(&a.source_path).unwrap().as_bytes()).unwrap()
    else {
        panic!("a source");
    };
    assert_eq!(retired.lifecycle, SourceLife::Retired);
    assert_eq!(
        retired.retired_reason.as_deref(),
        Some("A better edition is cited.")
    );
    assert!(!listed(&w), "a retired source is not listed to be reviewed");
    // It keeps its page's row and its link on the claim that cites it, marked.
    let index = w.reads.knowledge_index(&w.wiki, None).unwrap();
    let row = index.source(&a.source.to_string()).unwrap();
    assert_eq!(row.lifecycle, RETIRED);
    assert_eq!(
        row.retired_reason.as_deref(),
        Some("A better edition is cited.")
    );
    assert_eq!(index.links_of(&a.claim.to_string()).count(), 1);

    // Taking it back: the source is active again, and is listed again.
    let back = curate_with(
        &w,
        &a,
        "Take the retirement back",
        Retire::Leave,
        Retire::Reinstate,
    )
    .unwrap();
    signed(w.accept(&w.maintainer, &back.id));
    assert!(listed(&w));
}

#[test]
fn criterion_11_a_retired_claim_and_a_retired_source_stay_in_the_export() {
    use inklkunn_core::knowledge::compose::Retire;
    let Some(w) = World::new("retire-export", PlatformLimits::default(), now) else {
        return;
    };
    let a = attach_a_source(&w);
    let retire = curate_with(
        &w,
        &a,
        "Retire both",
        Retire::Retire {
            reason: "The survey was withdrawn.".to_owned(),
        },
        Retire::Retire {
            reason: "A second edition replaced it.".to_owned(),
        },
    )
    .unwrap();
    signed(w.accept(&w.maintainer, &retire.id));
    let dest = w._tmp.0.join("bundle");
    inklkunn_core::export::export_bundle(&*w.storage, &w.layout, &w.wiki, &dest, "0.0.0", now())
        .unwrap_or_else(|e| panic!("{e}"));
    // Both files are in the bundle as the repository has them: a retirement removes nothing.
    for (path, words) in [
        (&a.claim_path, "The survey was withdrawn."),
        (&a.source_path, "A second edition replaced it."),
    ] {
        let text = std::fs::read_to_string(dest.join(path))
            .unwrap_or_else(|e| panic!("the bundle has no {path}: {e}"));
        assert!(text.contains("lifecycle: retired"), "{path}: {text}");
        assert!(text.contains(words), "{path}: {text}");
        assert_eq!(
            Some(text),
            w.file(path),
            "{path} is as the repository has it"
        );
    }
}
