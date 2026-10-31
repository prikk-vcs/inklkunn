//! Recovery against a real repository (RFC 004 D-7; handoff 004 T5): acceptances and imports that ended after
//! `commit`, wikis whose queue was torn or whose locks were left, completed or discarded or cleared by the service's
//! own procedure, under the lease, with the plan recorded and every step in the audit trail. Self-skips with a visible
//! line when prikk is not on `PATH`. The real kills (a child process, `SIGKILL`) are `inklkunn-signer`'s
//! `tests/acceptance_kill.rs`; the core crate may not spawn a process.
//!
//! A stand-in `prikk` passes every call to the real one unless a control file names the verb: `fail-<verb>` makes it
//! fail, `say-<verb>` makes it print a canned answer.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use inklkunn_core::import::{plan_folder, run_import, wiki_state};
use inklkunn_core::message::{Actor, BlockId, Message, Summary};
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::reconcile::{Report, reconcile};
use inklkunn_core::recovery::{Step, Stop};
use inklkunn_core::{
    AcceptFailure, AcceptanceLedger, AcceptanceParts, AcceptanceToken, Authorizer, ExpectedMessage,
    IdentityAuthority, KeyAuthority, LeaseConfig, LedgerError, PreparedChange, PreparedFile,
    Principal, ProposalAcceptor, Purpose, ReadConfig, ReadService, Recover, Recovery,
    RecoveryError, RecoveryMode, RecoveryOutcome, RecoveryParts, Signer, SignerConfig, StoreActs,
    StoreAuthz, StoreLedger, SystemActFailure, SystemActs, ViewCreationAct, ViewCreationFacts,
    WikiLeases, Witnesses, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, Account, AccountId, AccountStore, ActStore, AuditLog,
    DisplayName, FileChange, IntentEnd, NewWiki, ProposalRecord, ProposalState, RecoveryEnding,
    RecoveryFlag, RecoveryReason, Role, RoleGrant, RoleStore, Slug, SqliteStore, SystemActEnd,
    SystemActId, SystemActKind, SystemActRecord,
};

const MAIN: &str = "heads/main";

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
}

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
    let p = base.join(format!("inklkunn-rec-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
echo "$verb" >> "$CTL/calls.log"
if [ -e "$CTL/fail-$verb" ]; then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
if [ -e "$CTL/say-$verb" ]; then cat "$CTL/say-$verb"; exit 0; fi
exec "{real}" "$@"
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

/// What a planted change is accepted under: a ledger that answers for it and for nothing else.
#[derive(Default)]
struct PlantLedger {
    pending: Mutex<HashMap<String, ExpectedMessage>>,
}

impl AcceptanceLedger for PlantLedger {
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(self.pending.lock().unwrap().get(token.as_str()).cloned())
    }

    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

struct World {
    _tmp: Dir,
    _shm: Dir,
    ctl: PathBuf,
    store: Arc<SqliteStore>,
    storage: Arc<PrikkCli>,
    leases: Arc<WikiLeases>,
    reads: Arc<ReadService>,
    proposals: Arc<ProposalService>,
    acceptor: Arc<ProposalAcceptor>,
    acts: SystemActs,
    signer: Arc<Signer>,
    planter: Signer,
    planted: Arc<PlantLedger>,
    recovery: Recovery,
    layout: inklkunn_store::RootLayout,
    wiki: inklkunn_store::WikiRecord,
    repo: PathBuf,
    maintainer: Principal,
    maintainer_id: AccountId,
    contributor: Principal,
    contributor_id: AccountId,
}

impl World {
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
        let config = |ledger: Arc<dyn AcceptanceLedger>, create: bool| {
            let mut cfg = SignerConfig::new(
                &standin,
                Sandbox::Unconfined,
                layout.clone(),
                tmp.0.join("keys"),
                &secret,
                &shm.0,
            );
            cfg.cheap_derivation = true;
            if create {
                Signer::open_creating_store(cfg, ledger)
            } else {
                Signer::open(cfg, ledger)
            }
            .expect("signer")
            .with_leases(&write_leases)
        };
        let signer = Arc::new(config(
            Arc::new(StoreLedger::new(store.clone(), store.clone())),
            true,
        ));
        let planted = Arc::new(PlantLedger::default());
        let planter = config(planted.clone(), false);
        let storage = Arc::new(
            PrikkCli::open(
                &standin,
                Sandbox::Unconfined,
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
            )
            .expect("prikk at the floor"),
        );
        let created = create_wiki(
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
        let message = inklkunn_core::creation_message(&wiki);
        let act = SystemActRecord {
            id: SystemActId::new(),
            wiki: wiki.id.clone(),
            view: wiki.default_view.clone(),
            kind: SystemActKind::System,
            base_block: created.first_block_id.clone(),
            message_digest: inklkunn_core::acceptance::message_digest(&message),
            message,
            digest: "0".repeat(64),
            bytes: 0,
            created_at: 1,
        };
        store.begin_system_act(&act).unwrap();
        store
            .finish_system_act(&SystemActEnd {
                act: act.id.clone(),
                outcome: AcceptanceOutcome::Sealed {
                    block_id: created.first_block_id.clone(),
                    patch_id: "0".repeat(64),
                },
                at: 2,
            })
            .unwrap();
        let leases = WikiLeases::new(write_leases.clone(), store.clone(), "recovery-test", lease);
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
            .with_audit(store.clone()),
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
        let recovery = Recovery::new(RecoveryParts {
            acceptances: store.clone(),
            acts: store.clone(),
            proposals: store.clone(),
            storage: storage.clone(),
            witnesses: Arc::new(Witnesses::new(
                &standin,
                Sandbox::Unconfined,
                layout.clone(),
            )),
            signer: signer.clone() as Arc<dyn Recover>,
            leases: leases.clone(),
            layout: layout.clone(),
            audit: store.clone(),
        })
        .with_reads(reads.clone());
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
            maintainer: Principal::session(authority(), maintainer_id.clone()),
            contributor: Principal::session(authority(), contributor_id.clone()),
            _tmp: tmp,
            _shm: shm,
            ctl,
            store,
            storage,
            leases,
            reads,
            proposals,
            acceptor,
            acts,
            signer,
            planter,
            planted,
            recovery,
            layout,
            repo: created.repo,
            wiki,
            maintainer_id,
            contributor_id,
        };
        world.import(&[("pages/Home.md", HOME), ("pages/Other.md", OTHER)]);
        Some(world)
    }

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

    fn accept(&self, p: &ProposalRecord) -> Result<inklkunn_core::Accepted, AcceptFailure> {
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

    /// An acceptance that commits and fails to seal: the wiki is flagged, a patch is queued.
    fn queued_acceptance(&self, p: &ProposalRecord) {
        self.control("fail-seal", "");
        let err = self.accept(p).unwrap_err();
        self.uncontrol("fail-seal");
        assert!(
            matches!(
                &err,
                AcceptFailure::Incomplete {
                    reason: RecoveryReason::Queued,
                    flagged: true,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    fn queue_count(&self) -> u64 {
        self.storage.status(&self.repo).unwrap().queue.count
    }

    fn audit(&self) -> Vec<(String, String, String)> {
        self.store
            .read(0, 10_000)
            .unwrap()
            .into_iter()
            .filter(|r| r.event.principal == "system:recovery")
            .map(|r| {
                (
                    r.event.action,
                    r.event.outcome.as_str().to_owned(),
                    r.event.detail,
                )
            })
            .collect()
    }

    fn recover(&self) -> inklkunn_core::RecoveryReport {
        self.recovery
            .recover(&self.wiki, RecoveryMode::Run)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn verifies(&self) -> bool {
        self.storage.verify(&self.repo).unwrap().verdict.ok
    }

    /// Plant a change that the service's records know nothing of, queued and not sealed: what a stranger's
    /// `commit`, or a record that was lost, leaves.
    fn plant_queued_patch(&self, page: &str, text: &str) {
        let proposal = inklkunn_store::ProposalId::new();
        let base = BlockId::new(&self.tip()).unwrap();
        let who = |n: &str| Actor::Account {
            id: AccountId::new(),
            name: DisplayName::new(n).unwrap(),
        };
        let (a, b) = (who("Stranger One"), who("Stranger Two"));
        let message = Message::proposal(
            Summary::from_text("Planted").unwrap(),
            proposal.clone(),
            a.clone(),
            b.clone(),
            base.clone(),
        )
        .unwrap();
        let change = PreparedChange {
            files: vec![PreparedFile {
                path: PathBuf::from(page),
                content: Some(text.as_bytes().to_vec()),
            }],
            renames: Vec::new(),
            message: message.to_text(),
        };
        let token = AcceptanceToken::new(format!("planted-{proposal}"));
        self.planted.pending.lock().unwrap().insert(
            token.as_str().to_owned(),
            ExpectedMessage {
                wiki: self.wiki.id.clone(),
                view: MAIN.to_owned(),
                kind: message.kind(),
                proposal: Some(proposal),
                proposed_by: a,
                accepted_by: b,
                base: Some(base),
                imported_from: None,
                pinned_tip: None,
                change_digest: change.digest(),
            },
        );
        self.control("fail-seal", "");
        let r = inklkunn_core::Accept::accept(&self.planter, &self.wiki.id, MAIN, &token, &change);
        self.uncontrol("fail-seal");
        assert!(
            matches!(r, Err(inklkunn_core::AcceptError::Queued { .. })),
            "{r:?}"
        );
        assert_eq!(self.queue_count(), 1);
    }

    fn flag(&self, reason: RecoveryReason) {
        self.store
            .set_recovery_flag(&RecoveryFlag {
                wiki: self.wiki.id.clone(),
                reason,
                act: None,
                since: 5,
            })
            .unwrap();
    }

    fn wal(&self) -> PathBuf {
        self.repo.join(".prikk/active/default/queue.wal")
    }
}

fn plant_lock(repo: &Path, relative: &str, pid: u32, kind: &str) -> String {
    let path = repo.join(".prikk").join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("pid={pid}\nkind={kind}\nnote=planted\n")).unwrap();
    std::fs::canonicalize(&path).unwrap().display().to_string()
}

fn quick(wait_ms: u64) -> LeaseConfig {
    LeaseConfig {
        max_wait: Duration::from_millis(wait_ms),
        poll: Duration::from_millis(10),
        ..LeaseConfig::default()
    }
}

fn step_names(r: &inklkunn_core::RecoveryReport) -> Vec<&'static str> {
    r.plan.steps.iter().map(Step::name).collect()
}

#[test]
fn a_queued_patch_is_completed_with_seal_alone_and_the_records_agree_with_the_history() {
    let Some(w) = World::new("complete", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let q = w.propose(
        "Another",
        &[(
            "pages/Another.md",
            Some("---\ntitle: Another\nid: pn\n---\n\na\n"),
        )],
    );
    let before = w.history();
    w.queued_acceptance(&p);
    assert_eq!(w.queue_count(), 1);
    assert_eq!(w.history(), before, "committed and not sealed");
    let commits = std::fs::read_to_string(w.ctl.join("calls.log"))
        .unwrap()
        .lines()
        .filter(|v| *v == "commit")
        .count();

    // The plan, shown: nothing is done, nothing is recorded, the flag stays.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.outcome, RecoveryOutcome::Planned);
    assert!(!shown.under_lease && shown.act.is_none());
    assert_eq!(
        step_names(&shown),
        [
            "seal-queued",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert_eq!(w.queue_count(), 1);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert!(w.store.recovery_acts(&w.wiki.id).unwrap().is_empty());
    assert!(w.audit().is_empty());

    // The run.
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert!(r.under_lease);
    assert_eq!(r.plan.stop, None);
    assert_eq!(
        r.done.iter().map(|d| d.step).collect::<Vec<_>>(),
        [
            "seal-queued",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    // Exactly one new block, carrying the message that was queued; nothing was committed again.
    let after = w.history();
    assert_eq!(after.len(), before.len() + 1);
    let newest = after.first().unwrap();
    assert_eq!(newest.patch_messages.len(), 1);
    let message = Message::parse(&newest.patch_messages.first().unwrap().message).unwrap();
    assert_eq!(message.proposal_id(), Some(&p.id));
    let commits_after = std::fs::read_to_string(w.ctl.join("calls.log"))
        .unwrap()
        .lines()
        .filter(|v| *v == "commit")
        .count();
    assert_eq!(commits_after, commits, "recovery never commits");
    assert!(w.page("pages/Home.md").contains("Uno."));
    // The records: the acceptance sealed with that block, its intent completed, the proposal accepted.
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(AcceptanceOutcome::Sealed { block_id, .. }) if block_id == newest.block_id
    ));
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    let intent = w.store.intents(&w.wiki.id).unwrap();
    assert_eq!(
        intent.last().unwrap().completion.as_ref().map(|c| c.end),
        Some(IntentEnd::Completed)
    );
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.leases.recorded_holder(&w.wiki.id).unwrap(), None);
    assert!(w.verifies());
    assert!(w.reconciled().is_clean());
    // The plan is recorded, with its end, and says what it did.
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    assert_eq!(acts.len(), 1);
    let record = acts.first().unwrap();
    assert_eq!(
        record.end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Recovered)
    );
    assert!(
        record.act.plan.contains("\"step\":\"seal-queued\""),
        "{}",
        record.act.plan
    );
    assert_eq!(Some(&record.act.id), r.act.as_ref());
    // Every step is an audit record before it is taken, then the end.
    let audit = w.audit();
    let actions: Vec<&str> = audit.iter().map(|(a, _, _)| a.as_str()).collect();
    assert_eq!(actions.first(), Some(&"recovery-begin"));
    assert_eq!(actions.iter().filter(|a| **a == "recovery-step").count(), 5);
    assert_eq!(
        audit
            .last()
            .map(|(a, o, d)| (a.as_str(), o.as_str(), d.as_str())),
        Some(("recovery", "allowed", "recovered"))
    );
    assert!(
        audit
            .iter()
            .any(|(a, _, d)| a == "recovery-plan" && d == "5 step(s)")
    );
    // The wiki takes writes again, and the next acceptance is an ordinary one.
    let done = w.accept(&q).expect("writes are accepted again");
    assert!(matches!(done, inklkunn_core::Accepted::Signed(_)));
    assert_eq!(w.history().len(), before.len() + 2);
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_wiki_under_recovery_serves_reads_and_refuses_writes_with_the_word_until_recovery_clears_it() {
    let Some(w) = World::new("word", LeaseConfig::default()) else {
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
    let page_before = w.page("pages/Home.md");
    w.queued_acceptance(&p);
    // Reads serve, from the history that is there.
    let home = w.reads.raw(&w.wiki, None, "Home").unwrap().expect("served");
    assert_eq!(String::from_utf8(home.bytes).unwrap(), page_before);
    // Writes are refused with the word, by both write paths.
    let err = w.accept(&q).unwrap_err();
    assert!(matches!(&err, AcceptFailure::NeedsRecovery(f) if f.reason == RecoveryReason::Queued));
    assert_eq!(err.word(), "needs-recovery");
    let folder = dir_in(&std::env::temp_dir(), "word-folder");
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
            inklkunn_core::import::ImportError::Act(SystemActFailure::NeedsRecovery(_))
        ),
        "{err}"
    );
    // Recovery clears it; the same writes are then accepted.
    assert_eq!(w.recover().outcome, RecoveryOutcome::Recovered);
    assert!(w.accept(&q).is_ok());
}

#[test]
fn an_import_that_ended_after_commit_is_completed_the_same_way() {
    let Some(w) = World::new("import", LeaseConfig::default()) else {
        return;
    };
    let before = w.history().len();
    let folder = dir_in(&std::env::temp_dir(), "import-folder");
    std::fs::create_dir_all(folder.0.join("pages")).unwrap();
    std::fs::write(
        folder.0.join("pages/New.md"),
        "---\ntitle: New\nid: pnew\n---\n\nnew\n",
    )
    .unwrap();
    let have = wiki_state(&*w.storage, &w.layout, &w.wiki).unwrap();
    let plan = plan_folder(&folder.0, &have, false).unwrap();
    w.control("fail-seal", "");
    let err = run_import(&w.acts, &w.wiki, &plan).unwrap_err();
    w.uncontrol("fail-seal");
    assert!(
        matches!(
            err,
            inklkunn_core::import::ImportError::Act(SystemActFailure::Incomplete { .. })
        ),
        "{err}"
    );
    assert_eq!(w.queue_count(), 1);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(w.history().len(), before + 1);
    assert!(w.page("pages/New.md").contains("new"));
    let acts = w.store.system_acts(&w.wiki.id).unwrap();
    assert!(
        acts.iter().all(|a| a.end.is_some()),
        "no system act is left open"
    );
    assert!(matches!(
        acts.last().unwrap().end.as_ref().map(|e| &e.outcome),
        Some(AcceptanceOutcome::Sealed { .. })
    ));
    assert!(w.verifies());
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_planted_queued_patch_that_matches_no_intent_is_reported_and_never_sealed_or_guessed_at() {
    let Some(w) = World::new("foreign", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    w.flag(RecoveryReason::Interrupted);
    let before = w.history();
    let r = w.recover();
    // prikk has no verb that removes a sound queued patch, and sealing it would put an unaccepted change in the
    // history: recovery stops, says so, and leaves the repository exactly as it found it.
    assert!(
        matches!(&r.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:#?}",
        r.lines()
    );
    assert_eq!(r.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(r.done.is_empty());
    assert_eq!(w.history(), before);
    assert_eq!(w.queue_count(), 1, "the patch is still queued");
    assert!(
        w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
        "the wiki stays flagged"
    );
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    assert_eq!(acts.len(), 1, "the plan is recorded, and ended as stopped");
    assert_eq!(
        nth(&acts, 0).end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Stopped)
    );
    assert!(nth(&acts, 0).act.plan.contains("foreign-queued-patch"));
    let audit = w.audit();
    assert!(
        audit.iter().any(|(a, o, d)| a == "recovery"
            && o == "failed"
            && d.contains("foreign-queued-patch"))
    );
    // The plan is on the report for the person who decides.
    assert!(
        r.lines()
            .iter()
            .any(|l| l.contains("STOP (foreign-queued-patch)"))
    );
}

#[test]
fn a_planted_torn_commit_that_matches_no_intent_is_discarded_and_reported() {
    let Some(w) = World::new("torn", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    w.flag(RecoveryReason::Interrupted);
    // The log is cut short: the commit was acknowledged and the log no longer holds it soundly.
    let len = std::fs::metadata(w.wal()).unwrap().len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(w.wal())
        .unwrap()
        .set_len(len - 25)
        .unwrap();
    assert_eq!(w.queue_count(), 0, "the torn commit is not in the queue");
    assert!(!w.verifies(), "and the repository does not verify");
    let before = w.history();
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        step_names(&r),
        [
            "discard-damaged",
            "restore-worktree",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(
        r.plan
            .notes
            .iter()
            .any(|n| n.contains("matches no recorded act")),
        "reported: {:?}",
        r.plan.notes
    );
    assert!(
        r.done
            .first()
            .is_some_and(|d| d.detail.contains("damaged commit discarded")),
        "{:?}",
        r.done
    );
    assert_eq!(w.history(), before, "the history is untouched");
    assert!(w.verifies());
    // The worktree is the tip again, so the next write is not refused for a dirty tree.
    assert_eq!(w.page("pages/Home.md"), HOME);
    assert_eq!(
        std::fs::read_to_string(w.repo.join("pages/Home.md")).unwrap(),
        HOME
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    let p = w.propose(
        "After",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    assert!(w.accept(&p).is_ok());
}

#[test]
fn an_acceptance_whose_commit_was_torn_is_discarded_and_ended_as_discarded_and_may_be_accepted_again()
 {
    let Some(w) = World::new("torn-act", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Torn",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.queued_acceptance(&p);
    let len = std::fs::metadata(w.wal()).unwrap().len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(w.wal())
        .unwrap()
        .set_len(len - 25)
        .unwrap();
    let before = w.history();
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        step_names(&r),
        [
            "discard-damaged",
            "restore-worktree",
            "end-failed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert_eq!(w.history(), before);
    // The acceptance is failed (`interrupted`) and its intent discarded; the proposal is still open.
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(AcceptanceOutcome::Failed { word }) if word == "interrupted"
    ));
    assert_eq!(
        w.store
            .intents(&w.wiki.id)
            .unwrap()
            .last()
            .unwrap()
            .completion
            .as_ref()
            .map(|c| c.end),
        Some(IntentEnd::Discarded)
    );
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    // A failed acceptance does not stand in the way of a retry.
    assert!(w.accept(&p).is_ok());
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_planted_stale_lock_is_cleared_and_reported_even_when_prikk_thinks_its_process_is_alive() {
    let Some(w) = World::new("locks", LeaseConfig::default()) else {
        return;
    };
    // PL-8: prikk's liveness check is meaningless inside the sandbox's PID namespace. A recorded pid that this
    // namespace shows as alive (this test's own) and one that is gone; the lease record shows no live holder.
    let me = std::process::id();
    let live = plant_lock(&w.repo, "active/default/active.lock", me, "active");
    let dead = plant_lock(
        &w.repo,
        "containers/objects.lock",
        2_147_483_000,
        "container:object-store",
    );
    w.flag(RecoveryReason::Interrupted);
    let witnesses = Witnesses::new("prikk", Sandbox::Unconfined, w.layout.clone());
    let listed = witnesses.locks(&w.wiki.id).unwrap();
    assert!(
        listed
            .iter()
            .any(|l| l.path == live && l.liveness.starts_with("appears running")),
        "the setup: prikk thinks the planted lock's process is running ({listed:?})"
    );
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(nth(&step_names(&shown), 0), &"clear-locks");
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(nth(&step_names(&r), 0), &"clear-locks");
    assert!(
        nth(&r.done, 0).detail.contains("2 lock(s) cleared"),
        "{:?}",
        r.done
    );
    assert!(witnesses.locks(&w.wiki.id).unwrap().is_empty());
    assert!(!Path::new(&live).exists() && !Path::new(&dead).exists());
    // The plan recorded what prikk said and that it was not consulted.
    let record = w.store.recovery_acts(&w.wiki.id).unwrap().remove(0);
    assert!(
        record.act.plan.contains("appears running"),
        "{}",
        record.act.plan
    );
    assert!(record.act.plan.contains(&format!("\"recorded_pid\":{me}")));
    assert!(r.plan.lines().iter().any(|l| l.contains("not consulted")));
    assert!(w.verifies());
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    let p = w.propose(
        "After",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    assert!(w.accept(&p).is_ok(), "prikk can write again");
}

#[test]
fn a_lock_of_a_kind_an_acceptance_does_not_leave_is_a_stop_and_nothing_is_cleared() {
    let Some(w) = World::new("unknown-lock", LeaseConfig::default()) else {
        return;
    };
    let active = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    let policy = plant_lock(
        &w.repo,
        "trust/policy.lock",
        2_147_483_000,
        "container:trust-policy",
    );
    w.flag(RecoveryReason::Interrupted);
    let r = w.recover();
    assert_eq!(r.plan.stop, Some(Stop::UnknownLock), "{:#?}", r.lines());
    assert!(matches!(r.outcome, RecoveryOutcome::Stopped { .. }));
    assert!(Path::new(&active).exists() && Path::new(&policy).exists());
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

#[test]
fn recovery_never_runs_without_the_lease_and_a_busy_wiki_is_told_so_with_nothing_done() {
    let Some(w) = World::new("lease", quick(100)) else {
        return;
    };
    let p = w.propose(
        "Half done",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    w.queued_acceptance(&p);
    let acts_before = w.store.recovery_acts(&w.wiki.id).unwrap().len();
    // Another act holds the wiki's lease: recovery is told the wiki is busy, with the depth, and does nothing.
    let held = w
        .leases
        .acquire(&w.wiki.id, Purpose::Acceptance, None)
        .unwrap();
    let err = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap_err();
    assert!(
        matches!(&err, RecoveryError::Busy(b) if b.depth >= 1),
        "{err:?}"
    );
    assert_eq!(w.queue_count(), 1);
    assert_eq!(
        w.store.recovery_acts(&w.wiki.id).unwrap().len(),
        acts_before
    );
    assert!(w.audit().is_empty(), "not even begun");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    // The lease being free is what lets it run: dropped, recovery runs and holds it for its own length.
    drop(held);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered);
    assert!(r.under_lease);
    // The signer refuses a recovery operation outside a lease on its own account, whatever the service did.
    let token = inklkunn_core::RecoveryToken::new(r.act.unwrap().to_string());
    let err = w
        .signer
        .restore_worktree(&w.wiki.id, MAIN, &token)
        .unwrap_err();
    assert_eq!(err, inklkunn_core::RecoverError::NoLease);
}

#[test]
fn a_step_that_fails_stops_the_recovery_keeps_the_flag_and_records_it() {
    let Some(w) = World::new("step-fails", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Half done",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    w.queued_acceptance(&p);
    // The seal is made but `verify` cannot pass: the records are written (the change is in the history), the flag
    // stays, and the recovery says where it stopped.
    w.control("fail-verify", "");
    let r = w.recover();
    w.uncontrol("fail-verify");
    assert!(
        matches!(
            &r.outcome,
            RecoveryOutcome::Stopped {
                at: Some("verify"),
                ..
            }
        ),
        "{:#?}",
        r.lines()
    );
    assert_eq!(
        r.done.iter().map(|d| d.step).collect::<Vec<_>>(),
        ["seal-queued", "end-sealed"]
    );
    assert!(
        w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
        "the flag is kept"
    );
    assert_eq!(
        w.state(&p),
        ProposalState::Accepted,
        "what was done is recorded"
    );
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    assert_eq!(
        acts.last().unwrap().end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Stopped)
    );
    assert!(
        w.audit()
            .iter()
            .any(|(a, o, d)| a == "recovery-step" && o == "failed" && d == "verify failed")
    );
    // A second run finds the change in the history, has nothing to seal, and finishes the job.
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        step_names(&r),
        ["verify", "reconcile", "clear-flag"],
        "nothing left but the checks"
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
}

#[test]
fn a_recovery_that_was_itself_interrupted_is_ended_and_the_next_one_goes_on() {
    let Some(w) = World::new("interrupted", LeaseConfig::default()) else {
        return;
    };
    // An earlier recovery began and never ended (its process died).
    let earlier = inklkunn_store::RecoveryActRecord {
        id: inklkunn_store::RecoveryActId::new(),
        wiki: w.wiki.id.clone(),
        plan: "{}".to_owned(),
        created_at: 1,
    };
    w.store.begin_recovery_act(&earlier).unwrap();
    let reasons = w.recovery.needs_recovery(&w.wiki.id).unwrap();
    assert_eq!(reasons, ["interrupted-recovery"]);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    assert_eq!(acts.len(), 2);
    assert_eq!(nth(&acts, 0).act, earlier);
    assert_eq!(
        nth(&acts, 0).end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Stopped)
    );
    assert_eq!(
        nth(&acts, 1).end.as_ref().map(|e| e.ending),
        Some(RecoveryEnding::Recovered)
    );
    assert!(w.recovery.needs_recovery(&w.wiki.id).unwrap().is_empty());
}

#[test]
fn a_wiki_that_needs_nothing_is_left_alone_and_a_dead_holders_record_alone_is_cleared() {
    let Some(w) = World::new("nothing", LeaseConfig::default()) else {
        return;
    };
    assert!(w.recovery.needs_recovery(&w.wiki.id).unwrap().is_empty());
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::NothingToDo);
    assert!(
        w.store.recovery_acts(&w.wiki.id).unwrap().is_empty(),
        "nothing recorded"
    );
    assert!(w.audit().is_empty());
    // An export's or a refresh's record that was left is not a reason: they leave nothing in the repository.
    for purpose in ["export", "refresh"] {
        w.store
            .record_lease_holder(&inklkunn_store::LeaseHolder {
                wiki: w.wiki.id.clone(),
                holder: "inklkunn-gone".to_owned(),
                purpose: purpose.to_owned(),
                acceptance: None,
                since: 1,
            })
            .unwrap();
        assert!(
            w.recovery.needs_recovery(&w.wiki.id).unwrap().is_empty(),
            "{purpose}"
        );
    }
    // An acceptance's is: a process died holding the lease; what it did is read from the witnesses.
    w.store
        .record_lease_holder(&inklkunn_store::LeaseHolder {
            wiki: w.wiki.id.clone(),
            holder: "inklkunn-gone".to_owned(),
            purpose: "acceptance".to_owned(),
            acceptance: None,
            since: 1,
        })
        .unwrap();
    assert_eq!(
        w.recovery.needs_recovery(&w.wiki.id).unwrap(),
        ["dead-holder"]
    );
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(step_names(&r), ["verify", "reconcile", "clear-flag"]);
    assert_eq!(
        w.leases.recorded_holder(&w.wiki.id).unwrap(),
        None,
        "the stale record is gone"
    );
    assert!(w.recovery.needs_recovery(&w.wiki.id).unwrap().is_empty());
    assert!(w.reconciled().is_clean());
}

#[test]
fn doctors_unreached_object_warnings_are_recorded_and_do_not_block() {
    let Some(w) = World::new("unreached", LeaseConfig::default()) else {
        return;
    };
    // A report as prikk 0.55.0 prints it with objects nothing refers to: a warning, exit 0.
    w.control(
        "say-doctor",
        "doctor repository: /srv/r/.prikk\n\
         warning [PRIKK-DOCTOR-UNREACHED-OBJECT]: 3 objects are not reachable from any ref or the queue\n\
         \x20 recommendation: the bytes stay; no command removes them yet\n\
         info [PRIKK-DOCTOR-VERIFY-OK]: repository structural verification scan completed\n\
         issue summary: errors=0, warnings=1, info=1\n",
    );
    w.flag(RecoveryReason::SealedUnconfirmed);
    let r = w.recover();
    w.uncontrol("say-doctor");
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert!(r.plan.notes.iter().any(|n| n.contains("unreached-object")));
    assert!(w.audit().iter().any(|(a, o, d)| a == "recovery-note"
        && o == "reported"
        && d.contains("unreached objects: 1")));
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
}

#[test]
fn a_doctor_report_of_a_shape_nobody_knows_stops_recovery_before_anything_is_planned() {
    let Some(w) = World::new("odd-doctor", LeaseConfig::default()) else {
        return;
    };
    w.control("say-doctor", "something a later prikk prints\n");
    w.flag(RecoveryReason::Interrupted);
    let err = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap_err();
    w.uncontrol("say-doctor");
    assert!(
        matches!(&err, RecoveryError::Witness(why) if why.contains("doctor")),
        "{err:?}"
    );
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert!(
        w.store.recovery_acts(&w.wiki.id).unwrap().is_empty(),
        "no plan was made from what could not be read"
    );
    // The trail says it began and that it ended without a plan.
    let actions: Vec<(String, String)> = w.audit().into_iter().map(|(a, o, _)| (a, o)).collect();
    assert_eq!(
        actions,
        [
            ("recovery-begin".to_owned(), "allowed".to_owned()),
            ("recovery".to_owned(), "failed".to_owned())
        ]
    );
}

#[test]
fn an_acceptance_that_stopped_before_commit_is_abandoned_and_a_dirty_worktree_is_put_back() {
    let Some(w) = World::new("abandon", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Never committed",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    // The acceptance's records are written and the files are in the worktree; `commit` never ran.
    w.control("fail-commit", "");
    let err = w.accept(&p).unwrap_err();
    w.uncontrol("fail-commit");
    assert!(matches!(err, AcceptFailure::Failed { .. }), "{err:?}");
    // The failure ended the acceptance on its own account; plant what a kill leaves instead: a pending acceptance
    // with an open intent, and a changed file in the worktree.
    let q = w.propose(
        "Killed",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    let rec = inklkunn_store::AcceptanceRecord {
        id: inklkunn_store::AcceptanceId::new(),
        proposal: q.id.clone(),
        revision: 1,
        wiki: w.wiki.id.clone(),
        view: MAIN.to_owned(),
        acceptor: w.maintainer_id.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "bootstrap-1".to_owned(),
        proposer: w.contributor_id.clone(),
        digest: "d".repeat(64),
        bytes: 1,
        created_at: 3,
    };
    assert!(matches!(
        w.store.begin_acceptance(&rec).unwrap(),
        inklkunn_store::AcceptanceBegin::Started
    ));
    let message = "Killed\n\nKind: proposal\n";
    w.store
        .record_intent(&inklkunn_store::IntentRecord {
            id: inklkunn_store::IntentId::new(),
            acceptance: rec.id.clone(),
            wiki: w.wiki.id.clone(),
            view: MAIN.to_owned(),
            base_block: w.tip(),
            message_digest: inklkunn_core::acceptance::message_digest(message),
            message: message.to_owned(),
            created_at: 4,
        })
        .unwrap();
    std::fs::write(w.repo.join("pages/Other.md"), "scribbled\n").unwrap();
    std::fs::write(w.repo.join("pages/Stray.md"), "stray\n").unwrap();
    let before = w.history();
    let reasons = w.recovery.needs_recovery(&w.wiki.id).unwrap();
    assert!(
        reasons.contains(&"open-intent") && reasons.contains(&"pending-acceptance"),
        "{reasons:?}"
    );
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        step_names(&r),
        [
            "restore-worktree",
            "end-failed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert_eq!(w.history(), before);
    assert_eq!(w.page("pages/Other.md"), OTHER);
    assert_eq!(
        std::fs::read_to_string(w.repo.join("pages/Other.md")).unwrap(),
        OTHER
    );
    assert!(!w.repo.join("pages/Stray.md").exists());
    let entry = w.store.acceptances_of(&q.id, 1).unwrap().remove(0);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(AcceptanceOutcome::Failed { word }) if word == "interrupted"
    ));
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(
        w.store
            .intents(&w.wiki.id)
            .unwrap()
            .last()
            .unwrap()
            .completion
            .as_ref()
            .map(|c| c.end),
        Some(IntentEnd::Abandoned)
    );
    assert_eq!(
        w.state(&q),
        ProposalState::HeldForReview,
        "the proposal stays open"
    );
    assert!(w.accept(&q).is_ok(), "and may be accepted");
}
