//! Views against a real prikk and the real signer (RFC 005 part 2 D-9; handoff 005 K-T8): a wiki created and written
//! by the real signer, a view created through the service's own orchestration (the signer's second operation, bound to
//! the recorded act), a change accepted **into the view** through the normal proposal path (a page's wording, a
//! claim's status and a second source), and the two views compared by the real read path (a real whole-tree `diff`
//! between two tips of two views). Self-skips with a visible line when prikk is not on `PATH`.
//!
//! Every sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::import::{plan_folder, run_import, wiki_state};
use inklkunn_core::knowledge::compare::Presence;
use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Expression, Lifecycle, Link, Source, SourceId,
    SourceKind, SourceLink, Standing, StatementKind, block_digest, claim_path, source_path,
};
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::read::{DiffStatus, ViewPageOutcome};
use inklkunn_core::reconcile::reconcile;
use inklkunn_core::views::{ViewKind, list as list_views};
use inklkunn_core::{
    AcceptanceParts, Accepted, Authorizer, IdentityAuthority, KeyAuthority, LeaseConfig, Principal,
    ProposalAcceptor, ReadConfig, ReadService, Signer, SignerConfig, StoreActs, StoreAuthz,
    StoreLedger, SystemActs, ViewCreationAct, ViewFailure, WikiLeases, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    Account, AccountId, AccountStore, ActStore, DisplayName, FileChange, MemoryAudit, MemoryStore,
    NewWiki, ProposalRecord, Role, RoleGrant, RoleStore, Slug, SystemActId, ViewActOutcome,
};

const MAIN: &str = "heads/main";

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
    let p = base.join(format!("inklkunn-views-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

/// The page every test starts with: one paragraph that holds a claim (`^b1`), and one that does not.
fn home(first: &str) -> String {
    format!("---\ntitle: Home\nid: p-home\n---\n\n{first} ^b1\n\nPlain words, no claim.\n")
}

const SAID: &str = "The bridge opened in 1950.";
const SAID_ELSEWHERE: &str = "The bridge opened in 1951.";

fn claim_id() -> ClaimId {
    ClaimId::parse("01930000-0000-7000-8000-000000000001").expect("a claim id")
}

fn source_id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

fn digest_of(first: &str) -> inklkunn_core::knowledge::Digest {
    let text = format!("{first} ^b1");
    let blocks = inklkunn_markup::blocks(&text);
    block_digest(blocks.first().expect("a block"))
}

/// The claim, said in `Home`'s first paragraph as `first`, with the standing given and these sources.
fn claim_file(first: &str, standing: Standing, sources: &[u8]) -> String {
    let mut c = Claim::new(claim_id());
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new("p-home", "b1").expect("an expression")];
    c.community_status = CommunityStatus {
        status: standing,
        proposal: None,
        judged_on: Some(digest_of(first)),
    };
    c.sources = sources
        .iter()
        .map(|n| SourceLink {
            source: source_id(*n),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: digest_of(first),
        })
        .collect();
    c.to_yaml()
}

fn source_file(n: u8) -> String {
    let mut s = Source::new(source_id(n));
    s.kind = SourceKind::Primary;
    s.to_yaml()
}

struct World {
    _tmp: Dir,
    _shm: Dir,
    store: Arc<MemoryStore>,
    storage: Arc<PrikkCli>,
    reads: Arc<ReadService>,
    proposals: Arc<ProposalService>,
    acceptor: Arc<ProposalAcceptor>,
    signer: Arc<Signer>,
    acts: SystemActs,
    layout: inklkunn_store::RootLayout,
    wiki: inklkunn_store::WikiRecord,
    repo: PathBuf,
    maintainer: Principal,
    reviewer: Principal,
    reviewer_id: AccountId,
    contributor: Principal,
}

impl World {
    /// A wiki whose history is: its creation, and one import of `Home` and `Other`.
    fn new(tag: &str) -> Option<World> {
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
        let secret = tmp.0.join("secret");
        std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
        let layout = inklkunn_store::RootLayout::new(tmp.0.join("root"));
        let store = Arc::new(MemoryStore::new());
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
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
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
            "views-test",
            LeaseConfig::default(),
        );
        let audit = Arc::new(MemoryAudit::new());
        let authorizer = || {
            Authorizer::new(
                Arc::new(StoreAuthz::new(store.clone(), store.clone())),
                audit.clone(),
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
            .with_audit(audit.clone()),
        );
        let acts = SystemActs::new(
            authorizer(),
            store.clone(),
            store.clone(),
            storage.clone(),
            layout.clone(),
            signer.clone(),
            leases,
        )
        .with_reads(reads.clone())
        .with_audit(audit.clone());
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
        let reviewer_id = person("Rae Reviewer", Role::Reviewer);
        let contributor_id = person("Noor Contributor", Role::Contributor);
        let world = World {
            maintainer: Principal::session(authority(), maintainer_id),
            reviewer: Principal::session(authority(), reviewer_id.clone()),
            reviewer_id,
            contributor: Principal::session(authority(), contributor_id),
            _tmp: tmp,
            _shm: shm,
            store,
            storage,
            reads,
            proposals,
            acceptor,
            signer,
            acts,
            layout,
            repo: created.repo,
            wiki,
        };
        world.import(&[
            ("pages/Home.md", &home(SAID)),
            (
                "pages/Other.md",
                "---\ntitle: Other\nid: p-other\n---\n\nHello.\n",
            ),
        ]);
        Some(world)
    }

    /// `import-folder` of these pages, through the service's own path.
    fn import(&self, pages: &[(&str, &str)]) {
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
        run_import(&self.acts, &self.wiki, &plan).unwrap_or_else(|e| panic!("{e}"));
    }

    /// A proposal by the contributor, in `view` (the default view when `None`), taken into the history by the
    /// maintainer.
    fn change(&self, view: Option<&str>, summary: &str, files: &[(String, String)]) -> Accepted {
        let edit = ProposalEdit {
            summary: summary.to_owned(),
            files: files
                .iter()
                .map(|(path, text)| FileChange {
                    path: path.clone(),
                    text: Some(text.clone()),
                })
                .collect(),
            ..ProposalEdit::default()
        };
        let draft = self
            .proposals
            .create(&self.contributor, &self.wiki, view, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        let held: ProposalRecord = self
            .proposals
            .submit(&self.contributor, &self.wiki, &draft.id)
            .unwrap_or_else(|e| panic!("submit: {e}"));
        self.acceptor
            .accept(&self.maintainer, &self.wiki, &held.id, 1)
            .unwrap_or_else(|e| panic!("accept: {e}"))
    }

    fn tip(&self, view: &str) -> String {
        self.storage
            .history(&self.repo, &Point::View(view.to_owned()), Some(1))
            .unwrap()
            .blocks
            .first()
            .unwrap()
            .block_id
            .clone()
    }

    fn views_in_prikk(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .storage
            .views(&self.repo)
            .unwrap()
            .views
            .into_iter()
            .filter(|v| !v.closed)
            .map(|v| v.name)
            .collect();
        names.sort();
        names
    }
}

fn pair(path: &str, text: String) -> (String, String) {
    (path.to_owned(), text)
}

#[test]
fn a_view_is_created_changed_through_a_proposal_and_compared_against_a_real_prikk() {
    let Some(w) = World::new("views") else {
        return;
    };
    // The default view holds a claim, accepted, said in Home, with one source.
    w.change(
        None,
        "Add the claim and its source",
        &[
            pair(
                &claim_path(&claim_id()),
                claim_file(SAID, Standing::Accepted, &[1]),
            ),
            pair(&source_path(&source_id(1)), source_file(1)),
            pair(&source_path(&source_id(2)), source_file(2)),
        ],
    );
    let main_tip = w.tip(MAIN);

    // A reviewer creates an alternative view: the signer's second operation, bound to the act that was recorded.
    let created = w
        .acceptor
        .create_view(
            &w.reviewer,
            &w.wiki,
            "reading",
            None,
            ViewKind::Interpretation,
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (created.name.as_str(), created.forked_from.as_str()),
        ("reading", "main")
    );
    assert_eq!(w.views_in_prikk(), ["heads/main", "heads/reading"]);
    assert_eq!(
        w.tip("heads/reading"),
        main_tip,
        "a new view starts where its source is"
    );
    let acts = w.store.view_acts(&w.wiki.id).unwrap();
    let entry = acts.first().unwrap();
    assert_eq!(entry.act.id, created.act);
    assert_eq!(entry.act.created_by, w.reviewer_id);
    assert_eq!(entry.act.from_block, main_tip);
    assert_eq!(
        entry.end.as_ref().map(|e| &e.outcome),
        Some(&ViewActOutcome::Created)
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    // The service lists it: the default view first, then the new one with its kind and where it was forked from.
    let list = list_views(&w.reads, &*w.store, &w.wiki).unwrap();
    let names: Vec<&str> = list.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["main", "reading"]);
    let origin = list.view("reading").unwrap().origin.clone().unwrap();
    assert_eq!(
        (origin.kind, origin.forked_from.as_str()),
        (ViewKind::Interpretation, "main")
    );

    // At the same change the two views have nothing to say of each other.
    let same = w
        .reads
        .compare_views(&w.wiki, None, Some("heads/reading"))
        .unwrap();
    assert!(same.same && same.pages.is_empty() && same.claims.is_empty());

    // A change is accepted **into the view**, by the normal proposal path: the wording, the status and a second
    // source of the claim.
    let signed = w.change(
        Some("heads/reading"),
        "Read the year differently",
        &[
            pair("pages/Home.md", home(SAID_ELSEWHERE)),
            pair(
                &claim_path(&claim_id()),
                claim_file(SAID_ELSEWHERE, Standing::Contested, &[1, 2]),
            ),
        ],
    );
    assert!(matches!(signed, Accepted::Signed(_)), "{signed:?}");
    assert_eq!(w.tip(MAIN), main_tip, "the default view did not move");
    assert_ne!(w.tip("heads/reading"), main_tip);

    // The real whole-tree diff between the two tips, and the two indexes, say where they differ.
    let got = w
        .reads
        .compare_views(&w.wiki, None, Some("heads/reading"))
        .unwrap();
    assert!(!got.same);
    let pages: Vec<(&str, DiffStatus, u64, u64)> = got
        .pages
        .iter()
        .map(|p| (p.title.as_str(), p.status, p.added, p.removed))
        .collect();
    assert_eq!(pages, [("Home", DiffStatus::Modified, 1, 1)]);
    assert_eq!(got.claims.len(), 1);
    let first = got.claims.first().unwrap();
    assert_eq!(first.difference.presence, Presence::Both);
    assert!(first.difference.wording && first.difference.sources);
    assert_eq!(
        first.difference.status,
        Some(("accepted".to_owned(), "contested".to_owned()))
    );
    assert_eq!(first.said_in.as_deref(), Some("Home"));
    // One page's own diff, with prikk's lines and the word marks.
    let ViewPageOutcome::Found(page) = w
        .reads
        .compare_page(&w.wiki, None, Some("heads/reading"), "Home")
        .unwrap()
    else {
        panic!("Home is in both views")
    };
    assert_eq!(page.shape.status, DiffStatus::Modified);
    assert_eq!((page.shape.added, page.shape.removed), (1, 1));
    assert!(!page.shape.hunks.is_empty() && !page.shape.truncated);
    // A page neither view has.
    assert_eq!(
        w.reads
            .compare_page(&w.wiki, None, Some("heads/reading"), "Nowhere")
            .unwrap(),
        ViewPageOutcome::Absent
    );
    // The claim, read in each view, is as that view has it; each knows the other differs.
    let there = w
        .reads
        .knowledge_index(&w.wiki, Some("heads/reading"))
        .unwrap();
    let here = w.reads.knowledge_index(&w.wiki, None).unwrap();
    assert_eq!(
        here.claim(&claim_id().to_string()).unwrap().standing,
        "accepted"
    );
    assert_eq!(
        there.claim(&claim_id().to_string()).unwrap().standing,
        "contested"
    );

    // Nothing the views did put the wiki out of step with its records.
    let records = StoreActs::new(w.store.clone(), w.store.clone());
    assert!(
        reconcile(&w.wiki.id, &w.repo, &*w.storage, &records)
            .unwrap()
            .is_clean()
    );
    assert!(w.storage.verify(&w.repo).unwrap().verdict.ok);
}

#[test]
fn who_may_create_a_view_what_it_may_be_called_and_what_the_signer_will_not_make_for_a_caller() {
    let Some(w) = World::new("guard") else {
        return;
    };
    let make = |who: &Principal, name: &str, from: Option<&str>| {
        w.acceptor
            .create_view(who, &w.wiki, name, from, ViewKind::Experimental)
    };
    // A contributor and a visitor who is nobody are refused before anything is recorded or asked.
    assert_eq!(
        make(&w.contributor, "x", None),
        Err(ViewFailure::NotAllowed)
    );
    assert_eq!(
        make(&Principal::Anonymous, "x", None),
        Err(ViewFailure::NotAllowed)
    );
    assert!(w.store.view_acts(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.views_in_prikk(), [MAIN]);
    // A maintainer may as well; the name must be a new view's, free, and fork from a view that is there.
    assert_eq!(
        make(&w.maintainer, "Bad Name", None),
        Err(ViewFailure::InvalidName(
            inklkunn_core::views::NameFault::Start
        ))
    );
    assert_eq!(
        make(&w.maintainer, "main", None),
        Err(ViewFailure::NameTaken)
    );
    assert_eq!(
        make(&w.maintainer, "fine", Some("nowhere")),
        Err(ViewFailure::NoSuchSource)
    );
    assert!(w.store.view_acts(&w.wiki.id).unwrap().is_empty());
    assert!(make(&w.maintainer, "fine", None).is_ok());
    assert_eq!(make(&w.reviewer, "fine", None), Err(ViewFailure::NameTaken));
    assert_eq!(w.views_in_prikk(), ["heads/fine", MAIN]);

    // The signer itself will not branch for a caller that holds no record: an act nobody recorded, an id that is not
    // one, and a recorded act asked for another name or another source.
    use inklkunn_core::Accept as _;
    let wiki = &w.wiki.id;
    for (name, from, act) in [
        (
            "heads/ghost",
            "heads/main",
            ViewCreationAct::new(inklkunn_store::ViewActId::new().as_str()),
        ),
        (
            "heads/ghost",
            "heads/main",
            ViewCreationAct::new("not-an-id"),
        ),
    ] {
        assert!(
            w.signer.create_view(wiki, name, from, &act).is_err(),
            "{name} {from}"
        );
    }
    let recorded = w
        .store
        .view_acts(wiki)
        .unwrap()
        .first()
        .unwrap()
        .act
        .id
        .clone();
    let act = ViewCreationAct::new(recorded.as_str());
    // It ended, so it is no longer pending: the signer refuses it even for the very name it was recorded for.
    assert!(
        w.signer
            .create_view(wiki, "heads/fine", "heads/main", &act)
            .is_err()
    );
    assert!(
        w.signer
            .create_view(wiki, "heads/other", "heads/main", &act)
            .is_err()
    );
    assert_eq!(
        w.views_in_prikk(),
        ["heads/fine", MAIN],
        "no view was made for a caller"
    );
}
