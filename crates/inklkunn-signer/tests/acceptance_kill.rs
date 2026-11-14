//! THE KILL TEST (RFC 004 D-7; handoff 004 T5; CON-4, FL-10, DUR-2): the acceptance runs in a **child process that is
//! `SIGKILL`ed at each of D-4's steps in turn**, and recovery then leaves `prikk verify` green, exactly the expected
//! blocks in `log`, the acceptance record and the intent consistent with them, and the plan recorded.
//!
//! A step is named in five ways. The acceptor's own steps ([`inklkunn_core::AcceptStep`]) and the system act's
//! ([`inklkunn_core::SystemStep`]) are named by their test observers, which the child sets to stop the process there.
//! The prikk calls inside the signer's write sequence (`status`, `worktree-status`, `commit`, `seal`, `verify`) are
//! reached by a **stand-in `prikk`** (the house pattern of `creation_kill.rs`), in three ways:
//!
//! - *before*: it tells the parent it is there and sleeps until killed, **before** the real verb runs;
//! - *after* (handoff 004, P10): it runs the **real verb to completion**, then tells the parent it is there and sleeps
//!   until killed, so the kill lands with the verb's effect durable and its caller waiting for an answer that never
//!   comes (a `commit` whose patch is queued; a `seal` that sealed). Deterministic: the state is the same every time;
//! - *inside* (P10): it starts the **real verb**, waits a couple of milliseconds, tells the parent, and waits for the
//!   verb; the parent kills the child and the verb as fast as it can, so the kill lands inside prikk's own write. Not
//!   deterministic by nature (it depends on how far the verb got), so these rows assert every invariant of the matrix
//!   and read **what the kill left** from the witnesses instead of naming a plan beforehand.
//!
//! The parent then kills the child and what it held, and a fresh recovery, as the next start would run it, reads the
//! witnesses the kill left. One row kills **the recovery itself** after its `seal-queued` step has sealed (the
//! stand-in runs the real seal, then waits), and a third recovery finishes what the second left.
//!
//! The child is this test executable run again with an environment variable; it is here and not in
//! `inklkunn-core` because only this crate may spawn a process (the boundary gate). Self-skips with a visible line
//! when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use inklkunn_core::message::{Kind, SystemLabel};
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::reconcile::reconcile;
use inklkunn_core::recovery::Stop;
use inklkunn_core::{
    AcceptStep, AcceptanceParts, Authorizer, IdentityAuthority, KeyAuthority, LeaseConfig,
    PreparedFile, Principal, ProposalAcceptor, ReadConfig, ReadService, Recover, Recovery,
    RecoveryError, RecoveryMode, RecoveryOutcome, RecoveryParts, Signer, SignerConfig, StoreActs,
    StoreAuthz, StoreLedger, SystemActs, SystemRequest, SystemStep, WikiLeases, Witnesses,
    WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, Account, AccountId, AccountStore, ActStore, DisplayName,
    FileChange, IntentEnd, NewWiki, ProposalId, ProposalRecord, ProposalState, RecoveryEnding,
    RecoveryFlag, RecoveryReason, Role, RoleGrant, RoleStore, RootLayout, Slug, SqliteStore,
    WikiId, WikiRecord, WikiStore,
};

const MAIN: &str = "heads/main";
const HOME: &str = "---\ntitle: Home\nid: p-home\n---\n\nOne.\n\nTwo.\n\nThree.\n";
const OTHER: &str = "---\ntitle: Other\nid: p-other\n---\n\nHello.\n";

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

fn dir_under(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-ak-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

/// How long the stand-in lets the real verb run before it tells the parent it is inside (`inside-` rows), in seconds
/// (`INKLKUNN_AK_INSIDE` overrides it, to look at where the kill lands for other delays).
fn inside_seconds() -> String {
    std::env::var("INKLKUNN_AK_INSIDE").unwrap_or_else(|_| "0.0005".to_owned())
}

/// The stand-in `prikk`: counts its calls per verb and, when a control file names the call's index, holds it:
///
/// - `block-<verb>` = n: writes its own process id to `blocked-<verb>-<n>` and becomes a sleep until killed, **before**
///   the real verb runs;
/// - `after-<verb>` = n: runs the real verb **to completion**, then writes its id to `blocked-after-<verb>-<n>` and
///   becomes a sleep until killed (the verb's effect is durable, its caller has not been answered);
/// - `halfway-<verb>` = n (`seal` only; handoff 004, P15): runs the real verb to completion, then puts back the three
///   files of the active session that it emptied (`queue.wal`, `ref-name`, `witness`) as they were, writes its id to
///   `blocked-halfway-<verb>-<n>` and becomes a sleep until killed: the block is published and the queue is not drained,
///   which is what a `seal` killed between the two leaves, made every time and not by the luck of a delay;
/// - `inside-<verb>` = n: starts the real verb, waits [`inside_seconds`], writes its id and the verb's to
///   `blocked-inside-<verb>-<n>` and waits for the verb (the parent kills both as fast as it can).
///
/// Every other call goes to the real prikk.
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
REAL="{real}"
verb="$1"
n="$(cat "$CTL/count-$verb" 2>/dev/null || echo 0)"
echo "$((n + 1))" > "$CTL/count-$verb"
if [ -e "$CTL/block-$verb" ] && [ "$(cat "$CTL/block-$verb")" = "$n" ]; then
  echo "$$" > "$CTL/blocked-$verb-$n"
  exec sleep 3600
fi
if [ -e "$CTL/after-$verb" ] && [ "$(cat "$CTL/after-$verb")" = "$n" ]; then
  "$REAL" "$@"
  echo "$$" > "$CTL/blocked-after-$verb-$n"
  exec sleep 3600
fi
if [ -e "$CTL/halfway-$verb" ] && [ "$(cat "$CTL/halfway-$verb")" = "$n" ]; then
  A=".prikk/active/default"
  mkdir -p "$CTL/kept"
  cp "$A/queue.wal" "$A/ref-name" "$A/witness" "$CTL/kept/"
  "$REAL" "$@"
  cp "$CTL/kept/queue.wal" "$CTL/kept/ref-name" "$CTL/kept/witness" "$A/"
  echo "$$" > "$CTL/blocked-halfway-$verb-$n"
  exec sleep 3600
fi
if [ -e "$CTL/inside-$verb" ] && [ "$(cat "$CTL/inside-$verb")" = "$n" ]; then
  "$REAL" "$@" &
  real_pid=$!
  sleep {inside}
  echo "$$ $real_pid" > "$CTL/blocked-inside-$verb-$n"
  wait "$real_pid"
  exit $?
fi
exec "$REAL" "$@"
"#,
        ctl = ctl.display(),
        real = real.display(),
        inside = inside_seconds()
    );
    let path = ctl.join("prikk");
    std::fs::write(&path, script).expect("stand-in");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// What the parent and the child share: everything is on disk (a file store, a repository, a key store) and named
/// here.
struct Paths {
    root: PathBuf,
    keys: PathBuf,
    secret: PathBuf,
    shm: PathBuf,
    store: PathBuf,
    prikk: PathBuf,
}

/// The services over those paths, opened by whichever process asks (the child, and the parent for recovery).
struct Services {
    store: Arc<SqliteStore>,
    storage: Arc<PrikkCli>,
    signer: Arc<Signer>,
    leases: Arc<WikiLeases>,
    layout: RootLayout,
    reads: Arc<ReadService>,
    proposals: Arc<ProposalService>,
    witnesses: Arc<Witnesses>,
}

fn open_services(paths: &Paths, holder: &str, create: bool) -> Services {
    let layout = RootLayout::new(&paths.root);
    let store = Arc::new(SqliteStore::open(&paths.store).expect("the store"));
    let write_leases = WriteLeases::new(layout.clone());
    let mut cfg = SignerConfig::new(
        &paths.prikk,
        Sandbox::Unconfined,
        layout.clone(),
        &paths.keys,
        &paths.secret,
        &paths.shm,
    );
    cfg.cheap_derivation = true;
    let ledger = Arc::new(StoreLedger::new(store.clone(), store.clone()));
    let signer = Arc::new(
        if create {
            Signer::open_creating_store(cfg, ledger)
        } else {
            Signer::open(cfg, ledger)
        }
        .expect("signer")
        .with_leases(&write_leases),
    );
    let storage = Arc::new(
        PrikkCli::open(
            &paths.prikk,
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
        )
        .expect("prikk at the floor"),
    );
    let leases = WikiLeases::new(write_leases, store.clone(), holder, LeaseConfig::default());
    let reads = Arc::new(ReadService::new(
        storage.clone(),
        store.clone(),
        store.clone(),
        layout.clone(),
        ReadConfig::default(),
        None,
    ));
    let authorizer = Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        store.clone(),
    );
    let proposals = Arc::new(ProposalService::new(
        store.clone(),
        store.clone(),
        store.clone(),
        authorizer,
        reads.clone(),
        QuotaConfig::default(),
    ));
    let witnesses = Arc::new(Witnesses::new(
        &paths.prikk,
        Sandbox::Unconfined,
        layout.clone(),
    ));
    Services {
        store,
        storage,
        signer,
        leases,
        layout,
        reads,
        proposals,
        witnesses,
    }
}

impl Services {
    fn acceptor(&self) -> ProposalAcceptor {
        ProposalAcceptor::new(AcceptanceParts {
            proposals: self.proposals.clone(),
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            roles: self.store.clone(),
            accounts: self.store.clone(),
            storage: self.storage.clone(),
            layout: self.layout.clone(),
            signer: self.signer.clone(),
            leases: self.leases.clone(),
            reads: self.reads.clone(),
        })
        .with_audit(self.store.clone())
    }

    fn system_acts(&self) -> SystemActs {
        SystemActs::new(
            Authorizer::new(
                Arc::new(StoreAuthz::new(self.store.clone(), self.store.clone())),
                self.store.clone(),
            ),
            self.store.clone(),
            self.store.clone(),
            self.storage.clone(),
            self.layout.clone(),
            self.signer.clone(),
            self.leases.clone(),
        )
        .with_audit(self.store.clone())
    }

    fn recovery(&self) -> Recovery {
        Recovery::new(RecoveryParts {
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            proposals: self.store.clone(),
            storage: self.storage.clone(),
            witnesses: self.witnesses.clone(),
            signer: self.signer.clone() as Arc<dyn Recover>,
            leases: self.leases.clone(),
            layout: self.layout.clone(),
            audit: self.store.clone(),
        })
        .with_reads(self.reads.clone())
    }
}

/// The summary of the system act the child makes, which finds it again in the records.
const SYSTEM_SUMMARY: &str = "Add a system page";

/// The child: do what the spec says (accept the proposal it names, make a system act, or run a recovery) and stop
/// where the spec says. It has no way to finish.
#[test]
fn child_accepts_and_is_stopped() {
    let Some(spec) = std::env::var_os("INKLKUNN_AK_CHILD") else {
        return; // an ordinary run of the suite: not the child
    };
    let spec: serde_json::Value = serde_json::from_str(&spec.to_string_lossy()).unwrap();
    let text = |k: &str| spec.get(k).and_then(|v| v.as_str()).unwrap().to_owned();
    let paths = Paths {
        root: text("root").into(),
        keys: text("keys").into(),
        secret: text("secret").into(),
        shm: text("shm").into(),
        store: text("store").into(),
        prikk: text("prikk").into(),
    };
    let s = open_services(&paths, &format!("child-{}", std::process::id()), false);
    let ctl = PathBuf::from(text("ctl"));
    let step = spec.get("step").and_then(|v| v.as_str()).map(str::to_owned);
    let wiki = s.store.get(&WikiId::parse(&text("wiki")).unwrap()).unwrap();
    // Stop at the step the spec names: write the marker and sleep.
    let stop_at = move |name: &'static str, step: Option<String>, ctl: PathBuf| {
        if step.as_deref() == Some(name) {
            std::fs::write(ctl.join("reached"), name).unwrap();
            std::thread::sleep(Duration::from_secs(3600));
        }
    };
    let outcome = match text("mode").as_str() {
        "accept" => {
            let mut acceptor = s.acceptor();
            if step.is_some() {
                let (ctl, step) = (ctl.clone(), step.clone());
                acceptor = acceptor.with_observer(Arc::new(move |at: AcceptStep| {
                    stop_at(at.as_str(), step.clone(), ctl.clone());
                }));
            }
            let who =
                Principal::session(authority(), AccountId::parse(&text("maintainer")).unwrap());
            format!(
                "{:?}",
                acceptor.accept(
                    &who,
                    &wiki,
                    &ProposalId::parse(&text("proposal")).unwrap(),
                    1
                )
            )
        }
        "system" => {
            let mut acts = s.system_acts();
            if step.is_some() {
                let (ctl, step) = (ctl.clone(), step.clone());
                acts = acts.with_observer(Arc::new(move |at: SystemStep| {
                    stop_at(at.as_str(), step.clone(), ctl.clone());
                }));
            }
            format!(
                "{:?}",
                acts.run(&wiki, &SystemLabel::inklkunn(), system_request())
            )
        }
        "recover" => format!("{:?}", s.recovery().recover(&wiki, RecoveryMode::Run)),
        other => panic!("unknown mode {other}"),
    };
    // Reaching here means the child was not stopped: tell the parent, which treats it as a failure of the test.
    std::fs::write(ctl.join("finished"), outcome).unwrap();
}

/// The system act the child makes: one page, written through the service's own path.
fn system_request() -> SystemRequest {
    SystemRequest {
        kind: inklkunn_store::SystemActKind::System,
        summary: SYSTEM_SUMMARY.to_owned(),
        files: vec![PreparedFile {
            path: PathBuf::from("pages/Sys.md"),
            content: Some(b"---\ntitle: Sys\nid: p-sys\n---\n\nA system page.\n".to_vec()),
        }],
        renames: Vec::new(),
        imported_from: None,
        planned_against: None,
    }
}

/// Which act the child makes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Who {
    /// A proposal's acceptance.
    Acceptance,
    /// A change the service makes for itself.
    System,
}

impl Who {
    fn label(self) -> &'static str {
        match self {
            Who::Acceptance => "acceptance",
            Who::System => "system act",
        }
    }

    fn mode(self) -> &'static str {
        match self {
            Who::Acceptance => "accept",
            Who::System => "system",
        }
    }
}

/// How a kill point is named.
#[derive(Clone, Copy)]
enum At {
    /// One of the acceptor's steps (D-4), by the observer.
    Step(AcceptStep),
    /// One of the system act's steps, by the observer.
    SysStep(SystemStep),
    /// The `n`-th call of a prikk verb since the child began (0-based), held by the stand-in **before** it runs.
    Prikk(&'static str, u32),
    /// The same call, held **after** the real verb ran to completion: its effect is durable, its caller waits.
    After(&'static str, u32),
    /// The same call, killed a couple of milliseconds **inside** the real verb.
    Inside(&'static str, u32),
    /// The same call, held after the real `seal` ran and the queue was put back: the block is published and the queue
    /// is not drained (P15).
    Halfway(&'static str, u32),
    /// The acceptance is killed before the seal (a patch is queued); then **recovery** is killed after its
    /// `seal-queued` step sealed (the real seal ran to completion), before any record of it was written.
    RecoveryAfterSeal,
}

impl At {
    fn label(self) -> String {
        match self {
            At::Step(s) => s.as_str().to_owned(),
            At::SysStep(s) => s.as_str().to_owned(),
            At::Prikk(verb, n) => format!("prikk {verb} #{n}"),
            At::After(verb, n) => format!("prikk {verb} #{n} done"),
            At::Inside(verb, n) => format!("inside prikk {verb} #{n}"),
            At::Halfway(verb, n) => format!("prikk {verb} #{n} published, queue not drained"),
            At::RecoveryAfterSeal => "recovery, after seal-queued".to_owned(),
        }
    }
}

/// What recovery must have made of a kill at one point.
struct Expect {
    /// The steps of the recorded plan, in order (`None` for a kill inside a real verb: whatever the state was).
    steps: Option<&'static [&'static str]>,
    /// The change is in the history afterwards (one block more). `None` for a kill inside a real verb: read from the
    /// log, and every invariant is checked against what is found.
    sealed: Option<bool>,
    /// An acceptance record (or system act record) exists afterwards (the kill came after it was written).
    record: bool,
    /// How many recovery acts are recorded afterwards: one, or two when a recovery was itself killed.
    recoveries: usize,
}

const TAIL: [&str; 3] = ["verify", "reconcile", "clear-flag"];

const fn fixed(steps: &'static [&'static str], sealed: bool, record: bool) -> Expect {
    Expect {
        steps: Some(steps),
        sealed: Some(sealed),
        record,
        recoveries: 1,
    }
}

const FAILED: &[&str] = &["end-failed", "verify", "reconcile", "clear-flag"];
const RESTORED: &[&str] = &[
    "restore-worktree",
    "end-failed",
    "verify",
    "reconcile",
    "clear-flag",
];
const QUEUED: &[&str] = &[
    "seal-queued",
    "end-sealed",
    "verify",
    "reconcile",
    "clear-flag",
];
const SEALED: &[&str] = &["end-sealed", "verify", "reconcile", "clear-flag"];
/// A `seal` that published its block and did not drain the queue (P15): prikk's own recommendation, then the rest.
const PUBLISHED: &[&str] = &[
    "complete-publication",
    "restore-worktree",
    "end-sealed",
    "verify",
    "reconcile",
    "clear-flag",
];

/// The matrix: D-4's steps in turn, then the prikk calls of the signer's write sequence between the intent and the
/// signer's return (before the call, after the real call, and inside it), then the system act's, then a recovery
/// that is itself killed.
fn matrix() -> Vec<(Who, At, Expect)> {
    use Who::{Acceptance, System};
    vec![
        // 1 – 2: only the lease holder's record exists; nothing else was written.
        (
            Acceptance,
            At::Step(AcceptStep::LeaseTaken),
            fixed(&TAIL, false, false),
        ),
        (
            Acceptance,
            At::Step(AcceptStep::Prepared),
            fixed(&TAIL, false, false),
        ),
        // 4: the acceptance record is written and the intent is not: the signer was never asked.
        (
            Acceptance,
            At::Step(AcceptStep::AcceptanceRecorded),
            fixed(FAILED, false, true),
        ),
        // The intent is written; the signer has not been asked, or was asked and had done nothing yet.
        (
            Acceptance,
            At::Step(AcceptStep::IntentRecorded),
            fixed(FAILED, false, true),
        ),
        (
            Acceptance,
            At::Prikk("status", 0),
            fixed(FAILED, false, true),
        ),
        (
            Acceptance,
            At::Prikk("worktree-status", 0),
            fixed(FAILED, false, true),
        ),
        // The files are in the worktree and `commit` has not run: the worktree is put back.
        (
            Acceptance,
            At::Prikk("commit", 0),
            fixed(RESTORED, false, true),
        ),
        // `commit` ran to completion and its caller was never answered (a deadline after the patch is durable, P1):
        // a patch is queued, and recovery completes it with `seal` alone.
        (
            Acceptance,
            At::After("commit", 0),
            fixed(QUEUED, true, true),
        ),
        // Committed, not sealed: a patch is queued, and recovery completes it with `seal` alone.
        (Acceptance, At::Prikk("seal", 0), fixed(QUEUED, true, true)),
        // Sealed in prikk, the caller never answered, none of it recorded.
        (Acceptance, At::After("seal", 0), fixed(SEALED, true, true)),
        // The block is published and the queue is not drained (what a `seal` killed between the two leaves, ~3 % of
        // the `inside` runs, made every time): `status` refuses, `doctor` names the pending cleanup, and recovery
        // finishes it with prikk's own recommendation (P15).
        (
            Acceptance,
            At::Halfway("seal", 0),
            fixed(PUBLISHED, true, true),
        ),
        // Sealed in prikk, none of it recorded.
        (
            Acceptance,
            At::Prikk("verify", 0),
            fixed(SEALED, true, true),
        ),
        (
            Acceptance,
            At::Step(AcceptStep::SignerReturned),
            fixed(SEALED, true, true),
        ),
        // The acceptance's end is written; the intent's and the proposal's are not.
        (
            Acceptance,
            At::Step(AcceptStep::AcceptanceSealed),
            fixed(SEALED, true, true),
        ),
        // The intent is complete; the proposal is not yet accepted.
        (
            Acceptance,
            At::Step(AcceptStep::IntentCompleted),
            fixed(SEALED, true, true),
        ),
        // Every record is written; the caches, the reconciliation and the lease are what was left.
        (
            Acceptance,
            At::Step(AcceptStep::ProposalAccepted),
            fixed(&TAIL, true, true),
        ),
        (
            Acceptance,
            At::Step(AcceptStep::Reconciled),
            fixed(&TAIL, true, true),
        ),
        // The kill lands inside the real `commit` and the real `seal` (P10): whichever way they had got, the
        // invariants hold.
        (
            Acceptance,
            At::Inside("commit", 0),
            Expect {
                steps: None,
                sealed: None,
                record: true,
                recoveries: 1,
            },
        ),
        (
            Acceptance,
            At::Inside("seal", 0),
            Expect {
                steps: None,
                sealed: None,
                record: true,
                recoveries: 1,
            },
        ),
        // A recovery killed after its `seal-queued` step: the patch is sealed, none of it recorded, a recovery act
        // begun and never ended. The next recovery finishes what is left (and ends the one that was interrupted).
        (
            Acceptance,
            At::RecoveryAfterSeal,
            Expect {
                steps: Some(SEALED),
                sealed: Some(true),
                record: true,
                recoveries: 2,
            },
        ),
        // ---- the system act: the same path, with a record of its own and no proposal.
        (
            System,
            At::SysStep(SystemStep::LeaseTaken),
            fixed(&TAIL, false, false),
        ),
        (
            System,
            At::SysStep(SystemStep::ActRecorded),
            fixed(FAILED, false, true),
        ),
        (System, At::Prikk("status", 0), fixed(FAILED, false, true)),
        (
            System,
            At::Prikk("worktree-status", 0),
            fixed(FAILED, false, true),
        ),
        (System, At::Prikk("commit", 0), fixed(RESTORED, false, true)),
        (System, At::After("commit", 0), fixed(QUEUED, true, true)),
        (System, At::Prikk("seal", 0), fixed(QUEUED, true, true)),
        (System, At::After("seal", 0), fixed(SEALED, true, true)),
        (System, At::Halfway("seal", 0), fixed(PUBLISHED, true, true)),
        (System, At::Prikk("verify", 0), fixed(SEALED, true, true)),
        (
            System,
            At::SysStep(SystemStep::SignerReturned),
            fixed(SEALED, true, true),
        ),
        // The act's end is written; the caches, the reconciliation and the lease are what was left.
        (
            System,
            At::SysStep(SystemStep::ActEnded),
            fixed(&TAIL, true, true),
        ),
        (
            System,
            At::SysStep(SystemStep::Reconciled),
            fixed(&TAIL, true, true),
        ),
        (
            System,
            At::Inside("commit", 0),
            Expect {
                steps: None,
                sealed: None,
                record: true,
                recoveries: 1,
            },
        ),
        (
            System,
            At::Inside("seal", 0),
            Expect {
                steps: None,
                sealed: None,
                record: true,
                recoveries: 1,
            },
        ),
    ]
}

/// A wiki of two pages, a maintainer and a contributor, and two proposals: `first` (the one the child accepts) and
/// `second` (which proves the wiki takes writes after recovery).
struct Fixture {
    _tmp: Dir,
    _shm: Dir,
    ctl: PathBuf,
    paths: Paths,
    wiki: WikiRecord,
    repo: PathBuf,
    maintainer: AccountId,
    first: ProposalRecord,
    second: ProposalRecord,
    base_blocks: usize,
    /// The real prikk, whatever stands in front of it.
    real: PathBuf,
}

fn fixture(tag: &str) -> Option<Fixture> {
    let Some(real) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    fixture_over(tag, real)
}

/// The first `prikk` on `PATH` (in any directory) whose version satisfies `wanted`.
fn prikk_where(wanted: impl Fn((u64, u64, u64)) -> bool) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .filter(|c| c.is_file())
        .find(|c| inklkunn_prikk::probe_version(c.as_os_str()).is_some_and(&wanted))
}

/// The fixture over the first prikk 0.57.0 or newer on `PATH`, wherever it stands (maintenance M28).
fn fixture_057(tag: &str) -> Option<Fixture> {
    let Some(real) = prikk_where(|v| v >= (0, 57, 0)) else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return None;
    };
    fixture_over(tag, real)
}

/// The fixture over the **first** prikk on `PATH`, when it is older than 0.57.0 (CI's is the floor): the answer *no
/// verb*. A first prikk that has the verb prints the line CI fails on.
fn fixture_floor(tag: &str) -> Option<Fixture> {
    let Some(real) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    if inklkunn_prikk::probe_version(real.as_os_str()).is_none_or(|v| v >= (0, 57, 0)) {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return None;
    }
    fixture_over(tag, real)
}

fn fixture_over(tag: &str, real: PathBuf) -> Option<Fixture> {
    if !Path::new("/dev/shm").is_dir() {
        println!("skipping: no /dev/shm here");
        return None;
    }
    let tmp = dir_under(&std::env::temp_dir(), tag);
    let shm = dir_under(Path::new("/dev/shm"), tag);
    let ctl = tmp.0.join("ctl");
    std::fs::create_dir_all(&ctl).unwrap();
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests\n").unwrap();
    let paths = Paths {
        root: tmp.0.join("root"),
        keys: tmp.0.join("keys"),
        secret,
        shm: shm.0.clone(),
        store: tmp.0.join("store.sqlite"),
        prikk: write_standin(&ctl, &real),
    };
    let s = open_services(&paths, "parent", true);
    let created = create_wiki(
        &*s.store,
        &*s.store,
        &s.layout,
        &*s.signer,
        &*s.storage,
        NewWiki::new(
            Slug::new(tag).unwrap(),
            DisplayName::new(&format!("Wiki {tag}")).unwrap(),
        ),
        "0.0.0",
        &KeyAuthority::for_tests(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let wiki = created.record.clone();
    // The creation is a system change the records know (the reconciliation needs it to call the first block recorded).
    let message = inklkunn_core::creation_message(&wiki);
    let act = inklkunn_store::SystemActRecord {
        id: inklkunn_store::SystemActId::new(),
        wiki: wiki.id.clone(),
        view: wiki.default_view.clone(),
        kind: inklkunn_store::SystemActKind::System,
        base_block: created.first_block_id.clone(),
        message_digest: inklkunn_core::acceptance::message_digest(&message),
        message,
        digest: "0".repeat(64),
        bytes: 0,
        created_at: 1,
    };
    s.store.begin_system_act(&act).unwrap();
    s.store
        .finish_system_act(&inklkunn_store::SystemActEnd {
            act: act.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: created.first_block_id.clone(),
                patch_id: "0".repeat(64),
            },
            at: 2,
        })
        .unwrap();
    let person = |name: &str, role: Role| {
        let id = AccountId::new();
        s.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(name).unwrap(),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap();
        s.store
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
    let maintainer = person("Maya Maintainer", Role::Maintainer);
    let contributor = person("Noor Contributor", Role::Contributor);
    // The two pages, through the service's own path (one accepted change under the system principal).
    let acts = SystemActs::new(
        Authorizer::new(
            Arc::new(StoreAuthz::new(s.store.clone(), s.store.clone())),
            s.store.clone(),
        ),
        s.store.clone(),
        s.store.clone(),
        s.storage.clone(),
        s.layout.clone(),
        s.signer.clone(),
        s.leases.clone(),
    );
    let folder = dir_under(&std::env::temp_dir(), &format!("{tag}-folder"));
    for (path, text) in [("pages/Home.md", HOME), ("pages/Other.md", OTHER)] {
        let p = folder.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let have =
        inklkunn_core::import::wiki_state(&*s.storage, &s.layout, &wiki, &ReadConfig::default())
            .unwrap();
    let plan = inklkunn_core::import::plan_folder(&folder.0, &have, false, &ReadConfig::default())
        .unwrap();
    inklkunn_core::import::run_import(&acts, &wiki, &plan).unwrap_or_else(|e| panic!("{e}"));
    let propose = |summary: &str, path: &str, text: String| -> ProposalRecord {
        let edit = ProposalEdit {
            summary: summary.to_owned(),
            files: vec![FileChange {
                path: path.to_owned(),
                text: Some(text),
            }],
            ..ProposalEdit::default()
        };
        let who = Principal::session(authority(), contributor.clone());
        let draft = s.proposals.create(&who, &wiki, None, edit).unwrap();
        s.proposals.submit(&who, &wiki, &draft.id).unwrap()
    };
    let first = propose(
        "Say it in Spanish",
        "pages/Home.md",
        HOME.replace("One.", "Uno."),
    );
    let second = propose(
        "Greet in Spanish",
        "pages/Other.md",
        OTHER.replace("Hello.", "Hola."),
    );
    let repo = s.layout.repo_dir(&wiki.id);
    let base_blocks = s
        .storage
        .history(&repo, &Point::View(MAIN.to_owned()), None)
        .unwrap()
        .blocks
        .len();
    Some(Fixture {
        _tmp: tmp,
        _shm: shm,
        ctl,
        paths,
        wiki,
        repo,
        maintainer,
        first,
        second,
        base_blocks,
        real,
    })
}

impl Fixture {
    /// The spec of a child that makes `mode`'s act (or runs a recovery) and stops at `step`.
    fn spec(&self, mode: &str, step: Option<String>) -> serde_json::Value {
        serde_json::json!({
            "root": self.paths.root, "keys": self.paths.keys, "secret": self.paths.secret,
            "shm": self.paths.shm, "store": self.paths.store, "prikk": self.paths.prikk,
            "ctl": self.ctl, "wiki": self.wiki.id.as_str(), "maintainer": self.maintainer.as_str(),
            "proposal": self.first.id.as_str(), "step": step, "mode": mode,
        })
    }

    fn clear_ctl(&self) {
        for entry in std::fs::read_dir(&self.ctl).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != "prikk" {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
    }

    /// Run a child with `spec` until `marker` appears, then kill **its whole process group** (the child, the stand-in
    /// prikk it was waiting for, the real verb that stand-in started, a sleep) in one signal, so that nothing the
    /// child had begun goes on to write after it is gone. Returns once nothing of the group is left.
    ///
    /// The signal is sent by a shell that is already running and waiting on its input (`kill` is a builtin there): a
    /// kill inside a real verb is as late as the signal is slow.
    fn run_and_kill(&self, spec: &serde_json::Value, marker: &Path, label: &str) {
        use std::io::Write as _;
        use std::os::unix::process::CommandExt as _;
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "child_accepts_and_is_stopped",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("INKLKUNN_AK_CHILD", spec.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Its own process group, the child its leader: the group is the child and what it started.
            .process_group(0)
            .spawn()
            .expect("child");
        let group = child.id();
        assert!(group > 1, "{label}: a group to kill");
        let group_text = group.to_string();
        let mut killer = Command::new("sh")
            .args(["-c", "read _; kill -s KILL -- \"-$0\"", &group_text])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the shell that sends the signal");
        let deadline = Instant::now() + Duration::from_secs(120);
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "{label}: the child never reached the kill point"
            );
            if let Ok(Some(status)) = child.try_wait() {
                let finished =
                    std::fs::read_to_string(self.ctl.join("finished")).unwrap_or_default();
                // The group may still hold what the child started; it is not left to run.
                let _ = Command::new("kill")
                    .args(["-s", "KILL", "--", &format!("-{group}")])
                    .status();
                panic!("{label}: the child ended ({status}) without being stopped: {finished}");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        {
            let mut input = killer.stdin.take().expect("the shell's input");
            let _ = input.write_all(b"\n");
        }
        let _ = killer.wait();
        let _ = child.kill();
        let _ = child.wait();
        // Nothing of the group is left: signal 0 reaches a group that still has a member.
        let alive = || {
            Command::new("kill")
                .args(["-s", "0", "--", &format!("-{group}")])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        let until = Instant::now() + Duration::from_secs(10);
        while alive() {
            assert!(
                Instant::now() < until,
                "{label}: the process group is still there"
            );
            let _ = Command::new("kill")
                .args(["-s", "KILL", "--", &format!("-{group}")])
                .stderr(Stdio::null())
                .status();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Run the child that makes `who`'s act to the kill point and kill it.
    fn kill_at(&self, who: Who, at: At) {
        self.clear_ctl();
        let (step, marker) = match at {
            At::Step(s) => (Some(s.as_str().to_owned()), self.ctl.join("reached")),
            At::SysStep(s) => (Some(s.as_str().to_owned()), self.ctl.join("reached")),
            At::Prikk(verb, n) => {
                std::fs::write(self.ctl.join(format!("block-{verb}")), n.to_string()).unwrap();
                (None, self.ctl.join(format!("blocked-{verb}-{n}")))
            }
            At::After(verb, n) => {
                std::fs::write(self.ctl.join(format!("after-{verb}")), n.to_string()).unwrap();
                (None, self.ctl.join(format!("blocked-after-{verb}-{n}")))
            }
            At::Inside(verb, n) => {
                std::fs::write(self.ctl.join(format!("inside-{verb}")), n.to_string()).unwrap();
                (None, self.ctl.join(format!("blocked-inside-{verb}-{n}")))
            }
            At::Halfway(verb, n) => {
                std::fs::write(self.ctl.join(format!("halfway-{verb}")), n.to_string()).unwrap();
                (None, self.ctl.join(format!("blocked-halfway-{verb}-{n}")))
            }
            At::RecoveryAfterSeal => unreachable!("a recovery is killed by `kill_recovery`"),
        };
        let label = format!("{}: {}", who.label(), at.label());
        self.run_and_kill(&self.spec(who.mode(), step), &marker, &label);
        // Nothing of the kill may go on to write: the stand-in's hold is a pass-through again.
        for verb in ["block", "after", "halfway", "inside"] {
            for name in ["status", "worktree-status", "commit", "seal", "verify"] {
                let _ = std::fs::remove_file(self.ctl.join(format!("{verb}-{name}")));
            }
        }
    }

    /// Kill the **recovery**: run it in a child, with the next `seal` held after the real seal ran to completion.
    /// Call it after `kill_at` has left a patch queued (the acceptance killed at its `seal`).
    fn kill_recovery(&self) {
        // The stand-in counts its calls per verb across processes: the next `seal` is the one it has counted.
        let next: u32 = std::fs::read_to_string(self.ctl.join("count-seal"))
            .expect("the acceptance's seal was counted")
            .trim()
            .parse()
            .unwrap();
        std::fs::write(self.ctl.join("after-seal"), next.to_string()).unwrap();
        let marker = self.ctl.join(format!("blocked-after-seal-{next}"));
        let _ = std::fs::remove_file(self.ctl.join("finished"));
        self.run_and_kill(
            &self.spec("recover", None),
            &marker,
            "recovery, after seal-queued",
        );
        let _ = std::fs::remove_file(self.ctl.join("after-seal"));
    }
}

fn log(s: &Services, repo: &Path) -> Vec<inklkunn_prikk::HistoryBlock> {
    s.storage
        .history(repo, &Point::View(MAIN.to_owned()), None)
        .unwrap()
        .blocks
}

/// Keep a copy of a repository a kill left in a state recovery stops at or fails closed on, to study it afterwards:
/// when `INKLKUNN_AK_KEEP` names a directory, the copy goes to `<that directory>/<label with no spaces>`.
fn keep_for_study(repo: &Path, label: &str) {
    let Some(dir) = std::env::var_os("INKLKUNN_AK_KEEP") else {
        return;
    };
    let name: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let to = PathBuf::from(dir).join(name);
    let _ = std::fs::remove_dir_all(&to);
    let _ = Command::new("cp").arg("-r").arg(repo).arg(&to).status();
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

/// What the kill left in prikk, read before recovery looked: the three states a kill of `commit` or `seal` can leave.
fn left_by_the_kill(base: usize, killed: usize, queued: u64) -> &'static str {
    if killed > base {
        "a sealed block"
    } else if queued > 0 {
        "a queued patch"
    } else {
        "nothing in the log"
    }
}

#[test]
fn a_sigkill_at_each_step_of_an_acceptance_is_recovered_with_nothing_lost_or_made_twice() {
    if std::env::var_os("INKLKUNN_AK_CHILD").is_some() {
        return; // the child runs `child_accepts_and_is_stopped`, not this test
    }
    let mut table = Vec::new();
    for (who, at, expect) in matrix() {
        let Some(f) = fixture(&format!("k{}", table.len())) else {
            return;
        };
        let label = format!("{}: {}", who.label(), at.label());
        match at {
            // The acceptance dies at its seal (a patch is queued); then the recovery that finds it dies after sealing.
            At::RecoveryAfterSeal => {
                f.kill_at(who, At::Prikk("seal", 0));
                f.kill_recovery();
            }
            other => f.kill_at(who, other),
        }

        // What the next start finds: a wiki that needs recovery, by its witnesses, in a process that knows nothing
        // of the one that died.
        let s = open_services(&f.paths, "restarted", false);
        let recovery = s.recovery();
        let reasons = recovery.needs_recovery(&f.wiki.id).unwrap();
        assert!(
            !reasons.is_empty(),
            "{label}: the kill left no witness at all"
        );
        let blocks_killed = log(&s, &f.repo);
        let timed = expect.steps.is_none();
        // A kill inside a real verb can leave prikk in a state its own `status` refuses (an integrity error); the
        // rows that kill at a step or between verbs never do.
        let status_killed = s.storage.status(&f.repo);
        let queued_killed = status_killed.as_ref().map_or(0, |st| st.queue.count);
        let left = left_by_the_kill(f.base_blocks, blocks_killed.len(), queued_killed);
        // ... and that refusal has one cause recovery answers (handoff 004, P15): a `seal` killed after it published
        // its block and before it drained the queue, which `doctor` names (`PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`)
        // and recovery finishes with prikk's own recommendation. Such a row goes on as any other, with every invariant
        // asserted; the kill matrix's `inside prikk seal` rows land here about three runs in a hundred, and the
        // `published, queue not drained` rows land here every time.
        let cleanup_pending = status_killed.is_err()
            && s.witnesses
                .doctor(&f.wiki.id)
                .is_ok_and(|d| d.is_only_cleanup_pending());
        if let (Err(e), false) = (&status_killed, cleanup_pending) {
            assert!(
                timed,
                "{label}: prikk status refuses a state no step-kill makes: {e}"
            );
            // Recovery cannot read a witness and fails closed, in a plan and in a run, changing nothing.
            for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
                let r = recovery.recover(&f.wiki, mode);
                assert!(
                    matches!(&r, Err(RecoveryError::Witness(_))),
                    "{label}: recovery must fail closed on a status prikk refuses: {r:?}"
                );
            }
            assert_eq!(
                log(&s, &f.repo),
                blocks_killed,
                "{label}: nothing was written"
            );
            assert!(
                !recovery.needs_recovery(&f.wiki.id).unwrap().is_empty(),
                "{label}: the wiki stays flagged"
            );
            keep_for_study(&f.repo, &label);
            table.push((
                label,
                "no outcome: prikk status itself refuses (integrity error); recovery fails closed, nothing changed, a person decides".to_owned(),
                String::new(),
            ));
            continue;
        }
        // The plan, shown first, changes nothing and is the plan the run then records.
        //
        // A kill inside a real verb can land at the instant prikk creates a lock file, before it has written a word
        // to it: `prikk unlock` then lists a lock of no kind and a pid that is "(unparseable)". Recovery's reader
        // refuses a shape it does not know, so the recovery **fails closed**: it reports that it could not read a
        // witness, changes nothing, and the wiki stays flagged. That is the right answer for this state (what that
        // lock belongs to is not known), and the operator's way out is to clear the empty lock file; the row then
        // goes on as any other, with every invariant asserted. (Only the rows that kill inside a verb can see it.)
        let mut failed_closed = false;
        let shown = match recovery.recover(&f.wiki, RecoveryMode::PlanOnly) {
            Ok(shown) => shown,
            Err(RecoveryError::Witness(why))
                if timed && why.contains("a recorded pid is not a number") =>
            {
                failed_closed = true;
                let run = recovery.recover(&f.wiki, RecoveryMode::Run);
                assert!(
                    matches!(&run, Err(RecoveryError::Witness(_))),
                    "{label}: a run must fail closed too: {run:?}"
                );
                assert_eq!(
                    log(&s, &f.repo),
                    blocks_killed,
                    "{label}: nothing was written"
                );
                assert!(
                    !recovery.needs_recovery(&f.wiki.id).unwrap().is_empty(),
                    "{label}: the wiki stays flagged"
                );
                assert!(
                    clear_empty_locks(&f.repo.join(".prikk")) > 0,
                    "{label}: the torn lock is an empty file ({why})"
                );
                recovery.recover(&f.wiki, RecoveryMode::PlanOnly).unwrap()
            }
            Err(e) => panic!("{label}: {e}"),
        };
        // A kill inside a real verb can also leave a state recovery has no answer for, and **names** that: the plan
        // stops. The recovery changes nothing, the wiki stays flagged, a person decides. (A `doctor` warning about stale
        // reference metadata, which asks for nothing, when `seal` was killed just after it published the block, is no
        // longer such a state: recovery notes it and goes on, M16.)
        if timed && let Some(stop) = shown.plan.stop {
            let run = recovery
                .recover(&f.wiki, RecoveryMode::Run)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            assert!(
                matches!(&run.outcome, RecoveryOutcome::Stopped { at: None, .. }),
                "{label}: {:#?}",
                run.lines()
            );
            assert_eq!(run.plan.stop, Some(stop), "{label}");
            assert!(run.done.is_empty(), "{label}: a stopped plan takes no step");
            assert_eq!(
                log(&s, &f.repo),
                blocks_killed,
                "{label}: nothing was written"
            );
            assert_eq!(
                s.storage.status(&f.repo).unwrap().queue.count,
                queued_killed,
                "{label}: nothing was sealed or discarded"
            );
            assert!(
                s.store.recovery_flag(&f.wiki.id).unwrap().is_some(),
                "{label}: the wiki stays flagged"
            );
            let recorded = s.store.recovery_acts(&f.wiki.id).unwrap();
            assert_eq!(
                recorded
                    .last()
                    .and_then(|a| a.end.as_ref())
                    .map(|e| e.ending),
                Some(RecoveryEnding::Stopped),
                "{label}: the stop is recorded"
            );
            keep_for_study(&f.repo, &label);
            table.push((
                label,
                format!(
                    "STOPPED, a person decides ({}); the kill left {left}; nothing changed",
                    stop.as_str()
                ),
                String::new(),
            ));
            continue;
        }
        let shown_steps: Vec<&str> = shown.plan.steps.iter().map(|x| x.name()).collect();
        if let Some(steps) = expect.steps {
            assert_eq!(shown_steps, steps, "{label}: the plan shown");
        } else {
            // A kill inside a real verb: the plan is whatever the state was, and it must be the right one for it.
            let has = |name: &str| shown_steps.contains(&name);
            match left {
                "a sealed block" => assert!(
                    has("end-sealed") && !has("seal-queued"),
                    "{label}: {shown_steps:?}"
                ),
                "a queued patch" => assert!(
                    has("seal-queued") && has("end-sealed"),
                    "{label}: {shown_steps:?}"
                ),
                _ => assert!(
                    has("end-failed") && !has("end-sealed"),
                    "{label}: {shown_steps:?}"
                ),
            }
            assert!(shown.plan.stop.is_none(), "{label}: {:#?}", shown.lines());
        }
        let earlier = usize::from(matches!(at, At::RecoveryAfterSeal));
        assert_eq!(
            s.store.recovery_acts(&f.wiki.id).unwrap().len(),
            earlier,
            "{label}: a plan shown is not recorded (only the killed recovery's own act is there)"
        );

        let report = recovery
            .recover(&f.wiki, RecoveryMode::Run)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(
            report.outcome,
            RecoveryOutcome::Recovered,
            "{label}: {:#?}",
            report.lines()
        );
        let ran: Vec<&str> = report.plan.steps.iter().map(|x| x.name()).collect();
        assert_eq!(ran, shown_steps, "{label}: the plan run is the plan shown");

        // `prikk verify` is green, and exactly the expected blocks are in the log.
        assert!(
            s.storage.verify(&f.repo).unwrap().verdict.ok,
            "{label}: verify"
        );
        let blocks = log(&s, &f.repo);
        let sealed = expect.sealed.unwrap_or(blocks.len() > f.base_blocks);
        let expected_len = f.base_blocks + usize::from(sealed);
        assert_eq!(blocks.len(), expected_len, "{label}: the blocks in the log");
        if !sealed {
            assert_eq!(
                blocks, blocks_killed,
                "{label}: the history is as the kill left it"
            );
        }
        assert_eq!(
            s.storage.status(&f.repo).unwrap().queue.count,
            0,
            "{label}: nothing is left queued"
        );

        // The records agree with the log.
        assert!(
            s.store.open_intents(&f.wiki.id).unwrap().is_empty(),
            "{label}: an intent is still open"
        );
        assert!(
            s.store.pending_acceptances(&f.wiki.id).unwrap().is_empty(),
            "{label}: an acceptance is still pending"
        );
        assert!(
            s.store
                .system_acts(&f.wiki.id)
                .unwrap()
                .iter()
                .all(|e| e.end.is_some()),
            "{label}: a system act has no end"
        );
        let newest_message = |block: &inklkunn_prikk::HistoryBlock| {
            inklkunn_core::message::Message::parse(&block.patch_messages.first().unwrap().message)
                .unwrap()
        };
        match who {
            Who::Acceptance => {
                let entries = s.store.acceptances_of(&f.first.id, 1).unwrap();
                assert_eq!(
                    entries.len(),
                    usize::from(expect.record),
                    "{label}: acceptance records"
                );
                let proposal = s
                    .proposals
                    .get(
                        &Principal::session(authority(), f.maintainer.clone()),
                        &f.wiki,
                        &f.first.id,
                    )
                    .unwrap();
                if sealed {
                    let newest = blocks.first().unwrap();
                    assert_eq!(
                        newest_message(newest).proposal_id(),
                        Some(&f.first.id),
                        "{label}: the newest block is the proposal's"
                    );
                    let entry = entries.first().unwrap();
                    assert!(
                        matches!(entry.result.as_ref().map(|r| &r.outcome), Some(AcceptanceOutcome::Sealed { block_id, .. }) if *block_id == newest.block_id),
                        "{label}: the acceptance names the block: {:?}",
                        entry.result
                    );
                    let intent = s.store.intents(&f.wiki.id).unwrap();
                    assert_eq!(
                        intent.last().unwrap().completion.as_ref().map(|c| c.end),
                        Some(IntentEnd::Completed),
                        "{label}"
                    );
                    assert_eq!(proposal.state, ProposalState::Accepted, "{label}");
                } else {
                    assert_eq!(
                        proposal.state,
                        ProposalState::HeldForReview,
                        "{label}: the proposal stays open"
                    );
                    if let Some(entry) = entries.first() {
                        assert!(
                            matches!(entry.result.as_ref().map(|r| &r.outcome), Some(AcceptanceOutcome::Failed { word }) if word == "interrupted"),
                            "{label}: {:?}",
                            entry.result
                        );
                        if let Some(i) = s.store.intents(&f.wiki.id).unwrap().last() {
                            assert_eq!(
                                i.completion.as_ref().map(|c| c.end),
                                Some(IntentEnd::Abandoned),
                                "{label}"
                            );
                        }
                    }
                }
                assert_eq!(
                    std::fs::read_to_string(f.repo.join("pages/Home.md"))
                        .unwrap()
                        .contains("Uno."),
                    sealed,
                    "{label}: the worktree is the tip"
                );
            }
            Who::System => {
                let ours: Vec<_> = s
                    .store
                    .system_acts(&f.wiki.id)
                    .unwrap()
                    .into_iter()
                    .filter(|e| e.act.message.contains(SYSTEM_SUMMARY))
                    .collect();
                assert_eq!(
                    ours.len(),
                    usize::from(expect.record),
                    "{label}: system act records"
                );
                if sealed {
                    let newest = blocks.first().unwrap();
                    assert_eq!(
                        newest_message(newest).kind(),
                        Kind::System,
                        "{label}: the newest block is the system act's"
                    );
                    assert!(
                        newest
                            .patch_messages
                            .first()
                            .unwrap()
                            .message
                            .contains(SYSTEM_SUMMARY),
                        "{label}"
                    );
                    let end = ours.first().and_then(|e| e.end.as_ref());
                    assert!(
                        matches!(end.map(|e| &e.outcome), Some(AcceptanceOutcome::Sealed { block_id, .. }) if *block_id == newest.block_id),
                        "{label}: the system act names the block: {end:?}"
                    );
                } else if let Some(entry) = ours.first() {
                    assert!(
                        matches!(entry.end.as_ref().map(|e| &e.outcome), Some(AcceptanceOutcome::Failed { word }) if word == "interrupted"),
                        "{label}: {:?}",
                        entry.end
                    );
                }
                assert_eq!(
                    f.repo.join("pages/Sys.md").exists(),
                    sealed,
                    "{label}: the worktree is the tip"
                );
            }
        }

        // The plan is recorded, and ended.
        let acts = s.store.recovery_acts(&f.wiki.id).unwrap();
        assert_eq!(acts.len(), expect.recoveries, "{label}: recovery acts");
        let act = acts.last().unwrap();
        assert_eq!(
            act.end.as_ref().map(|e| e.ending),
            Some(RecoveryEnding::Recovered),
            "{label}"
        );
        if expect.recoveries > 1 {
            // The recovery that was killed is ended as stopped by the next one, and not left open.
            assert_eq!(
                acts.first().unwrap().end.as_ref().map(|e| e.ending),
                Some(RecoveryEnding::Stopped),
                "{label}: the killed recovery"
            );
        }
        assert_eq!(Some(&act.act.id), report.act.as_ref(), "{label}");
        for step in &ran {
            assert!(
                act.act.plan.contains(&format!("\"step\":\"{step}\"")),
                "{label}: {step} in {}",
                act.act.plan
            );
        }
        // Clear: no flag, no lease record, nothing more to recover, the records and the history agree.
        assert_eq!(
            s.store.recovery_flag(&f.wiki.id).unwrap(),
            None,
            "{label}: flag"
        );
        assert_eq!(
            s.leases.recorded_holder(&f.wiki.id).unwrap(),
            None,
            "{label}: lease record"
        );
        assert!(
            recovery.needs_recovery(&f.wiki.id).unwrap().is_empty(),
            "{label}: needs recovery"
        );
        let records = StoreActs::new(s.store.clone(), s.store.clone());
        assert!(
            reconcile(&f.wiki.id, &f.repo, &*s.storage, &records)
                .unwrap()
                .is_clean(),
            "{label}: reconciliation"
        );

        // And the wiki takes writes: the other proposal is accepted, and (for an acceptance) a retry of the first is
        // told what is true.
        let acceptor = s.acceptor();
        let maintainer = Principal::session(authority(), f.maintainer.clone());
        acceptor
            .accept(&maintainer, &f.wiki, &f.second.id, 1)
            .unwrap_or_else(|e| panic!("{label}: a write after recovery: {e}"));
        assert_eq!(log(&s, &f.repo).len(), expected_len + 1, "{label}");
        if who == Who::Acceptance {
            let retry = acceptor.accept(&maintainer, &f.wiki, &f.first.id, 1);
            if sealed {
                assert!(
                    matches!(retry, Ok(inklkunn_core::Accepted::AlreadySigned { .. })),
                    "{label}: {retry:?}"
                );
                assert_eq!(
                    log(&s, &f.repo).len(),
                    expected_len + 1,
                    "{label}: a retry seals nothing twice"
                );
            } else {
                assert!(
                    retry.is_ok(),
                    "{label}: the proposal that was not accepted can be: {retry:?}"
                );
                assert_eq!(log(&s, &f.repo).len(), expected_len + 2, "{label}");
            }
        }
        table.push((
            label,
            format!(
                "{}{}{}",
                if sealed {
                    "change in the history, records completed"
                } else {
                    "nothing in the history, act failed (interrupted)"
                },
                if timed {
                    format!(" (the kill left {left})")
                } else {
                    String::new()
                },
                if failed_closed {
                    " [first failed closed on a torn lock file; the operator cleared it]"
                } else {
                    ""
                },
            ),
            ran.join(" > "),
        ));
    }
    println!("kill-test matrix (step killed -> recovery outcome):");
    for (label, outcome, steps) in &table {
        if steps.is_empty() {
            println!("  {label:<46} -> {outcome}");
        } else {
            println!("  {label:<46} -> {outcome} ; plan: {steps}");
        }
    }
    assert_eq!(table.len(), matrix().len());
}

/// The runbook's section for the stop `foreign-queued-patch` (`docs/RECOVERY.md`).
fn runbook_section() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RECOVERY.md");
    let doc = std::fs::read_to_string(&path).expect("docs/RECOVERY.md is there");
    let at = doc
        .find("## STOP `foreign-queued-patch`")
        .expect("the runbook has the section");
    let section = doc.get(at..).expect("the rest of the document");
    // The section ends at the next second-level heading, if there is one.
    match section.get(3..).and_then(|rest| rest.find("\n## ")) {
        Some(end) => section.get(..end + 3).expect("the section").to_owned(),
        None => section.to_owned(),
    }
}

/// The commands of the fenced block that follows `marker` in `section`, each with the comment after its `#`.
fn runbook_commands(section: &str, marker: &str) -> Vec<(String, String)> {
    let (_, after) = section
        .split_once(marker)
        .unwrap_or_else(|| panic!("the runbook has `{marker}`"));
    let (_, after) = after
        .split_once("```sh\n")
        .expect("a command block follows");
    let (block, _) = after.split_once("```").expect("the block ends");
    block
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once('#') {
            Some((command, comment)) => (command.trim().to_owned(), comment.trim().to_owned()),
            None => (l.to_owned(), String::new()),
        })
        .collect()
}

#[test]
fn the_runbooks_way_out_of_an_orphan_queued_patch_is_true_of_the_state_a_commit_kill_leaves() {
    // Handoff 004, P16, M28: on the first prikk on `PATH` (CI's is the floor, which has no `uncommit`).
    let Some(f) = fixture_floor("runbook-floor") else {
        return;
    };
    the_runbooks_way_out_of_an_orphan_queued_patch(f, false);
}

#[test]
fn on_0_57_0_the_runbooks_way_out_of_an_orphan_queued_patch_is_true_of_the_state_a_commit_kill_leaves()
 {
    // Handoff 004, P16, maintenance M3 and M28: the kill matrix's take-back row on the first prikk 0.57.0 or newer on
    // `PATH`, wherever it stands: recovery offers the take-back by the patch's id.
    let Some(f) = fixture_057("runbook-057") else {
        return;
    };
    the_runbooks_way_out_of_an_orphan_queued_patch(f, true);
}

/// The state is the one the kill matrix's `prikk commit` rows leave when the service's records of the act are gone (a
/// database restored from a copy older than the repository): a sound patch is queued and no recorded act describes it.
/// The test follows the runbook's own commands, literally, as far as prikk 0.55 supports them (they only read), and
/// checks each statement the runbook makes about what recovery says; `verb` is whether the prikk has `uncommit`.
fn the_runbooks_way_out_of_an_orphan_queued_patch(f: Fixture, verb: bool) {
    let real = f.real.clone();
    let older = f.paths.store.with_extension("older.sqlite");
    std::fs::copy(&f.paths.store, &older).expect("the copy a restore would bring back");
    f.kill_at(Who::Acceptance, At::After("commit", 0));
    let restored = Paths {
        root: f.paths.root.clone(),
        keys: f.paths.keys.clone(),
        secret: f.paths.secret.clone(),
        shm: f.paths.shm.clone(),
        store: older,
        prikk: f.paths.prikk.clone(),
    };
    let s = open_services(&restored, "restarted", false);
    // The service starts, finds the repository and its records disagreeing, and the wiki is flagged.
    s.store
        .set_recovery_flag(&RecoveryFlag {
            wiki: f.wiki.id.clone(),
            reason: RecoveryReason::Interrupted,
            act: None,
            since: 5,
        })
        .unwrap();
    let recovery = s.recovery();
    let blocks = log(&s, &f.repo);
    assert_eq!(blocks.len(), f.base_blocks);
    let section = runbook_section();

    // What recovery says: the STOP word and its meaning, after the witnesses the runbook says it names.
    let shown = recovery.recover(&f.wiki, RecoveryMode::PlanOnly).unwrap();
    let text = shown.lines().join("\n");
    assert_eq!(shown.plan.stop, Some(Stop::ForeignQueuedPatch), "{text}");
    assert!(
        section.contains("## STOP `foreign-queued-patch`"),
        "the runbook names the word recovery prints"
    );
    for said in [
        "no act is unfinished",
        "prikk status: 1 patch(es) queued",
        "prikk doctor: 0 error(s), 0 warning(s)",
        "the worktree is the baseline",
        "0 lock(s) held",
        "STOP (foreign-queued-patch): a patch is queued that no recorded act describes",
        "prikk has no verb that removes a sound queued patch",
        "recovery changes nothing and the wiki stays flagged for a person to decide",
    ] {
        assert!(text.contains(said), "the plan says `{said}`:\n{text}");
    }
    assert!(
        shown.plan.steps.is_empty() && shown.plan.ops().is_empty(),
        "no step is planned, so nothing the signer may be asked"
    );

    // The runbook's own commands, as written. `<patch id>` is the id its earlier command printed.
    let commands = runbook_commands(&section, "**Look, without changing anything.**");
    let words: Vec<&str> = commands.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(
        words,
        [
            "inklkunn wiki recover <wiki> --plan",
            "cd $INKLKUNN_DATA_DIR/wikis/<xx>/<wiki id>",
            "prikk status --format json",
            "prikk show <patch id>",
            "prikk worktree-status",
            "prikk log",
        ],
        "the runbook's commands"
    );
    let mut patch_id: Option<String> = None;
    let mut seen_log = false;
    for (command, _) in &commands {
        if command == "inklkunn wiki recover <wiki> --plan" {
            // Its output is the plan checked above.
        } else if let Some(dir) = command.strip_prefix("cd ") {
            assert_eq!(dir, "$INKLKUNN_DATA_DIR/wikis/<xx>/<wiki id>");
            // `<xx>` is the first two characters of the wiki id, under the data directory's `wikis`.
            let id = f.wiki.id.as_str();
            let shard = id.get(..2).expect("an id is longer than two characters");
            assert_eq!(f.repo, f.paths.root.join(shard).join(id));
        } else if let Some(args) = command.strip_prefix("prikk ") {
            let args = if args.contains("<patch id>") {
                args.replace(
                    "<patch id>",
                    patch_id
                        .as_deref()
                        .expect("the runbook has printed the patch id before it uses it"),
                )
            } else {
                args.to_owned()
            };
            let args: Vec<String> = args.split_whitespace().map(str::to_owned).collect();
            let out = Command::new(&real)
                .args(&args)
                .current_dir(&f.repo)
                .output()
                .expect("prikk runs");
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            assert!(
                out.status.success(),
                "`{command}` failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            match args.first().map(String::as_str) {
                Some("status") => {
                    // "that one patch is queued, and its id"
                    let status: serde_json::Value =
                        serde_json::from_str(&stdout).expect("the runbook asks for the JSON form");
                    assert_eq!(
                        status
                            .pointer("/queue/count")
                            .and_then(serde_json::Value::as_u64),
                        Some(1)
                    );
                    patch_id = status
                        .pointer("/queue/patches/0/patch_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);
                    assert!(patch_id.is_some(), "the status names the patch: {stdout}");
                }
                Some("show") => {
                    // "what the patch changes, file by file"
                    assert!(stdout.contains("queued: yes"), "{stdout}");
                    assert!(stdout.contains("path: pages/Home.md"), "{stdout}");
                    // The text the operation replaces and the text it puts (the span that changed, not the page).
                    assert!(stdout.contains("old:\nOne\n"), "{stdout}");
                    assert!(stdout.contains("new:\nUno\n"), "{stdout}");
                }
                Some("worktree-status") => {
                    // "whether the worktree matches it"
                    assert!(
                        stdout.contains("worktree: clean against baseline"),
                        "{stdout}"
                    );
                }
                Some("log") => {
                    // "the sealed history, which the patch is not part of"
                    seen_log = true;
                    let shown_blocks = stdout.lines().filter(|l| l.starts_with("block ")).count();
                    assert_eq!(shown_blocks, f.base_blocks, "{stdout}");
                    let id = patch_id.as_deref().expect("the patch id");
                    assert!(!stdout.contains(id), "the patch is not in the history");
                }
                other => panic!("the runbook runs a verb the test does not know: {other:?}"),
            }
        } else {
            panic!("a command the test does not know: {command}");
        }
    }
    assert!(seen_log);
    // "The worktree holds every page the patch changes whole, as the patch leaves it."
    assert!(
        std::fs::read_to_string(f.repo.join("pages/Home.md"))
            .unwrap()
            .contains("Uno."),
        "the worktree holds the page as the patch leaves it"
    );
    // "A queued patch is not in the history and does no harm while it waits": the repository verifies with it queued.
    let verified = Command::new(&real)
        .arg("verify")
        .current_dir(&f.repo)
        .output()
        .unwrap();
    assert!(verified.status.success());

    // "prikk 0.55 has no verb that discards a sound queued patch": the one discard it has is for a commit the log no
    // longer holds soundly, and it refuses over this patch (a plan only, so nothing is written either way).
    let help = Command::new(&real).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    for line in help.lines().filter(|l| l.contains("discard")) {
        assert!(
            line.contains("doctor [path] --discard-damaged-commits"),
            "prikk has a discard the runbook does not know of: {line}"
        );
    }
    let refused = Command::new(&real)
        .args(["doctor", "--discard-damaged-commits", "--plan-only"])
        .current_dir(&f.repo)
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("nothing acknowledged is damaged"),
        "{refused:?}"
    );

    // From prikk 0.57.0 `uncommit` discards a sound queued patch, and the runbook says how a person confirms it: the
    // flag recovery takes, by the patch's id, in the words the plan prints (maintenance M3, ledger PK-43).
    assert!(
        section.contains("--discard-queued-patch <patch id>") && section.contains("prikk uncommit"),
        "the runbook names the confirmation"
    );
    let help = Command::new(&real).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    let has_uncommit = help.contains("uncommit");
    assert_eq!(
        has_uncommit, verb,
        "the caller chose the prikk by whether it has the verb"
    );
    // A plan that is only shown offers nothing and asks prikk for no preview (M5: the preview takes prikk's lock): with
    // the verb it says that a run shows the preview and the command, without it that nothing can be taken back (M21:
    // whether the verb exists is asked, and takes no lock).
    let offered = recovery
        .recover(&f.wiki, RecoveryMode::PlanOnly)
        .unwrap()
        .plan
        .notes
        .join("\n");
    if has_uncommit {
        assert!(
            offered.contains("`recover --plan` does not ask prikk")
                && !offered.contains("prikk says"),
            "{offered}"
        );
    } else {
        assert!(
            offered.contains("this prikk has no `uncommit`")
                && !offered.contains("`recover --plan` does not ask prikk")
                && !offered.contains("prikk says"),
            "{offered}"
        );
    }

    // "A recovery that stops leaves the wiki flagged and the repository exactly as it found it; it records the plan
    // and that it stopped, so the trail shows what was seen."
    let run = recovery.recover(&f.wiki, RecoveryMode::Run).unwrap();
    assert!(
        matches!(&run.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:#?}",
        run.lines()
    );
    assert_eq!(run.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(run.done.is_empty());
    // A run asks prikk for the preview, so it offers the take-back exactly when this prikk has the verb.
    assert_eq!(
        run.plan.notes.join("\n").contains("--discard-queued-patch"),
        has_uncommit,
        "the run offers the take-back exactly when this prikk has the verb: {:?}",
        run.plan.notes
    );
    assert_eq!(log(&s, &f.repo), blocks, "nothing was sealed");
    assert_eq!(s.storage.status(&f.repo).unwrap().queue.count, 1);
    assert!(s.store.recovery_flag(&f.wiki.id).unwrap().is_some());
    let acts = s.store.recovery_acts(&f.wiki.id).unwrap();
    assert_eq!(acts.len(), 1);
    assert_eq!(
        acts.first().and_then(|a| a.end.as_ref()).map(|e| e.ending),
        Some(RecoveryEnding::Stopped)
    );
    assert!(
        acts.first()
            .is_some_and(|a| a.act.plan.contains("foreign-queued-patch"))
    );
}
