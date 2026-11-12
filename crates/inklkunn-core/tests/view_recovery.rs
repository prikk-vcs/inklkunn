//! A view's creation is a recoverable act (handoff 005 K4; review of 2026-11-08, finding K-c), against a real prikk
//! and the real signer. prikk's `branch create` publishes the ref's pointer and then appends the ref log; killed
//! between the two it leaves the view **listed**, `verify` failing, and every other publication refusing until
//! `prikk ref complete <ref>` (prikk 0.49.0 and later) finishes it. So a holder that died creating a view flags
//! the wiki, and recovery reads the act, the listing and `doctor`:
//!
//! - the view is there at the block it was forked from and nothing is unfinished: the act ends *created*;
//! - the same with exactly that ref's publication unfinished: prikk's own recommendation, `ref complete`, run by the
//!   signer bound to the recovery act, then *created*;
//! - the view is not there and nothing is unfinished: *failed*;
//! - anything else: a stop (`view-creation-unclear`, or `doctor-trouble`), and nothing is changed.
//!
//! The dead holder is made as the service would leave it: the act is recorded and the lease holder's record is
//! another process's, and the signer is asked directly (what `create_view` does between its record and its end); the
//! end is never written. A stand-in `prikk` passes every call to the real one unless a control file names the verb:
//! `leads-after-branch-create` runs the real `branch create` to completion and puts the ref log back as it was (the
//! state a kill between the two writes leaves, made on cue), and `kill-branch-create` (holding a number of steps of a
//! shell loop) starts the real verb and sends it `SIGKILL` after those steps: a real kill, whose landing depends on how far the
//! verb got. Self-skips with a visible line when prikk is not on `PATH`.
//!
//! Every sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::Accept;
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::recovery::{Step, Stop};
use inklkunn_core::views::ViewKind;
use inklkunn_core::{
    AcceptFailure, AcceptanceParts, Authorizer, IdentityAuthority, KeyAuthority, LeaseConfig,
    Principal, ProposalAcceptor, ReadConfig, ReadService, Recover, Recovery, RecoveryError,
    RecoveryMode, RecoveryOutcome, RecoveryParts, Signer, SignerConfig, StoreAuthz, StoreLedger,
    ViewCreationAct, ViewFailure, WikiLeases, Witnesses, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceStore, Account, AccountId, AccountStore, ActStore, DisplayName, FileChange,
    LeaseHolder, MemoryAudit, MemoryStore, NewWiki, RecoveryReason, Role, RoleGrant, RoleStore,
    Slug, ViewActId, ViewActOutcome, ViewActRecord,
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
    let p = base.join(format!("inklkunn-vrec-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
if [ "$1 $2" = "branch create" ] && [ -e "$CTL/leads-after-branch-create" ]; then
  mkdir -p "$CTL/kept-ref-log"
  for f in .prikk/refs/containers/log-*.container; do cp "$f" "$CTL/kept-ref-log/"; done
  "{real}" "$@" > /dev/null 2>&1
  for f in "$CTL"/kept-ref-log/log-*.container; do cp "$f" .prikk/refs/containers/; done
  echo "error: the call was stopped after branch create had published its pointer" >&2; exit 1
fi
if [ "$1 $2" = "branch create" ] && [ -e "$CTL/kill-branch-create" ]; then
  "{real}" "$@" > /dev/null 2>&1 &
  real_pid=$!
  n="$(cat "$CTL/kill-branch-create")"; i=0
  while [ "$i" -lt "$n" ]; do i=$((i + 1)); done
  kill -9 "$real_pid" 2> /dev/null
  wait "$real_pid" 2> /dev/null
  echo "error: the call was killed" >&2; exit 1
fi
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

const HOME: &str = "---\ntitle: Home\nid: p-home\n---\n\nOne.\n\nTwo.\n";

struct World {
    _tmp: Dir,
    _shm: Dir,
    ctl: PathBuf,
    store: Arc<MemoryStore>,
    audit: Arc<MemoryAudit>,
    storage: Arc<PrikkCli>,
    proposals: Arc<ProposalService>,
    acceptor: Arc<ProposalAcceptor>,
    signer: Arc<Signer>,
    recovery: Recovery,
    leases: Arc<WikiLeases>,
    standin: PathBuf,
    layout: inklkunn_store::RootLayout,
    repo: PathBuf,
    wiki: inklkunn_store::WikiRecord,
    maintainer: Principal,
    reviewer: Principal,
    reviewer_id: AccountId,
    contributor: Principal,
}

impl World {
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
        let ctl = tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let standin = write_standin(&ctl, &real);
        let secret = tmp.0.join("secret");
        std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
        let layout = inklkunn_store::RootLayout::new(tmp.0.join("root"));
        let store = Arc::new(MemoryStore::new());
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
                &standin,
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
            "view-recovery-test",
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
            audit: audit.clone(),
        })
        .with_reads(reads);
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
        Some(World {
            maintainer: Principal::session(authority(), maintainer_id),
            reviewer: Principal::session(authority(), reviewer_id.clone()),
            reviewer_id,
            contributor: Principal::session(authority(), contributor_id),
            _tmp: tmp,
            _shm: shm,
            ctl,
            store,
            audit,
            storage,
            proposals,
            acceptor,
            signer,
            recovery,
            leases,
            standin,
            layout,
            repo: created.repo,
            wiki,
        })
    }

    fn control(&self, name: &str, content: &str) {
        std::fs::write(self.ctl.join(name), content).unwrap();
    }

    fn uncontrol(&self, name: &str) {
        let _ = std::fs::remove_file(self.ctl.join(name));
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

    /// Every view prikk lists, closed ones included.
    fn listed(&self) -> Vec<String> {
        self.storage
            .all_views(&self.repo)
            .unwrap()
            .views
            .into_iter()
            .map(|v| v.name)
            .collect()
    }

    fn verifies(&self) -> bool {
        self.storage.verify(&self.repo).unwrap().verdict.ok
    }

    /// The record the service writes before it asks the signer, and the record of a holder that is gone.
    fn record_act(&self, name: &str, from_block: &str) -> ViewActRecord {
        let act = ViewActRecord {
            id: ViewActId::new(),
            wiki: self.wiki.id.clone(),
            name: name.to_owned(),
            forked_from: "main".to_owned(),
            from_block: from_block.to_owned(),
            kind: ViewKind::Experimental,
            created_by: self.reviewer_id.clone(),
            created_at: 1,
        };
        self.store.begin_view_act(&act).unwrap();
        self.store
            .record_lease_holder(&LeaseHolder {
                wiki: self.wiki.id.clone(),
                holder: format!("a-process-that-is-gone-{name}"),
                purpose: "view".to_owned(),
                acceptance: None,
                since: 1,
            })
            .unwrap();
        act
    }

    /// What `create_view` does between its record and its end, and then nothing: the holder dies.
    fn ask_the_signer_then_die(&self, act: &ViewActRecord) {
        let r = self.signer.create_view(
            &self.wiki.id,
            &format!("heads/{}", act.name),
            MAIN,
            &ViewCreationAct::new(act.id.as_str()),
        );
        println!("the signer answered {r:?}");
    }

    fn act_end(&self, act: &ViewActRecord) -> Option<ViewActOutcome> {
        self.store
            .view_acts(&self.wiki.id)
            .unwrap()
            .into_iter()
            .find(|e| e.act.id == act.id)
            .and_then(|e| e.end.map(|e| e.outcome))
    }

    fn create(&self, name: &str) -> Result<inklkunn_core::ViewCreated, ViewFailure> {
        self.acceptor.create_view(
            &self.reviewer,
            &self.wiki,
            name,
            None,
            ViewKind::Experimental,
        )
    }

    /// A proposal by the contributor in the default view, taken in by the maintainer.
    fn accept_a_change(&self, text: &str) -> Result<inklkunn_core::Accepted, AcceptFailure> {
        let edit = ProposalEdit {
            summary: "Reword the page".to_owned(),
            files: vec![FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(text.to_owned()),
            }],
            ..ProposalEdit::default()
        };
        let draft = self
            .proposals
            .create(&self.contributor, &self.wiki, None, edit)
            .unwrap_or_else(|e| panic!("create: {e}"));
        let held = self
            .proposals
            .submit(&self.contributor, &self.wiki, &draft.id)
            .unwrap_or_else(|e| panic!("submit: {e}"));
        self.acceptor
            .accept(&self.maintainer, &self.wiki, &held.id, 1)
    }

    fn plan_names(&self) -> Vec<&'static str> {
        let report = self
            .recovery
            .recover(&self.wiki, RecoveryMode::PlanOnly)
            .unwrap();
        report.plan.steps.iter().map(Step::name).collect()
    }
}

#[test]
fn a_holder_that_died_after_prikk_published_the_pointer_is_recovered_with_ref_complete() {
    let Some(w) = World::new("lead") else {
        return;
    };
    let base = w.tip(MAIN);
    let act = w.record_act("draft", &base);
    w.control("leads-after-branch-create", "");
    w.ask_the_signer_then_die(&act);
    w.uncontrol("leads-after-branch-create");
    // What the kill left: the view is listed, the repository does not verify, and the act has no end.
    assert!(w.listed().contains(&"heads/draft".to_owned()));
    assert!(!w.verifies());
    assert_eq!(w.act_end(&act), None);
    // The wiki is flagged by the next write, with the act named, and nothing is created or accepted meanwhile.
    let refused = w.create("other");
    assert!(
        matches!(&refused, Err(ViewFailure::NeedsRecovery(f)) if f.reason == RecoveryReason::Interrupted),
        "{refused:?}"
    );
    assert!(!w.listed().contains(&"heads/other".to_owned()));
    let refused = w.accept_a_change(&HOME.replace("One.", "First."));
    assert!(
        matches!(&refused, Err(AcceptFailure::NeedsRecovery(_))),
        "{refused:?}"
    );
    // The plan is prikk's own recommendation, then the end, then the tail every recovery has.
    assert_eq!(
        w.plan_names(),
        [
            "complete-view-creation",
            "end-view-created",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    let report = w
        .recovery
        .recover(&w.wiki, RecoveryMode::Run)
        .expect("recovery runs");
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), Some(ViewActOutcome::Created));
    assert!(w.verifies());
    assert_eq!(
        w.tip("heads/draft"),
        base,
        "the view is at the block it was forked from"
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    // Each step is in the audit trail before it is taken.
    let events = w.audit.events();
    let steps: Vec<&str> = events
        .iter()
        .filter(|e| e.action == "recovery-step")
        .map(|e| e.detail.as_str())
        .collect();
    assert_eq!(
        steps.get(..2),
        Some(&["complete-view-creation", "end-view-created"][..])
    );
    // The name is taken (the act created the view), and the wiki publishes and accepts again.
    assert_eq!(w.create("draft").unwrap_err(), ViewFailure::NameTaken);
    w.create("other").expect("a view is created again");
    w.accept_a_change(&HOME.replace("One.", "First."))
        .expect("a change is accepted again");
    assert!(w.verifies());
}

#[test]
fn a_holder_that_died_before_prikk_wrote_anything_is_recovered_as_failed_and_the_name_is_free() {
    let Some(w) = World::new("nothing") else {
        return;
    };
    let act = w.record_act("draft", &w.tip(MAIN));
    // The holder died after the record and before the signer was asked: nothing is in the repository.
    assert!(!w.listed().contains(&"heads/draft".to_owned()));
    assert_eq!(
        w.plan_names(),
        ["end-view-failed", "verify", "reconcile", "clear-flag"]
    );
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert!(matches!(
        w.act_end(&act),
        Some(ViewActOutcome::Failed { word }) if word == "interrupted"
    ));
    w.create("draft").expect("the name is free again");
}

#[test]
fn a_holder_that_died_after_prikk_finished_the_view_is_recovered_as_created() {
    let Some(w) = World::new("whole") else {
        return;
    };
    let act = w.record_act("draft", &w.tip(MAIN));
    w.ask_the_signer_then_die(&act);
    assert!(w.verifies(), "prikk finished the publication");
    assert_eq!(w.act_end(&act), None);
    // Not settled from the listing: recovery reads `doctor` and sees nothing unfinished, and only then ends it.
    assert_eq!(
        w.plan_names(),
        ["end-view-created", "verify", "reconcile", "clear-flag"]
    );
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), Some(ViewActOutcome::Created));
    assert_eq!(w.create("draft").unwrap_err(), ViewFailure::NameTaken);
}

#[test]
fn a_view_at_another_block_than_the_one_it_was_forked_from_is_a_stop_and_nothing_is_changed() {
    let Some(w) = World::new("stop-block") else {
        return;
    };
    // The record says another block than the one the view was made at (the record is not this view's).
    let act = w.record_act("draft", &"e".repeat(64));
    w.ask_the_signer_then_die(&act);
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert!(
        matches!(report.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:#?}",
        report.lines()
    );
    assert_eq!(report.plan.stop, Some(Stop::ViewCreationUnclear));
    assert!(report.done.is_empty());
    assert_eq!(w.act_end(&act), None, "the act is not ended");
    assert!(
        w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
        "the flag stays"
    );
    assert!(w.verifies());
    assert!(
        report
            .lines()
            .iter()
            .any(|l| l.contains("STOP (view-creation-unclear)")),
        "{:#?}",
        report.lines()
    );
}

#[test]
fn two_view_creations_that_never_ended_one_of_them_half_published_are_a_stop() {
    let Some(w) = World::new("stop-other") else {
        return;
    };
    let base = w.tip(MAIN);
    // `other` was being published when its holder died; `draft` has a record and nothing in the repository.
    let other = w.record_act("other", &base);
    w.control("leads-after-branch-create", "");
    w.ask_the_signer_then_die(&other);
    w.uncontrol("leads-after-branch-create");
    let draft = w.record_act("draft", &base);
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    // Two creations never ended: recovery does not guess which publication is whose.
    assert!(
        matches!(report.outcome, RecoveryOutcome::Stopped { .. }),
        "{:#?}",
        report.lines()
    );
    assert_eq!(report.plan.stop, Some(Stop::SeveralActs));
    assert_eq!(w.act_end(&other), None);
    assert_eq!(w.act_end(&draft), None);
    assert!(!w.verifies(), "nothing was completed");
}

#[test]
fn a_view_creation_that_failed_in_the_signer_after_the_pointer_leaves_the_act_open_and_flags_the_wiki()
 {
    let Some(w) = World::new("uncertain") else {
        return;
    };
    // The production path, the holder alive: `branch create` fails after it published the pointer. The view is
    // listed, so it is not called failed, and prikk did not say it finished, so it is not called created.
    w.control("leads-after-branch-create", "");
    let made = w.create("draft");
    w.uncontrol("leads-after-branch-create");
    assert!(matches!(&made, Err(ViewFailure::Uncertain(_))), "{made:?}");
    let entries = w.store.view_acts(&w.wiki.id).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries.first().is_some_and(|e| e.end.is_none()));
    let flag = w.store.recovery_flag(&w.wiki.id).unwrap().expect("flagged");
    assert_eq!(flag.reason, RecoveryReason::Interrupted);
    assert_eq!(
        flag.act,
        entries.first().map(|e| e.act.id.to_string()),
        "the flag names the act"
    );
    assert!(matches!(
        w.accept_a_change(&HOME.replace("One.", "First.")),
        Err(AcceptFailure::NeedsRecovery(_))
    ));
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert!(w.verifies());
    assert_eq!(
        entries.first().and_then(|e| w.act_end(&e.act)),
        Some(ViewActOutcome::Created)
    );
    w.accept_a_change(&HOME.replace("One.", "First."))
        .expect("a change is accepted again");
}

/// How many real kills to make (`INKLKUNN_VR_KILLS` raises it, to look at where they land).
fn kills() -> usize {
    std::env::var("INKLKUNN_VR_KILLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
}

#[test]
fn a_real_sigkill_during_branch_create_leaves_a_state_recovery_finishes_or_ends_whatever_it_is() {
    let Some(w) = World::new("sigkill") else {
        return;
    };
    let (mut absent, mut lead, mut whole, mut empty_lock) = (0, 0, 0, 0);
    for i in 0..kills() {
        let name = format!("v{i}");
        let base = w.tip(MAIN);
        let act = w.record_act(&name, &base);
        // The real `branch create` is killed `steps` steps of a shell loop after it starts: from before it wrote anything to after
        // it finished, depending on how far it got.
        let steps = 6 * i;
        w.control("kill-branch-create", &steps.to_string());
        w.ask_the_signer_then_die(&act);
        w.uncontrol("kill-branch-create");
        let listed = w.listed().contains(&format!("heads/{name}"));
        let verifies = w.verifies();
        match (listed, verifies) {
            (false, true) => absent += 1,
            (true, false) => lead += 1,
            (true, true) => whole += 1,
            (false, false) => panic!("a kill left something no row of the plan knows: {name}"),
        }
        let report = match w.recovery.recover(&w.wiki, RecoveryMode::Run) {
            Ok(report) => report,
            // A kill the instant prikk created a lock file leaves it empty, and recovery cannot read the lock listing:
            // the state docs/RECOVERY.md's "An empty lock file" is for. The operator removes that one empty file, and
            // recovery then makes its plan.
            Err(RecoveryError::Witness(why)) if why.contains("a recorded pid is not a number") => {
                assert!(
                    clear_empty_locks(&w.repo.join(".prikk")) > 0,
                    "{name}: {why}"
                );
                empty_lock += 1;
                w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap()
            }
            Err(e) => panic!("{name}: {e}"),
        };
        assert_eq!(
            report.outcome,
            RecoveryOutcome::Recovered,
            "{name}: {:#?}",
            report.lines()
        );
        assert!(w.verifies(), "{name}");
        assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None, "{name}");
        // The act ended as what the repository shows: created when the view is there, failed when it is not.
        let now_listed = w.listed().contains(&format!("heads/{name}"));
        assert_eq!(
            now_listed, listed,
            "{name}: recovery neither makes nor removes a view"
        );
        match w.act_end(&act) {
            Some(ViewActOutcome::Created) => assert!(now_listed, "{name}"),
            Some(ViewActOutcome::Failed { word }) => {
                assert_eq!(word, "interrupted");
                assert!(!now_listed, "{name}");
            }
            None => panic!("{name}: the act was not ended"),
        }
        if now_listed {
            assert_eq!(w.tip(&format!("heads/{name}")), base, "{name}");
        }
    }
    println!(
        "real kills: {absent} left no view, {lead} left a pointer ahead of the log, {whole} left a whole view; {empty_lock} of them also left an empty lock file"
    );
    // After all of them the wiki publishes and accepts, which it could not with one publication unfinished.
    w.create("after").expect("a view is created");
    w.accept_a_change(&HOME.replace("One.", "First."))
        .expect("a change is accepted");
    assert!(w.verifies());
}

/// Remove every empty `*.lock` file under `dir`, as an operator would clear a lock that a killed prikk created and
/// never wrote a word to. Returns how many.
fn clear_empty_locks(dir: &Path) -> usize {
    let mut removed = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            removed += clear_empty_locks(&path);
        } else if meta.is_file()
            && meta.len() == 0
            && path.extension().is_some_and(|e| e == "lock")
            && std::fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

// ---- the witnesses and the steps that rest on them ----

#[test]
fn an_open_view_creation_is_a_reason_to_recover_with_no_dead_holder_and_no_flag() {
    let Some(w) = World::new("reason") else {
        return;
    };
    // What a start of the service asks of every wiki, from the records alone.
    let reasons = |w: &World| {
        inklkunn_core::recovery::needs_recovery(&*w.store, &*w.store, &w.wiki.id, None).unwrap()
    };
    assert_eq!(reasons(&w), Vec::<&str>::new());
    let act = ViewActRecord {
        id: ViewActId::new(),
        wiki: w.wiki.id.clone(),
        name: "draft".to_owned(),
        forked_from: "main".to_owned(),
        from_block: w.tip(MAIN),
        kind: ViewKind::Experimental,
        created_by: w.reviewer_id.clone(),
        created_at: 1,
    };
    w.store.begin_view_act(&act).unwrap();
    assert_eq!(reasons(&w), ["open-view-act"]);
    assert_eq!(
        w.recovery.needs_recovery(&w.wiki.id).unwrap(),
        ["open-view-act"]
    );
    // Ended, it is none.
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert_eq!(reasons(&w), Vec::<&str>::new());
}

#[test]
fn a_view_whose_name_differs_only_by_case_is_not_the_acts_view_and_recovery_stops() {
    let Some(w) = World::new("case") else {
        return;
    };
    let base = w.tip(MAIN);
    // `Draft` is in the repository (made some other way); the act is for `draft` and nothing of it was published.
    let other = w.record_act("Draft", &base);
    w.ask_the_signer_then_die(&other);
    w.store
        .finish_view_act(&inklkunn_store::ViewActEnd {
            act: other.id.clone(),
            outcome: ViewActOutcome::Created,
            at: 2,
        })
        .unwrap();
    let act = w.record_act("draft", &base);
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.plan.stop,
        Some(Stop::ViewCreationUnclear),
        "{:#?}",
        report.lines()
    );
    // It says why: a view of the name is there in another case; it is not read as this creation's view.
    assert!(
        report
            .lines()
            .iter()
            .any(|l| l.contains("prikk shows: a view of that name in another case")),
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), None);
}

/// The real signer for everything, except that the completion of a publication does nothing and says it is done: a
/// signer that lies. The act must not be ended as created on its word.
struct SaysDone(Arc<Signer>);

impl Recover for SaysDone {
    fn complete_view_creation(
        &self,
        _: &inklkunn_store::WikiId,
        _: &str,
        _: &inklkunn_core::RecoveryToken,
        _: &ViewCreationAct,
    ) -> Result<(), inklkunn_core::RecoverError> {
        Ok(())
    }

    fn seal_queued(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &inklkunn_core::AcceptanceToken,
    ) -> Result<(), inklkunn_core::RecoverError> {
        self.0.seal_queued(wiki, view, act, acceptance)
    }

    fn discard_damaged(
        &self,
        wiki: &inklkunn_store::WikiId,
        act: &inklkunn_core::RecoveryToken,
    ) -> Result<inklkunn_signer::recover::text::Discarded, inklkunn_core::RecoverError> {
        self.0.discard_damaged(wiki, act)
    }

    fn clear_locks(
        &self,
        wiki: &inklkunn_store::WikiId,
        act: &inklkunn_core::RecoveryToken,
        locks: &[String],
    ) -> Result<Vec<String>, inklkunn_core::RecoverError> {
        self.0.clear_locks(wiki, act, locks)
    }

    fn restore_worktree(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
    ) -> Result<usize, inklkunn_core::RecoverError> {
        self.0.restore_worktree(wiki, view, act)
    }
}

impl World {
    fn recovery_over(&self, signer: Arc<dyn Recover>, storage: Arc<dyn Storage>) -> Recovery {
        Recovery::new(RecoveryParts {
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            proposals: self.store.clone(),
            storage,
            witnesses: Arc::new(Witnesses::new(
                &self.standin,
                Sandbox::Unconfined,
                self.layout.clone(),
            )),
            signer,
            leases: self.leases.clone(),
            layout: self.layout.clone(),
            audit: self.audit.clone(),
        })
    }
}

#[test]
fn an_act_is_not_ended_as_created_on_a_signers_word_that_the_publication_is_finished() {
    let Some(w) = World::new("liar") else {
        return;
    };
    let act = w.record_act("draft", &w.tip(MAIN));
    w.control("leads-after-branch-create", "");
    w.ask_the_signer_then_die(&act);
    w.uncontrol("leads-after-branch-create");
    let liar = w.recovery_over(Arc::new(SaysDone(w.signer.clone())), w.storage.clone());
    let report = liar.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert!(
        matches!(
            &report.outcome,
            RecoveryOutcome::Stopped {
                at: Some("end-view-created"),
                ..
            }
        ),
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), None, "the act is not ended");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert!(!w.verifies());
    // The honest signer then finishes it from the same state.
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), Some(ViewActOutcome::Created));
}

/// What `Moving` does to the second listing of views and to the second history of `heads/draft`.
type EditViews = Box<dyn Fn(&mut inklkunn_prikk::Views) + Send + Sync>;
type EditHistory = Box<dyn Fn(&mut inklkunn_prikk::History) + Send + Sync>;

/// prikk, except that from the second time it is asked its full listing of views, and its history of `heads/draft`,
/// are edited: what a state that moved between the plan and the step looks like.
struct Moving {
    inner: Arc<PrikkCli>,
    listings: std::sync::atomic::AtomicUsize,
    histories: std::sync::atomic::AtomicUsize,
    edit_views: EditViews,
    edit_history: EditHistory,
}

impl Moving {
    fn new(inner: Arc<PrikkCli>, edit_views: EditViews, edit_history: EditHistory) -> Self {
        Moving {
            inner,
            listings: std::sync::atomic::AtomicUsize::new(0),
            histories: std::sync::atomic::AtomicUsize::new(0),
            edit_views,
            edit_history,
        }
    }
}

impl Storage for Moving {
    fn views(&self, repo: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::Views> {
        self.inner.views(repo)
    }

    fn all_views(&self, repo: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::Views> {
        let mut views = self.inner.all_views(repo)?;
        if self
            .listings
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            >= 1
        {
            (self.edit_views)(&mut views);
        }
        Ok(views)
    }

    fn tree(
        &self,
        repo: &Path,
        point: &Point,
        prefix: Option<&str>,
    ) -> inklkunn_prikk::Result<inklkunn_prikk::Tree> {
        self.inner.tree(repo, point, prefix)
    }

    fn stat(
        &self,
        repo: &Path,
        point: &Point,
        path: &str,
    ) -> inklkunn_prikk::Result<inklkunn_prikk::FileMeta> {
        self.inner.stat(repo, point, path)
    }

    fn read(
        &self,
        repo: &Path,
        point: &Point,
        path: &str,
        max_bytes: u64,
    ) -> inklkunn_prikk::Result<Vec<u8>> {
        self.inner.read(repo, point, path, max_bytes)
    }

    fn history(
        &self,
        repo: &Path,
        view: &Point,
        limit: Option<u64>,
    ) -> inklkunn_prikk::Result<inklkunn_prikk::History> {
        let mut history = self.inner.history(repo, view, limit)?;
        if *view == Point::View("heads/draft".to_owned())
            && self
                .histories
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                >= 1
        {
            (self.edit_history)(&mut history);
        }
        Ok(history)
    }

    fn change(&self, repo: &Path, id: &str) -> inklkunn_prikk::Result<inklkunn_prikk::Change> {
        self.inner.change(repo, id)
    }

    fn diff(
        &self,
        repo: &Path,
        from: &Point,
        to: &Point,
        paths: &[&str],
    ) -> inklkunn_prikk::Result<inklkunn_prikk::Diff> {
        self.inner.diff(repo, from, to, paths)
    }

    fn plan(
        &self,
        repo: &Path,
        point: &Point,
    ) -> inklkunn_prikk::Result<inklkunn_prikk::Materialized> {
        self.inner.plan(repo, point)
    }

    fn materialize(
        &self,
        repo: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        bounds: inklkunn_prikk::ExtractBounds,
    ) -> inklkunn_prikk::Result<inklkunn_prikk::Materialized> {
        self.inner.materialize(repo, point, scratch, name, bounds)
    }

    fn verify(&self, repo: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::Verification> {
        self.inner.verify(repo)
    }

    fn status(&self, repo: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::RepoStatus> {
        self.inner.status(repo)
    }
}

#[test]
fn an_act_is_ended_only_as_what_prikk_shows_at_the_step_not_as_what_it_showed_when_the_plan_was_made()
 {
    // Planned as *failed* (the view is not there) and the view is there at the step; planned as *created* (the view is
    // there at the block it was forked from) and it is gone at the step, or at another block: neither is written.
    let Some(w) = World::new("moved") else {
        return;
    };
    let base = w.tip(MAIN);
    let stopped_at = |report: &inklkunn_core::recovery::RecoveryReport, step: &str| {
        assert!(
            matches!(&report.outcome, RecoveryOutcome::Stopped { at: Some(at), .. } if *at == step),
            "{step}: {:#?}",
            report.lines()
        );
    };
    let unchanged = |_: &mut inklkunn_prikk::History| {};
    let absent = w.record_act("draft", &base);
    let appears = w.recovery_over(
        w.signer.clone(),
        Arc::new(Moving::new(
            w.storage.clone(),
            Box::new(|views| {
                views.views.push(inklkunn_prikk::View {
                    name: "heads/draft".to_owned(),
                    state_id: "f".repeat(64),
                    closed: false,
                    current: false,
                });
            }),
            Box::new(unchanged),
        )),
    );
    let report = appears.recover(&w.wiki, RecoveryMode::Run).unwrap();
    stopped_at(&report, "end-view-failed");
    assert_eq!(w.act_end(&absent), None);
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(
        report.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        report.lines()
    );

    let whole = w.record_act("whole", &base);
    w.ask_the_signer_then_die(&whole);
    let vanishes = w.recovery_over(
        w.signer.clone(),
        Arc::new(Moving::new(
            w.storage.clone(),
            Box::new(|views| views.views.retain(|v| v.name == MAIN)),
            Box::new(unchanged),
        )),
    );
    let report = vanishes.recover(&w.wiki, RecoveryMode::Run).unwrap();
    stopped_at(&report, "end-view-created");
    assert_eq!(w.act_end(&whole), None);
    let report = w.recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(report.outcome, RecoveryOutcome::Recovered);
}

#[test]
fn an_act_is_ended_as_created_only_when_the_view_is_still_at_its_block_at_the_step() {
    let Some(w) = World::new("moved-tip") else {
        return;
    };
    let base = w.tip(MAIN);
    let act = w.record_act("draft", &base);
    w.ask_the_signer_then_die(&act);
    let tip_moves = w.recovery_over(
        w.signer.clone(),
        Arc::new(Moving::new(
            w.storage.clone(),
            Box::new(|_| {}),
            Box::new(|history| {
                if let Some(block) = history.blocks.first_mut() {
                    block.block_id = "e".repeat(64);
                }
            }),
        )),
    );
    let report = tip_moves.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert!(
        matches!(
            &report.outcome,
            RecoveryOutcome::Stopped {
                at: Some("end-view-created"),
                ..
            }
        ),
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), None);
}

/// The real signer for everything, and after it has completed a publication, something else happens (`then`): what
/// another process doing its own work between two steps of a recovery looks like.
struct Meddles {
    signer: Arc<Signer>,
    then: Box<dyn Fn() + Send + Sync>,
}

impl Recover for Meddles {
    fn complete_view_creation(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        creation: &ViewCreationAct,
    ) -> Result<(), inklkunn_core::RecoverError> {
        let done = self
            .signer
            .complete_view_creation(wiki, view, act, creation);
        (self.then)();
        done
    }

    fn seal_queued(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &inklkunn_core::AcceptanceToken,
    ) -> Result<(), inklkunn_core::RecoverError> {
        self.signer.seal_queued(wiki, view, act, acceptance)
    }

    fn discard_damaged(
        &self,
        wiki: &inklkunn_store::WikiId,
        act: &inklkunn_core::RecoveryToken,
    ) -> Result<inklkunn_signer::recover::text::Discarded, inklkunn_core::RecoverError> {
        self.signer.discard_damaged(wiki, act)
    }

    fn clear_locks(
        &self,
        wiki: &inklkunn_store::WikiId,
        act: &inklkunn_core::RecoveryToken,
        locks: &[String],
    ) -> Result<Vec<String>, inklkunn_core::RecoverError> {
        self.signer.clear_locks(wiki, act, locks)
    }

    fn restore_worktree(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
    ) -> Result<usize, inklkunn_core::RecoverError> {
        self.signer.restore_worktree(wiki, view, act)
    }
}

#[test]
fn an_act_that_something_else_ended_before_the_step_is_not_ended_again_or_mistaken_for_another() {
    let Some(w) = World::new("ended-elsewhere") else {
        return;
    };
    let base = w.tip(MAIN);
    let act = w.record_act("draft", &base);
    w.control("leads-after-branch-create", "");
    w.ask_the_signer_then_die(&act);
    w.uncontrol("leads-after-branch-create");
    // Between the completion and the end, the act is ended by someone else and a newer creation (not made) is
    // recorded: the step finds its own act ended, and does not take the newer one for it.
    let (store, first, wiki, reviewer) = (
        w.store.clone(),
        act.id.clone(),
        w.wiki.id.clone(),
        w.reviewer_id.clone(),
    );
    let then = move || {
        store
            .finish_view_act(&inklkunn_store::ViewActEnd {
                act: first.clone(),
                outcome: ViewActOutcome::Created,
                at: 3,
            })
            .unwrap();
        store
            .begin_view_act(&ViewActRecord {
                id: ViewActId::new(),
                wiki: wiki.clone(),
                name: "newer".to_owned(),
                forked_from: "main".to_owned(),
                from_block: "d".repeat(64),
                kind: ViewKind::Experimental,
                created_by: reviewer.clone(),
                created_at: 3,
            })
            .unwrap();
    };
    let racing = w.recovery_over(
        Arc::new(Meddles {
            signer: w.signer.clone(),
            then: Box::new(then),
        }),
        w.storage.clone(),
    );
    let report = racing.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert!(
        report
            .done
            .iter()
            .any(|d| d.step == "end-view-created" && d.detail.contains("already ended")),
        "{:#?}",
        report.lines()
    );
    assert_eq!(w.act_end(&act), Some(ViewActOutcome::Created));
    // The newer creation is not touched: it is still open, and a later recovery sees it.
    assert!(
        w.store
            .view_acts(&w.wiki.id)
            .unwrap()
            .iter()
            .any(|e| e.act.name == "newer" && e.end.is_none())
    );
}

/// `docs/RECOVERY.md`, whitespace folded to single spaces, so that a statement may wrap lines.
fn runbook() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RECOVERY.md"))
        .expect("docs/RECOVERY.md is there")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn what_the_runbook_says_of_a_killed_view_creation_is_what_prikk_and_recovery_say() {
    let Some(w) = World::new("runbook") else {
        return;
    };
    let doc = runbook();
    let act = w.record_act("draft", &w.tip(MAIN));
    w.control("leads-after-branch-create", "");
    w.ask_the_signer_then_die(&act);
    w.uncontrol("leads-after-branch-create");
    // The state the runbook describes: listed, not verifying, and `doctor` naming exactly one error with prikk's code
    // and the command the runbook names.
    assert!(w.listed().contains(&"heads/draft".to_owned()));
    assert!(!w.verifies());
    let doctor = Witnesses::new(&w.standin, Sandbox::Unconfined, w.layout.clone())
        .doctor(&w.wiki.id)
        .unwrap();
    let lead = doctor
        .diagnostics
        .iter()
        .find(|d| d.lead_ref().is_some())
        .expect("the lead");
    assert!(
        doc.contains(&format!("`{}`", lead.code)),
        "the code is named"
    );
    assert!(doc.contains("authoritative pointer leads committed ref log by one transition"));
    assert!(doc.contains("prikk ref complete heads/<name>"));
    assert!(
        lead.message
            .starts_with("authoritative pointer leads committed ref log by one transition")
    );
    assert_eq!(
        lead.recommendation.as_deref(),
        Some("run `prikk ref complete heads/draft`; doctor does not sign or append")
    );
    // The step, the stop and their words are in the runbook as recovery prints them.
    let plan = w
        .recovery
        .recover(&w.wiki, RecoveryMode::PlanOnly)
        .unwrap()
        .plan;
    for step in &plan.steps {
        if matches!(
            step.name(),
            "complete-view-creation" | "end-view-created" | "end-view-failed"
        ) {
            assert!(
                doc.contains(&format!("`{}`", step.name())),
                "{}",
                step.name()
            );
        }
    }
    for text in [
        "`end-view-created`",
        "`end-view-failed`",
        "`complete-view-creation`",
        "`STOP (view-creation-unclear)`",
        "`prikk ref complete heads/<name>`, not `seal`",
        "prikk branch list --all",
        "ref complete heads/<name> --plan-only",
    ] {
        assert!(doc.contains(text), "{text}");
    }
    assert!(
        plan.lines()
            .iter()
            .any(|l| l.contains("run ref complete heads/draft once, as prikk's doctor recommends")),
        "{:#?}",
        plan.lines()
    );
    // And the stop the runbook names is the one recovery prints for a state it cannot tell apart.
    assert_eq!(Stop::ViewCreationUnclear.as_str(), "view-creation-unclear");
    assert!(doc.contains("`view-creation-unclear`"));
    assert!(
        Stop::ViewCreationUnclear
            .meaning()
            .contains("another block than the one it was forked from")
    );
    assert!(
        doc.contains("the view is listed at **another block** than the one it was forked from")
    );
}
