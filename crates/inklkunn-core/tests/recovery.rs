//! Recovery against a real repository (RFC 004 D-7; handoff 004 T5): acceptances and imports that ended after
//! `commit`, wikis whose queue was torn or whose locks were left, completed or discarded or cleared by the service's
//! own procedure, under the lease, with the plan recorded and every step in the audit trail. Self-skips with a visible
//! line when prikk is not on `PATH`. The real kills (a child process, `SIGKILL`) are `inklkunn-signer`'s
//! `tests/acceptance_kill.rs`; the core crate may not spawn a process.
//!
//! A stand-in `prikk` passes every call to the real one unless a control file names the verb: `fail-<verb>` makes it
//! fail, `failafter-<verb>` (holding a number) lets that many calls of the verb through and fails the rest,
//! `say-<verb>` makes it print a canned answer (`say-until-seal` ends every canned answer once a `seal` has run),
//! `runfail-<verb>` makes the real verb do its work and the call
//! then report a failure (what a call stopped at its bound after its effect was durable looks like),
//! `dies-after-publishing-<verb>` makes the real `seal` publish its block and then puts back the three files of the
//! active session that it emptied, as they were, and fails (a `seal` killed between publishing and draining the queue,
//! made on cue: handoff 004, P15; `leaves-the-lock` adds the stale `active.lock` the killed process would have left,
//! and `leaves-only-the-metadata` puts back the ref's metadata but not the queue: the other state a kill just after
//! the publication leaves, a lone `ACTIVE-REF-METADATA-DEBRIS` warning); `fail-unlock-clear` fails `unlock` only when
//! it is asked to clear a lock (`unlock --lock <path> ...`), so the listing still works; `appears-running` lists every lock as one
//! whose holder appears running and refuses a clear that is not `--force` (prikk 0.57.0's `--yes`), and `refuse-yes` refuses
//! a clear that is not `--force` for any holder (prikk 0.58.0's `--yes` for a holder it cannot judge): every clear's arguments
//! are logged, for `World::unlock_calls`; `leads-after-branch-create`
//! runs the real `branch create` and puts the ref log back as it was (a view's creation killed between the pointer and
//! the log, as in `view_recovery.rs`); `leads-the-log` (with `dies-after-publishing-seal`) puts the ref log back as it was
//! after the real `seal` published, so that the pointer leads the log by one transition and the queue still holds the
//! patch (a `seal` killed after it moved the pointer and before it appended the log: maintenance M61); `log-other-message`
//! prints the real history with the summary of the acceptance's message changed; `list-fails-after-clear` fails
//! the listing of locks once a clear has been asked for, and `relock-after-clear` (holding a path) lets the real clear
//! work and then writes the lock back, which is a clear that did not hold; `format-marker` (holding a number) makes
//! every verb but `--version` answer `error: unsupported format version: <number>`, exit 1 (a damaged or unknown marker).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use inklkunn_core::Accept;
use inklkunn_core::import::{plan_folder, run_import, wiki_state};
use inklkunn_core::message::{Actor, BlockId, Message, Summary};
use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::reconcile::{Report, reconcile};
use inklkunn_core::recovery::processes::{ProcessInfo, ProcessTable};
use inklkunn_core::recovery::{Step, Stop};
use inklkunn_core::{
    AcceptFailure, AcceptanceLedger, AcceptanceParts, AcceptanceToken, Authorizer, CheckOutcome,
    ExpectedMessage, IdentityAuthority, KeyAuthority, LeaseConfig, LedgerError, OldKey,
    PreparedChange, PreparedFile, Principal, ProposalAcceptor, Purpose, ReadConfig, ReadService,
    Recover, RecoverOptions, Recovery, RecoveryError, RecoveryMode, RecoveryOutcome, RecoveryParts,
    Signer, SignerConfig, StoreActs, StoreAuthz, StoreLedger, SystemActFailure, SystemActs,
    Verifier, ViewCreationAct, ViewCreationFacts, WikiLeases, Witnesses, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, Account, AccountId, AccountStore, ActStore, AuditEvent,
    AuditLog, AuditSink, DisplayName, FileChange, IntentEnd, NewWiki, ProposalRecord,
    ProposalState, RecoveryEnding, RecoveryFlag, RecoveryReason, Role, RoleGrant, RoleStore, Slug,
    SqliteStore, SystemActEnd, SystemActId, SystemActKind, SystemActRecord, VerificationState,
    WikiStore,
};

/// Every question this crate asks of the prikk on `PATH`: the only place a test reaches a capability (maintenance M39).
#[path = "support/prikk_probes.rs"]
mod prikk_probes;

const MAIN: &str = "heads/main";

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
}

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
if [ "$verb" = uncommit ] && [ "$#" -eq 1 ]; then echo uncommit-probe >> "$CTL/calls.log"; else echo "$verb" >> "$CTL/calls.log"; fi
if [ "$1 $2" = "branch create" ] && [ -e "$CTL/leads-after-branch-create" ]; then
  mkdir -p "$CTL/kept-ref-log"
  for f in .prikk/refs/containers/log-*.container; do cp "$f" "$CTL/kept-ref-log/"; done
  "{real}" "$@" > /dev/null 2>&1
  for f in "$CTL"/kept-ref-log/log-*.container; do cp "$f" .prikk/refs/containers/; done
  echo "error: the call was stopped after branch create had published its pointer" >&2; exit 1
fi
if [ "$verb" = uncommit ] && [ -e "$CTL/hold-uncommit" ]; then
  : > "$CTL/uncommit-started"
  n=0
  while [ ! -e "$CTL/release" ] && [ "$n" -lt 400 ]; do sleep 0.05; n=$((n + 1)); done
fi
if [ "$verb" = unlock ] && [ "$#" -gt 1 ]; then echo "$*" >> "$CTL/unlock-args.log"; fi
if [ "$verb" = unlock ] && [ -e "$CTL/appears-running" ]; then
  if [ "$#" -eq 1 ]; then
    "{real}" "$@" | sed 's/^  liveness: .*/  liveness: appears running -- do not clear: the process that took the lock is still running (same pid and start time)/'
    exit 0
  fi
  case " $* " in *" --force "*) ;; *) echo "error: precondition not met: the process that took the lock appears running -- refusing to clear its lock. --yes does not clear a lock whose holder appears running." >&2; exit 1;; esac
fi
if [ "$verb" = unlock ] && [ "$#" -gt 1 ] && [ -e "$CTL/refuse-yes" ]; then
  case " $* " in *" --force "*) ;; *) echo "error: precondition not met: whether the process that took the lock is still running cannot be established (written by an older prikk, which recorded no start time to check against) -- refusing to clear its lock. --yes does not clear a lock whose holder's liveness is unknown. --force clears it anyway." >&2; exit 1;; esac
fi
if [ "$verb" = unlock ] && [ "$#" -gt 1 ] && [ -e "$CTL/fail-unlock-clear" ]; then echo "error: precondition not met: injected failure of unlock --lock" >&2; exit 1; fi
if [ "$verb" = unlock ] && [ "$#" -eq 1 ] && [ -e "$CTL/list-fails-after-clear" ] && [ -e "$CTL/unlock-cleared" ]; then echo "error: precondition not met: injected failure of unlock (listing)" >&2; exit 1; fi
if [ "$verb" = unlock ] && [ "$#" -gt 1 ]; then
  : > "$CTL/unlock-cleared"
  if [ -e "$CTL/relock-after-clear" ]; then
    "{real}" "$@"; rc=$?
    printf 'pid=2147483000\nkind=active\nnote=PR-007 lock has no stale-lock stealing yet\n' > "$(cat "$CTL/relock-after-clear")"
    exit $rc
  fi
fi
if [ -e "$CTL/fail-$verb" ] && ! ( [ "$verb" = uncommit ] && [ "$#" -eq 1 ] ); then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
if [ -e "$CTL/format-marker" ] && [ "$verb" != --version ]; then echo "error: unsupported format version: $(cat "$CTL/format-marker")" >&2; exit 1; fi
if [ -e "$CTL/failafter-$verb" ]; then
  n="$(cat "$CTL/failafter-$verb")"; c="$(cat "$CTL/count-$verb" 2>/dev/null || echo 0)"
  echo "$((c + 1))" > "$CTL/count-$verb"
  if [ "$c" -ge "$n" ]; then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
fi
if [ "$verb" = log ] && [ -e "$CTL/log-other-message" ]; then "{real}" "$@" | sed 's/Say it in Spanish/Something else entirely/'; exit 0; fi
if [ "$verb" = seal ] && [ -e "$CTL/say-until-seal" ]; then : > "$CTL/sealed"; fi
if [ "$verb $2" = "status --format" ] && [ -e "$CTL/say-status-json" ] && ! ( [ -e "$CTL/say-until-seal" ] && [ -e "$CTL/sealed" ] ); then cat "$CTL/say-status-json"; exit 0; fi
if [ -e "$CTL/say-$verb" ] && ! ( [ -e "$CTL/say-until-seal" ] && [ -e "$CTL/sealed" ] ); then cat "$CTL/say-$verb"; exit 0; fi
if [ -e "$CTL/runfail-$verb" ]; then
  "{real}" "$@" > /dev/null 2>&1
  echo "error: the call was stopped after $verb had done its work" >&2; exit 1
fi
if [ -e "$CTL/dies-after-publishing-$verb" ]; then
  A=".prikk/active/default"
  mkdir -p "$CTL/kept"
  cp "$A/queue.wal" "$A/ref-name" "$A/witness" "$CTL/kept/"
  if [ -e "$CTL/leads-the-log" ]; then
    # Killed after the ref's pointer moved and before the ref log was appended (maintenance M61): the log is put back.
    mkdir -p "$CTL/kept-ref-log"
    for f in .prikk/refs/containers/log-*.container; do cp "$f" "$CTL/kept-ref-log/"; done
  fi
  "{real}" "$@" > /dev/null 2>&1
  cp "$CTL/kept/ref-name" "$CTL/kept/witness" "$A/"
  if [ ! -e "$CTL/leaves-only-the-metadata" ]; then cp "$CTL/kept/queue.wal" "$A/"; fi
  if [ -e "$CTL/leads-the-log" ]; then
    for f in "$CTL"/kept-ref-log/log-*.container; do cp "$f" .prikk/refs/containers/; done
  fi
  if [ -e "$CTL/leaves-the-lock" ]; then
    printf 'pid=2147483000\nkind=active\nnote=PR-007 lock has no stale-lock stealing yet\n' > "$A/active.lock"
  fi
  echo "error: the call was stopped after $verb had published its block" >&2; exit 1
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
    standin: PathBuf,
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
        let real = prikk_probes::prikk_first()?;
        World::over(tag, lease, &real)
    }

    /// The same, over the first prikk on `PATH` that is 0.57.0 or newer (the host's, when the gate runs with 0.55.0
    /// first): for the tests that need `uncommit` and what 0.57.0 does with a held lock. The probe prints the guarded
    /// line when there is none (`prikk_probes`).
    fn new_057(tag: &str, lease: LeaseConfig) -> Option<World> {
        let real = prikk_probes::prikk_057()?;
        World::over(tag, lease, &real)
    }

    /// The same, over the **first** `prikk` on `PATH`, when it is older than 0.57.0 (CI's is the floor): the tests of the
    /// answer *no verb*. A first prikk that has `uncommit` prints the guarded skip line (CI fails on it: the floor must
    /// be first), and the subject runs on the 0.57.0 side through [`World::new_057`] instead (maintenance M28).
    fn new_floor(tag: &str, lease: LeaseConfig) -> Option<World> {
        let real = prikk_probes::prikk_floor()?;
        World::over(tag, lease, &real)
    }

    fn over(tag: &str, lease: LeaseConfig, real: &Path) -> Option<World> {
        if !Path::new("/dev/shm").is_dir() {
            println!("skipping: no /dev/shm here");
            return None;
        }
        let tmp = dir_in(&std::env::temp_dir(), tag);
        let shm = dir_in(Path::new("/dev/shm"), tag);
        let ctl = tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let standin = write_standin(&ctl, real);
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
            standin,
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
        let have = wiki_state(
            &*self.storage,
            &self.layout,
            &self.wiki,
            &ReadConfig::default(),
        )
        .unwrap();
        let plan = plan_folder(&folder.0, &have, false, &ReadConfig::default()).unwrap();
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
        self.propose_in(None, summary, files)
    }

    /// A proposal into `view` (`None`: the wiki's default view), made by the contributor and submitted. The acceptance
    /// service takes the view from the proposal: nothing else names it (maintenance M62).
    fn propose_in(
        &self,
        view: Option<&str>,
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
            .create(&self.contributor, &self.wiki, view, edit)
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

    /// An acceptance whose `seal` published its block and was killed before it drained the queue (the stand-in's
    /// `dies-after-publishing-seal`, with `extra` controls): the wiki is flagged, and the block is in the history.
    fn killed_seal(&self, p: &ProposalRecord, extra: &[&str]) {
        self.control("dies-after-publishing-seal", "");
        for name in extra {
            self.control(name, "");
        }
        let err = self.accept(p).unwrap_err();
        self.uncontrol("dies-after-publishing-seal");
        for name in extra {
            self.uncontrol(name);
        }
        assert!(
            matches!(&err, AcceptFailure::Incomplete { flagged: true, .. }),
            "{err:?}"
        );
    }

    fn recovery_witnesses(&self) -> Witnesses {
        Witnesses::new(&self.standin, Sandbox::Unconfined, self.layout.clone())
    }

    /// The id of the first queued patch.
    fn queued_patch_id(&self) -> String {
        self.storage
            .status(&self.repo)
            .unwrap()
            .queue
            .patches
            .first()
            .expect("a patch is queued")
            .patch_id
            .clone()
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

    /// How many times prikk was asked for the `uncommit` verb (the preview and the take-back both are).
    fn uncommit_calls(&self) -> usize {
        std::fs::read_to_string(self.ctl.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| *l == "uncommit")
            .count()
    }

    /// Every `unlock --lock <path> ...` call prikk was given, as the stand-in logged it (the verb first): how recovery
    /// asked for each clear (maintenance M50).
    fn unlock_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.ctl.join("unlock-args.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
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
        self.plant_change(page, text, false);
        assert_eq!(self.queue_count(), 1);
    }

    /// Plant a change the records know nothing of **and seal it**: a block in the history that no act explains
    /// (the reconciliation calls it an unrecorded seal). Returns its block.
    fn plant_sealed_block(&self, page: &str, text: &str) -> String {
        self.plant_change(page, text, true);
        assert_eq!(self.queue_count(), 0);
        self.tip()
    }

    fn plant_change(&self, page: &str, text: &str, seal: bool) {
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
                base: Some(base.clone()),
                imported_from: None,
                pinned_tip: base,
                change_digest: change.digest(),
            },
        );
        if !seal {
            self.control("fail-seal", "");
        }
        let r = inklkunn_core::Accept::accept(&self.planter, &self.wiki.id, MAIN, &token, &change);
        self.uncontrol("fail-seal");
        if seal {
            assert!(r.is_ok(), "{r:?}");
        } else {
            assert!(
                matches!(r, Err(inklkunn_core::AcceptError::Queued { .. })),
                "{r:?}"
            );
        }
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
    // The body a prikk of 0.56.0 or older writes, byte for byte (its fixed `note=` line is how 0.57.0 tells an old
    // body from an incomplete one; any other note reads as a body cut short).
    std::fs::write(
        &path,
        format!("pid={pid}\nkind={kind}\nnote=PR-007 lock has no stale-lock stealing yet\n"),
    )
    .unwrap();
    std::fs::canonicalize(&path).unwrap().display().to_string()
}

/// `docs/RECOVERY.md`, whose statements about what recovery and prikk say these tests hold true. Whitespace is
/// collapsed, so that a sentence wrapped across lines is the sentence.
fn runbook() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RECOVERY.md"))
        .expect("docs/RECOVERY.md is there")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
fn a_seal_killed_between_publishing_and_draining_is_finished_with_prikks_own_recommendation_as_a_step()
 {
    // Handoff 004, P15. `status --format json` refuses, `doctor` names exactly PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING
    // and recommends `seal --allow-no-audit`; the block published is the acceptance's. Both ways it is left: with the
    // stale `active.lock` the killed process would have left (cleared first) and without.
    for (tag, lock) in [("cleanup", false), ("cleanup-lock", true)] {
        let Some(w) = World::new(tag, LeaseConfig::default()) else {
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
        w.killed_seal(&p, if lock { &["leaves-the-lock"] } else { &[] });
        // The state, as prikk shows it.
        assert_eq!(
            w.history().len(),
            before.len() + 1,
            "the block is published"
        );
        // `status --format json` refuses over the state a killed seal leaves up to 0.57.0; 0.58.0 reads it (M51).
        prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &w.tip());
        let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
        assert!(doctor.is_only_cleanup_pending(), "{doctor:?}");
        assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1);
        let published = w.tip();
        let seals = |w: &World| {
            std::fs::read_to_string(w.ctl.join("calls.log"))
                .unwrap()
                .lines()
                .filter(|v| *v == "seal")
                .count()
        };
        let commits = std::fs::read_to_string(w.ctl.join("calls.log"))
            .unwrap()
            .lines()
            .filter(|v| *v == "commit")
            .count();
        let seals_before = seals(&w);

        // The plan, shown: it changes nothing, and it names the step and what it rests on.
        let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
        assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
        let mut expected = vec!["complete-publication", "restore-worktree", "end-sealed"];
        if lock {
            expected.insert(0, "clear-locks");
        }
        expected.extend(["verify", "reconcile", "clear-flag"]);
        assert_eq!(step_names(&shown), expected);
        assert!(
            shown
                .lines()
                .iter()
                .any(|l| l.contains("run seal --allow-no-audit once, as prikk's doctor recommends")),
            "{:#?}",
            shown.lines()
        );
        assert_eq!(seals(&w), seals_before, "a plan runs nothing");
        prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &w.tip());

        // The run.
        let r = w.recover();
        assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
        let done: Vec<&str> = r.done.iter().map(|d| d.step).collect();
        assert_eq!(done, expected);
        assert_eq!(seals(&w), seals_before + 1, "prikk's recommendation, once");
        let commits_after = std::fs::read_to_string(w.ctl.join("calls.log"))
            .unwrap()
            .lines()
            .filter(|v| *v == "commit")
            .count();
        assert_eq!(commits_after, commits, "recovery never commits");
        // No second block: the tip is the one that was published; the queue is drained; the repository verifies.
        assert_eq!(w.history().len(), before.len() + 1);
        assert_eq!(w.tip(), published);
        assert_eq!(w.queue_count(), 0);
        assert!(w.verifies());
        assert!(
            w.recovery_witnesses()
                .doctor(&w.wiki.id)
                .unwrap()
                .diagnostics
                .iter()
                .all(|d| !d.is_cleanup_pending())
        );
        assert!(w.page("pages/Home.md").contains("Uno."));
        // The records agree with the history: the acceptance sealed in that block, the intent completed, the
        // proposal accepted, the flag gone.
        let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
        assert!(matches!(
            entry.result.map(|r| r.outcome),
            Some(AcceptanceOutcome::Sealed { block_id, .. }) if block_id == published
        ));
        assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
        assert_eq!(w.state(&p), ProposalState::Accepted);
        assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
        assert!(w.reconciled().is_clean());
        // The step is in the recorded plan and in the audit trail before it was taken.
        let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
        let record = acts.first().unwrap();
        assert!(
            record
                .act
                .plan
                .contains("\"step\":\"complete-publication\""),
            "{}",
            record.act.plan
        );
        let audit = w.audit();
        let at = audit
            .iter()
            .position(|(a, _, d)| a == "recovery-step" && d == "complete-publication")
            .expect("the step is audited");
        assert!(
            audit
                .iter()
                .skip(at + 1)
                .any(|(a, _, d)| a == "recovery-step" && d == "end-sealed"),
            "{audit:?}"
        );
        // The wiki takes writes again.
        assert!(matches!(
            w.accept(&q).expect("writes are accepted again"),
            inklkunn_core::Accepted::Signed(_)
        ));
        assert_eq!(w.history().len(), before.len() + 2);
        assert!(w.reconciled().is_clean());
    }
}

#[test]
fn a_pending_cleanup_is_not_finished_unless_doctor_names_exactly_that_and_the_block_is_the_acts() {
    let Some(w) = World::new("cleanup-stops", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let blocks = w.history();
    let seals = || {
        std::fs::read_to_string(w.ctl.join("calls.log"))
            .unwrap()
            .lines()
            .filter(|v| *v == "seal")
            .count()
    };
    let seals_before = seals();
    // Nothing is changed by any of the refusals below.
    let untouched = |what: &str| {
        assert_eq!(w.history(), blocks, "{what}");
        assert_eq!(seals(), seals_before, "{what}: no seal ran");
        assert!(
            w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
            "{what}"
        );
        prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &w.tip());
        assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1, "{what}");
    };

    // Another error beside the cleanup, as `doctor` says it: not that code alone, so a stop and nothing done.
    w.control(
        "say-doctor",
        "doctor repository: /x\nerror [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree but matching active publication state remains\n  recommendation: run signer-backed `prikk seal --allow-no-audit` for the affected ref\nerror [PRIKK-DOCTOR-OBJECT-MISSING]: an object is missing\nissue summary: errors=2, warnings=0, info=0\n",
    );
    let r = w.recover();
    assert_eq!(r.plan.stop, Some(Stop::DoctorTrouble), "{:#?}", r.lines());
    assert!(matches!(
        r.outcome,
        RecoveryOutcome::Stopped { at: None, .. }
    ));
    assert!(r.done.is_empty());
    untouched("another error beside it");
    // `status` refuses and `doctor` does not say why (a clean report): a witness that cannot be explained is not
    // read, and recovery fails closed before it plans.
    w.control(
        "say-doctor",
        "doctor repository: /x\ninfo [PRIKK-DOCTOR-VERIFY-OK]: done\nissue summary: errors=0, warnings=0, info=1\n",
    );
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = w.recovery.recover(&w.wiki, mode);
        assert!(
            matches!(&r, Err(RecoveryError::Witness(why)) if why.contains("prikk status")),
            "{r:?}"
        );
    }
    untouched("status refuses and doctor is clean");
    // The same error as a warning is not the error.
    w.control(
        "say-doctor",
        "doctor repository: /x\nwarning [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: x\nissue summary: errors=0, warnings=1, info=0\n",
    );
    assert!(matches!(
        w.recovery.recover(&w.wiki, RecoveryMode::Run),
        Err(RecoveryError::Witness(_))
    ));
    untouched("a warning of that code");
    w.uncontrol("say-doctor");
    // `status` is refused for another reason than an integrity error (prikk could not be asked at all, say): the
    // pending cleanup does not explain it, and recovery fails closed.
    w.control("fail-status", "");
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = w.recovery.recover(&w.wiki, mode);
        assert!(
            matches!(&r, Err(RecoveryError::Witness(why)) if why.contains("injected failure of status")),
            "{r:?}"
        );
    }
    w.uncontrol("fail-status");
    untouched("status refused for another reason");

    // The block published is not an unfinished act's: no act at all (its records are gone, say), so the cleanup is
    // somebody else's publication. A stop, nothing done.
    let intent = w.store.open_intents(&w.wiki.id).unwrap().remove(0);
    w.store
        .complete_intent(&inklkunn_store::IntentCompletion {
            intent: intent.intent.id.clone(),
            end: IntentEnd::Abandoned,
            at: 9,
        })
        .unwrap();
    let acceptance = intent.intent.acceptance.clone();
    w.store
        .finish_acceptance(&inklkunn_store::AcceptanceResult {
            acceptance,
            outcome: AcceptanceOutcome::Failed {
                word: "interrupted".to_owned(),
            },
            at: 9,
        })
        .unwrap();
    let r = w.recover();
    assert_eq!(
        r.plan.stop,
        Some(Stop::PublishedBlockIsNotTheAct),
        "{:#?}",
        r.lines()
    );
    assert!(r.done.is_empty());
    assert_eq!(w.history(), blocks);
    assert_eq!(seals(), seals_before, "no seal ran");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &w.tip());
}

/// What `prikk status --format json` says in the state a killed seal leaves, as prikk 0.58.0 says it, made up for any
/// prikk: the stand-in answers `status` with it (`say-status`), so the 0.58.0 reading is exercised on every prikk. The
/// queue is `patches` (ids; the messages and operations are of no use to the reader), sealed in `block`, if any.
fn canned_status(block: Option<&str>, patches: &[String]) -> String {
    serde_json::json!({
        "schema_version": "status-report-v1",
        "active_wal_records": patches.len(),
        "trailing_partial_wal_bytes": 0,
        "current_branch": MAIN,
        "provisional_worktree": null,
        "interrupted_materialization": null,
        "queue": {
            "count": patches.len(),
            "target_ref": MAIN,
            "target_ref_status": null,
            "already_sealed": block.map(|b| serde_json::json!({"block_id": b, "route": "prikk seal --allow-no-audit"})),
            "patches": patches.iter().map(|id| serde_json::json!({
                "patch_id": id, "message": "m", "operations": []
            })).collect::<Vec<_>>(),
        },
    })
    .to_string()
}

#[test]
fn a_status_that_reads_the_pending_cleanup_must_agree_with_the_history_and_with_doctor() {
    // Maintenance M51: from prikk 0.58.0 `status --format json` reads the state a killed seal leaves and says which
    // block the queue is already sealed in. Where it does, the decision is the same as where it refuses (the
    // acceptance's block, `doctor`'s one error); where it disagrees with the history or with `doctor` it is a stop and
    // not a step: nothing is concluded from either. (The answers are made up through the stand-in so that every prikk
    // meets them; the real answer of each prikk is asserted by `assert_status_shows_a_pending_cleanup`.)
    let Some(w) = World::new("cleanup-reads", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let blocks = w.history();
    let tip = blocks.first().unwrap();
    let patches: Vec<String> = tip
        .patch_messages
        .iter()
        .map(|m| m.patch_id.clone())
        .collect();
    let seals = || {
        std::fs::read_to_string(w.ctl.join("calls.log"))
            .unwrap()
            .lines()
            .filter(|v| *v == "seal")
            .count()
    };
    let seals_before = seals();
    let refuses = |what: &str, said: &str| {
        for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
            let r = w.recovery.recover(&w.wiki, mode);
            assert!(
                matches!(&r, Err(RecoveryError::Witness(why)) if why.contains("prikk status") && why.contains(said)),
                "{what}: {r:?}"
            );
        }
        assert_eq!(w.history(), blocks, "{what}");
        assert_eq!(seals(), seals_before, "{what}: no seal ran");
        assert!(
            w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
            "{what}"
        );
        assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1, "{what}");
    };
    // Another block than the tip: a stop, not a step.
    let elsewhere = "e".repeat(64);
    w.control(
        "say-status-json",
        &canned_status(Some(&elsewhere), &patches),
    );
    refuses("another block", "already sealed in block");
    // An ordinary queue while `doctor` names a pending cleanup.
    w.control("say-status-json", &canned_status(None, &patches));
    refuses("an ordinary queue", "ordinary queue");
    // A queue that is not the block's patches (more, fewer or others).
    for (what, queue) in [
        ("another patch", vec!["f".repeat(64)]),
        ("none", Vec::new()),
        (
            "one more",
            patches
                .iter()
                .cloned()
                .chain(std::iter::once("f".repeat(64)))
                .collect(),
        ),
    ] {
        w.control(
            "say-status-json",
            &canned_status(Some(&tip.block_id), &queue),
        );
        refuses(what, "not the patches of the newest block");
    }
    // The block is named, and `doctor` names no cleanup.
    w.control(
        "say-status-json",
        &canned_status(Some(&tip.block_id), &patches),
    );
    w.control(
        "say-doctor",
        "doctor repository: /x\ninfo [PRIKK-DOCTOR-VERIFY-OK]: done\nissue summary: errors=0, warnings=0, info=1\n",
    );
    refuses("doctor names none", "names no error at all");
    w.uncontrol("say-doctor");

    // Where it agrees, the plan is the same as where the answer was a refusal, and the run finishes it.
    w.control(
        "say-status-json",
        &canned_status(Some(&tip.block_id), &patches),
    );
    w.control("say-until-seal", "");
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(
        step_names(&shown),
        [
            "complete-publication",
            "restore-worktree",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(
        shown.lines().iter().any(|l| l.contains(&format!(
            "saw: prikk status: 1 patch(es) queued, all already sealed in block {}",
            tip.block_id
        ))),
        "{:#?}",
        shown.lines()
    );
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(seals(), seals_before + 1, "prikk's recommendation, once");
    assert_eq!(w.history(), blocks, "no second block");
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    w.uncontrol("say-status-json");
    w.uncontrol("say-until-seal");
    assert!(w.verifies());
}

/// A `seal` killed just after it published (the queue drained, the ref's metadata stale): recovery notes the one
/// warning `doctor` has and finishes; the next acceptance's commit replaces the stale metadata (M16, PK-46).
fn a_lone_metadata_debris_is_noted_and_recovery_finishes(w: &World) {
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &["leaves-only-the-metadata"]);
    let blocks = w.history();
    assert_eq!(w.queue_count(), 0, "the queue is drained");
    let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(
        doctor.diagnostics.iter().any(|d| d.is_metadata_debris()),
        "{doctor:?}"
    );
    assert!(!doctor.is_only_cleanup_pending());
    assert!(doctor.others().is_empty(), "{:?}", doctor.others());
    let debris = doctor
        .diagnostics
        .iter()
        .find(|d| d.is_metadata_debris())
        .unwrap();
    // A warning up to prikk 0.57.0 and `info` from 0.58.0 (M52); recovery reads it alike on both.
    prikk_probes::assert_debris_is_worded_as_this_prikk_words_it(&w.storage, debris);
    assert!(w.verifies(), "nothing is lost: the repository verifies");

    // The plan is no stop: it ends the act as sealed and carries the one-line note.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(nth(&step_names(&shown), 0), &"end-sealed");
    assert!(
        shown
            .plan
            .notes
            .contains(&"prikk's stale ref metadata; the next accepted change clears it".to_owned()),
        "{:?}",
        shown.plan.notes
    );
    assert_eq!(shown.plan.notes.len(), 1, "the one note, on any prikk");
    // The run recovers: records ended, the flag cleared, the history untouched.
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(w.history(), blocks);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert!(w.verifies());
    // Recovery does not touch the metadata; the next accepted change's commit replaces it.
    let still = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(still.diagnostics.iter().any(|d| d.is_metadata_debris()));
    let next = w.propose(
        "And in Portuguese",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Ola.")))],
    );
    assert!(w.accept(&next).is_ok(), "the wiki takes writes again");
    let after = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(
        !after.diagnostics.iter().any(|d| d.is_metadata_debris()),
        "the next commit cleared it: {after:?}"
    );
    assert!(w.verifies());

    // What docs/RECOVERY.md says about it.
    let runbook = runbook();
    assert!(
        runbook.contains("prikk's stale ref metadata; the next accepted change clears it"),
        "the runbook says the sentence"
    );
    assert!(!runbook.contains("recovery stops at `doctor-trouble`"));
}

#[test]
fn a_lone_metadata_debris_after_a_kill_inside_seal_is_noted_and_in_the_way_of_nothing() {
    // Maintenance M16 (ledger PK-46). Handoff 004's P15 stopped here at `doctor-trouble`, and the wiki stayed unable to
    // take writes, because only a commit clears the warning and only the service commits.
    let Some(w) = World::new("debris", LeaseConfig::default()) else {
        return;
    };
    a_lone_metadata_debris_is_noted_and_recovery_finishes(&w);
}

#[test]
fn on_0_57_0_the_next_acceptances_commit_clears_the_metadata_debris_recovery_left() {
    // Maintenance M16, through the real state on 0.57.0 (the host's prikk, found on `PATH` wherever it stands).
    let Some(w) = World::new_057("debris-057", LeaseConfig::default()) else {
        return;
    };
    a_lone_metadata_debris_is_noted_and_recovery_finishes(&w);
}

#[test]
fn a_debris_warning_beside_a_pending_cleanup_is_in_the_way_of_neither_the_plan_nor_the_step() {
    // Maintenance M23 (decision 1, T-d): "debris is never a stop" is true beside a pending cleanup too. The two cannot
    // be made together in one repository (the debris needs an empty queue, the cleanup a queue equal to the tip's
    // patches), so the cleanup is real and `doctor` is the stand-in's canned report of both, as prikk would print them.
    let Some(w) = World::new("cleanup-debris", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let blocks = w.history();
    w.control(
        "say-doctor",
        "doctor repository: /x\nerror [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree but matching active publication state remains\n  recommendation: run signer-backed `prikk seal --allow-no-audit` for the affected ref; doctor does not sign or append\nwarning [PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS]: active WAL is empty but stale ref metadata remains\n  recommendation: no repair is required; the next guarded active-WAL append will replace stale metadata\nissue summary: errors=1, warnings=1, info=0\n",
    );
    // The canned report holds until a `seal` has run (the cleanup's step); after it prikk's own answer is the real one.
    w.control("say-until-seal", "");
    let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(doctor.is_only_cleanup_pending(), "{doctor:?}");
    assert!(doctor.diagnostics.iter().any(|d| d.is_metadata_debris()));
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(nth(&step_names(&shown), 0), &"complete-publication");
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        r.done.first().map(|d| d.step),
        Some("complete-publication"),
        "{:#?}",
        r.lines()
    );
    assert_eq!(w.history(), blocks);
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    w.uncontrol("say-doctor");
    w.uncontrol("say-until-seal");
    assert!(w.verifies());
}

#[test]
fn a_debris_warning_beside_an_unfinished_view_creation_is_in_the_way_of_neither_the_plan_nor_the_step()
 {
    // Maintenance M23 (decision 1, T-d), on the real state: a `seal` killed after it published leaves the debris
    // (recovery finishes that and leaves the warning, which only a commit clears); then a view's creation is killed
    // between its two writes. `doctor` names both, and the plan and the signer's step see the one lead.
    let Some(w) = World::new("view-debris", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &["leaves-only-the-metadata"]);
    let first = w.recover();
    assert_eq!(
        first.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        first.lines()
    );
    let base = w.tip();
    let act = inklkunn_store::ViewActRecord {
        id: inklkunn_store::ViewActId::new(),
        wiki: w.wiki.id.clone(),
        name: "draft".to_owned(),
        forked_from: "main".to_owned(),
        from_block: base.clone(),
        kind: inklkunn_core::views::ViewKind::Experimental,
        created_by: w.maintainer_id.clone(),
        created_at: 1,
    };
    w.store.begin_view_act(&act).unwrap();
    w.store
        .record_lease_holder(&inklkunn_store::LeaseHolder {
            wiki: w.wiki.id.clone(),
            holder: "a-process-that-is-gone".to_owned(),
            purpose: "view".to_owned(),
            acceptance: None,
            since: 1,
        })
        .unwrap();
    w.control("leads-after-branch-create", "");
    let answered = w.signer.create_view(
        &w.wiki.id,
        "heads/draft",
        MAIN,
        &ViewCreationAct::new(act.id.as_str()),
    );
    w.uncontrol("leads-after-branch-create");
    println!("the signer answered {answered:?}");
    let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(
        doctor.diagnostics.iter().any(|d| d.is_metadata_debris()),
        "{doctor:?}"
    );
    assert_eq!(doctor.only_ref_lead(), Some("heads/draft"), "{doctor:?}");
    assert!(!w.verifies(), "the half-published view stops every verify");
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(nth(&step_names(&shown), 0), &"complete-view-creation");
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        r.done.first().map(|d| d.step),
        Some("complete-view-creation")
    );
    assert!(w.verifies());
    let ended = w
        .store
        .view_acts(&w.wiki.id)
        .unwrap()
        .into_iter()
        .find(|e| e.act.id == act.id)
        .and_then(|e| e.end.map(|e| e.outcome));
    assert_eq!(ended, Some(inklkunn_store::ViewActOutcome::Created));
}

#[test]
fn an_empty_lock_file_fails_closed_and_the_runbooks_one_step_lets_recovery_go_on() {
    // Handoff 004, P15 / ruling 1. A kill the instant prikk created a lock file leaves it empty: `unlock` lists it with
    // `recorded pid: (unparseable)`, the strict reader refuses it, and recovery changes nothing.
    let Some(w) = World::new("emptylock", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Half done",
        &[("pages/Home.md", Some(&HOME.replace("Two.", "Dos.")))],
    );
    w.queued_acceptance(&p);
    let blocks = w.history();
    let lock = w.repo.join(".prikk/active/default/active.lock");
    std::fs::write(&lock, "").unwrap();
    let acts_before = w.store.recovery_acts(&w.wiki.id).unwrap().len();
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = w.recovery.recover(&w.wiki, mode);
        assert!(
            matches!(&r, Err(RecoveryError::Witness(why))
                if why.contains("prikk unlock") && why.contains("a recorded pid is not a number")),
            "{r:?}"
        );
    }
    assert_eq!(w.history(), blocks);
    assert_eq!(w.queue_count(), 1, "nothing was sealed");
    assert!(
        lock.exists(),
        "recovery does not clear a lock it cannot read"
    );
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert_eq!(
        w.store.recovery_acts(&w.wiki.id).unwrap().len(),
        acts_before,
        "a witness that cannot be read plans nothing and records no plan"
    );

    // docs/RECOVERY.md: what the operator sees, the commands, and what they print.
    let runbook = runbook();
    for said in [
        "a witness could not be read, so nothing is concluded: prikk unlock",
        "a recorded pid is not a number",
        "recorded pid: (unparseable)",
        "prikk unlock --lock <that path> --force",
        "no locks currently held",
    ] {
        assert!(runbook.contains(said), "the runbook says `{said}`");
    }
    assert_eq!(
        std::fs::metadata(&lock).unwrap().len(),
        0,
        "`ls -l` says size 0"
    );
    // The step is `prikk unlock --lock <path> --force`, which clears exactly that file; this crate may not spawn a process,
    // so the step itself is run against a real prikk in `inklkunn-signer`'s tests (`tests/recover.rs`), and here only
    // its effect is made.
    std::fs::remove_file(&lock).unwrap();
    // After: recovery reads again, makes its plan, and finishes what the kill left.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
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
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(w.queue_count(), 0);
    assert!(w.verifies());
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
    let have = wiki_state(&*w.storage, &w.layout, &w.wiki, &ReadConfig::default()).unwrap();
    let plan = plan_folder(&folder.0, &have, false, &ReadConfig::default()).unwrap();
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
    let have = wiki_state(&*w.storage, &w.layout, &w.wiki, &ReadConfig::default()).unwrap();
    let plan = plan_folder(&folder.0, &have, false, &ReadConfig::default()).unwrap();
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
fn a_queued_patch_nobody_describes_is_never_taken_back_by_a_prikk_without_the_verb() {
    // Maintenance M3, M28. `uncommit` came with 0.57.0: the first prikk on `PATH` (CI's is the floor) answers "no verb",
    // and nothing is ever taken back. The take-back itself is the next test's, on a prikk that has the verb.
    let Some(w) = World::new_floor("take-back-floor", LeaseConfig::default()) else {
        return;
    };
    take_back_by_a_persons_word(w, false);
}

#[test]
fn on_0_57_0_a_queued_patch_nobody_describes_is_taken_back_only_when_a_person_types_its_id() {
    // Maintenance M3, prikk 0.57.0, ledger PK-43; M28: the first prikk 0.57.0 or newer on `PATH`, whatever stands first.
    let Some(w) = World::new_057("take-back-057", LeaseConfig::default()) else {
        return;
    };
    take_back_by_a_persons_word(w, true);
}

/// The state is a sound queued patch that no recorded act describes, on a prikk with the verb (`verb`) or without.
fn take_back_by_a_persons_word(w: World, verb: bool) {
    let planted = "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n";
    w.plant_queued_patch("pages/Home.md", planted);
    w.flag(RecoveryReason::Interrupted);
    let before = w.history();
    let id = w.queued_patch_id();
    let confirm = |given: &str| RecoverOptions {
        discard_queued_patch: Some(given.to_owned()),
    };

    // Without a word from a person, a run stops as it always did; with the verb it also says what prikk would do and
    // the command that confirms it (M5: only a run asks prikk for the preview, under the lease). Nothing is taken back.
    let shown = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::Run, &RecoverOptions::default())
        .unwrap();
    assert_eq!(shown.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(shown.plan.steps.is_empty() && shown.plan.ops().is_empty());
    let has_verb = shown
        .plan
        .notes
        .iter()
        .any(|n| n.contains("prikk can take the queued patch"));
    // Whether the prikk has the verb is not for the code under test to say: `uncommit` is 0.57.0's, and the caller chose
    // the prikk by it.
    assert_eq!(has_verb, verb, "{:?}", shown.plan.notes);
    if !has_verb {
        assert!(
            shown
                .plan
                .notes
                .iter()
                .any(|n| n.contains("has no `uncommit`")),
            "{:?}",
            shown.plan.notes
        );
        // An older prikk: the person's word changes nothing, and says so.
        let r = w
            .recovery
            .recover_with(&w.wiki, RecoveryMode::Run, &confirm(&id))
            .unwrap();
        assert_eq!(r.plan.stop, Some(Stop::ForeignQueuedPatch));
        assert!(r.done.is_empty());
        assert!(
            r.plan.notes.iter().any(|n| n.contains("was not used")),
            "{:?}",
            r.plan.notes
        );
        assert_eq!(w.queue_count(), 1, "the patch is still queued");
        return;
    }
    assert!(
        shown
            .plan
            .notes
            .iter()
            .any(|n| n.contains(&format!("--discard-queued-patch {id}"))),
        "{:?}",
        shown.plan.notes
    );
    assert_eq!(w.queue_count(), 1);

    // Another id, a short one, the right id in the wrong case: none of them is the confirmation.
    for wrong in ["0".repeat(64), id[..10].to_owned(), id.to_uppercase()] {
        let r = w
            .recovery
            .recover_with(&w.wiki, RecoveryMode::Run, &confirm(&wrong))
            .unwrap();
        assert_eq!(r.plan.stop, Some(Stop::ForeignQueuedPatch), "{wrong}");
        assert!(r.done.is_empty() && r.plan.ops().is_empty(), "{wrong}");
        assert_eq!(w.queue_count(), 1, "{wrong}");
    }
    assert_eq!(w.history(), before);

    // The right id: the patch leaves the queue, the worktree goes back to the tip, the repository verifies, the flag
    // goes, the history is untouched, and the plan the act recorded names the patch the person confirmed.
    let r = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::Run, &confirm(&id))
        .unwrap();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    let steps: Vec<&str> = r.done.iter().map(|d| d.step).collect();
    assert_eq!(
        steps,
        [
            "discard-queued",
            "restore-worktree",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(
        nth(&r.done, 0).detail.contains(&id)
            && nth(&r.done, 0).detail.contains("--recovery-restore"),
        "{:?}",
        r.done
    );
    assert_eq!(w.queue_count(), 0, "the patch left the queue");
    assert_eq!(
        w.history(),
        before,
        "no block was made, none was taken away"
    );
    assert!(w.verifies());
    let home = std::fs::read_to_string(w.repo.join("pages/Home.md")).unwrap();
    assert_ne!(
        home, planted,
        "the worktree is the tip's again, not the patch's result"
    );
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    let recorded = acts
        .iter()
        .map(|a| a.act.plan.clone())
        .find(|p| p.contains("discard-queued"))
        .expect("the plan that was acted on is recorded");
    assert!(recorded.contains(&id), "{recorded}");
    assert!(
        w.audit()
            .iter()
            .any(|(a, o, d)| a == "recovery-step" && o == "allowed" && d == "discard-queued")
    );
    // Taking back a patch is not something a second run does again: nothing is queued, so nothing is offered.
    let again = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::Run, &confirm(&id))
        .unwrap();
    assert_eq!(again.outcome, RecoveryOutcome::NothingToDo);
}

#[test]
fn showing_the_plan_with_a_persons_word_takes_nothing_back_and_does_not_ask_prikk_either() {
    // `--plan` changes nothing: the command line refuses it beside the confirmation, and the library, shown the same
    // pair, plans the stop, says the word was not used, and asks prikk for nothing (M5: the preview takes prikk's lock).
    let Some(w) = World::new("take-back-plan", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    let shown = w
        .recovery
        .recover_with(
            &w.wiki,
            RecoveryMode::PlanOnly,
            &RecoverOptions {
                discard_queued_patch: Some(id.clone()),
            },
        )
        .unwrap();
    assert_eq!(shown.outcome, RecoveryOutcome::Planned);
    assert!(shown.done.is_empty());
    assert_eq!(shown.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert!(shown.plan.steps.is_empty() && shown.plan.ops().is_empty());
    assert!(
        shown
            .plan
            .notes
            .iter()
            .any(|n| n.starts_with(&format!("`--discard-queued-patch {id}` was not used: "))),
        "{:?}",
        shown.plan.notes
    );
    assert_eq!(
        w.uncommit_calls(),
        0,
        "a plan shown never asks for uncommit"
    );
    assert_eq!(w.queue_count(), 1, "looking took nothing back");
}

#[test]
fn a_plan_shown_never_asks_a_prikk_without_the_verb_for_the_preview() {
    // Maintenance M5, M28: the first prikk on `PATH` (the floor in CI), which has no verb to ask.
    let Some(w) = World::new_floor("preview-lease-floor", quick(300)) else {
        return;
    };
    a_plan_shown_never_asks_for_the_preview_and_a_run_asks_it_under_the_lease(w, false);
}

#[test]
fn on_0_57_0_a_plan_shown_never_asks_prikk_for_the_preview_and_a_run_asks_it_under_the_lease() {
    // Maintenance M5, M28: the first prikk 0.57.0 or newer on `PATH`.
    let Some(w) = World::new_057("preview-lease-057", quick(300)) else {
        return;
    };
    a_plan_shown_never_asks_for_the_preview_and_a_run_asks_it_under_the_lease(w, true);
}

/// Maintenance M5. `uncommit --plan-only` takes prikk's active lock, so asking is writing: `recover --plan` (the
/// library's `PlanOnly`) must never ask, with or without the verb (`verb`), and a run asks once, holding the wiki's
/// lease.
fn a_plan_shown_never_asks_for_the_preview_and_a_run_asks_it_under_the_lease(w: World, verb: bool) {
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    // Were prikk asked for the preview, it would fail visibly; it is not. (Whether it has the verb is asked, with no
    // patch id, which takes no lock and is not the preview: M21. The stand-in lets that question through.)
    w.control("fail-uncommit", "");
    for _ in 0..2 {
        let shown = w
            .recovery
            .recover_with(&w.wiki, RecoveryMode::PlanOnly, &RecoverOptions::default())
            .unwrap();
        assert_eq!(shown.plan.stop, Some(Stop::ForeignQueuedPatch));
        assert!(
            shown.plan.notes.iter().any(|n| if verb {
                n.contains("`recover --plan` does not ask prikk")
                    && n.contains(&w.wiki.id.to_string())
            } else {
                n.contains("this prikk has no `uncommit`")
            }),
            "{:?}",
            shown.plan.notes
        );
        assert!(
            shown
                .plan
                .notes
                .iter()
                .all(|n| !n.contains("could not be read") && !n.contains("prikk says")),
            "{:?}",
            shown.plan.notes
        );
    }
    assert_eq!(
        w.uncommit_calls(),
        0,
        "a plan shown never asks for uncommit"
    );
    assert!(w.store.recovery_acts(&w.wiki.id).unwrap().is_empty());
    w.uncontrol("fail-uncommit");

    // A run asks, and while prikk is being asked the wiki's lease is held (the stand-in waits inside `uncommit`).
    w.control("hold-uncommit", "");
    let report = std::thread::scope(|s| {
        let run = s.spawn(|| {
            w.recovery
                .recover_with(&w.wiki, RecoveryMode::Run, &RecoverOptions::default())
        });
        let started = w.ctl.join("uncommit-started");
        for _ in 0..400 {
            if started.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let held = started.exists()
            && matches!(
                w.leases.acquire(&w.wiki.id, Purpose::Recovery, None),
                Err(inklkunn_core::LeaseError::Busy(_))
            );
        w.control("release", "");
        let report = run.join().expect("the run").expect("a run");
        assert!(held, "prikk was asked while the lease was not held");
        report
    });
    assert!(report.under_lease);
    assert_eq!(report.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert_eq!(w.uncommit_calls(), 1, "a run asks exactly once");
    assert_eq!(w.queue_count(), 1);
    assert!(report.done.is_empty());
    let _ = id;
}

#[test]
fn a_stale_lock_beside_an_orphan_patch_clears_nothing_on_a_prikk_without_the_verb() {
    // Maintenance M6, M12, M28: the first prikk on `PATH` (the floor in CI) has no preview to read afterwards.
    let Some(w) = World::new_floor("lock-and-orphan-floor", LeaseConfig::default()) else {
        return;
    };
    a_stale_lock_beside_an_orphan_patch(w, false);
}

#[test]
fn on_0_57_0_a_stale_lock_beside_an_orphan_patch_is_cleared_then_the_stop_comes_with_the_preview_and_the_offer_works_next_run()
 {
    // Maintenance M6, M28: the first prikk 0.57.0 or newer on `PATH`.
    let Some(w) = World::new_057("lock-and-orphan-057", LeaseConfig::default()) else {
        return;
    };
    a_stale_lock_beside_an_orphan_patch(w, true);
}

/// Maintenance M6. The lock a killed process left holds prikk's lock, so the preview cannot be read; the plan used to
/// stop with every step gone, the clearing included, and the next run met the same lock. Now the run clears it, asks
/// for the preview, and stops with it; the person's word is used by the run after. Without the verb (`verb` false)
/// nothing is cleared (M12).
fn a_stale_lock_beside_an_orphan_patch(w: World, verb: bool) {
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    let lock = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    let before = w.history();

    // Whether prikk has the verb is asked of prikk without a patch id, which takes no lock: it is answered with the
    // lock held, on both sides of 0.57.0 (M12).
    prikk_probes::assert_has_verb(&w.recovery_witnesses(), &w.wiki.id, verb);

    // A plan shown: with the verb the clearing is the one step and the stop follows; without it the stop clears
    // nothing (M12: no preview can follow). Nothing is asked for the preview and nothing is done.
    let shown = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::PlanOnly, &RecoverOptions::default())
        .unwrap();
    assert_eq!(shown.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert_eq!(
        shown.plan.steps.iter().map(Step::name).collect::<Vec<_>>(),
        if verb {
            vec!["clear-locks"]
        } else {
            Vec::new()
        }
    );
    assert!(Path::new(&lock).exists());
    assert_eq!(w.uncommit_calls(), 0);

    if !verb {
        // A run clears nothing either: the lock stays, the wiki stays flagged, and the plan says why.
        let run = w
            .recovery
            .recover_with(
                &w.wiki,
                RecoveryMode::Run,
                &RecoverOptions {
                    discard_queued_patch: Some(id.clone()),
                },
            )
            .unwrap();
        assert_eq!(run.plan.stop, Some(Stop::ForeignQueuedPatch));
        assert!(run.done.is_empty() && run.plan.steps.is_empty());
        assert!(Path::new(&lock).exists(), "nothing was cleared");
        assert_eq!(w.uncommit_calls(), 0, "no preview was asked for");
        let notes = run.plan.notes.join("\n");
        assert!(
            notes.contains("has no `uncommit`") && notes.contains("nothing is cleared"),
            "{notes}"
        );
        assert_eq!(w.queue_count(), 1);
        assert_eq!(w.history(), before);
        assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
        return;
    }

    // The run with the word typed: the lock is cleared, the preview is shown, the stop stays, the word is not used yet.
    let first = w
        .recovery
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(id.clone()),
            },
        )
        .unwrap();
    assert!(
        matches!(&first.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:#?}",
        first.lines()
    );
    assert_eq!(first.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert_eq!(
        first.done.iter().map(|d| d.step).collect::<Vec<_>>(),
        ["clear-locks"]
    );
    assert!(!Path::new(&lock).exists(), "the stale lock is cleared");
    assert_eq!(w.queue_count(), 1);
    assert_eq!(w.history(), before);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert_eq!(w.uncommit_calls(), 1, "asked once, after the lock was gone");
    let notes = first.plan.notes.join("\n");
    assert!(
        notes.contains(&format!("prikk can take the queued patch {id} back"))
            && notes.contains("prikk says: ")
            && !notes.contains("could not be read"),
        "{notes}"
    );
    assert!(
        notes.contains(&format!(
            "inklkunn wiki recover {} --discard-queued-patch {id}",
            w.wiki.id
        )),
        "{notes}"
    );
    assert!(
        first
            .plan
            .discard_unused
            .as_deref()
            .is_some_and(|why| why.contains("stale lock")),
        "{:?}",
        first.plan.discard_unused
    );

    // The next run, with the word: the offer works.
    let second = w
        .recovery
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(id.clone()),
            },
        )
        .unwrap();
    assert_eq!(
        second.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        second.lines()
    );
    assert_eq!(w.queue_count(), 0);
    assert!(w.verifies());
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    assert!(second.plan.discard_unused.is_none());
}

#[test]
fn on_0_57_0_a_held_stale_lock_makes_the_preview_refuse_and_the_run_that_clears_it_reads_it() {
    // Maintenance M12 (ledger PK-47, measured on 0.57.0): `uncommit --plan-only` takes the active lock, so a stale
    // lock that is held makes it refuse. That is why the run clears the lock first, and the preview it then reads is
    // prikk's own. Needs a 0.57.0 prikk: the host's, found on `PATH` wherever it stands.
    let Some(w) = World::new_057("held-lock-refuses", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    let lock = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    let witnesses = w.recovery_witnesses();
    // The question whether the verb exists is answered while the lock is held; the preview itself is refused.
    prikk_probes::assert_has_verb(&w.recovery_witnesses(), &w.wiki.id, true);
    let refused = witnesses.uncommit_plan(&w.wiki.id, &id);
    assert!(
        refused.is_err(),
        "a held stale lock makes `uncommit --plan-only` refuse: {refused:?}"
    );
    assert!(Path::new(&lock).exists(), "the refusal changed nothing");
    assert_eq!(w.queue_count(), 1);
    // The run clears the lock and then reads the preview.
    let run = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::Run, &RecoverOptions::default())
        .unwrap();
    assert_eq!(run.plan.stop, Some(Stop::ForeignQueuedPatch));
    assert_eq!(
        run.done.iter().map(|d| d.step).collect::<Vec<_>>(),
        ["clear-locks"]
    );
    assert!(!Path::new(&lock).exists());
    let notes = run.plan.notes.join("\n");
    assert!(
        notes.contains(&format!("prikk can take the queued patch {id} back"))
            && notes.contains("prikk says: ")
            && !notes.contains("could not be read"),
        "{notes}"
    );
    assert!(witnesses.uncommit_plan(&w.wiki.id, &id).unwrap().is_some());
    assert_eq!(w.queue_count(), 1, "nothing was taken back");
}

/// One orphan patch queued under the word: the plan shown first, then the run, on the same state, with the lock
/// planted or not. What the plan says is compared with what the run did (maintenance M21).
fn plan_then_run(w: &World, lock: bool, verb: bool) {
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    let lock_path = lock.then(|| {
        plant_lock(
            &w.repo,
            "active/default/active.lock",
            2_147_483_000,
            "active",
        )
    });
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    let word = RecoverOptions {
        discard_queued_patch: Some(id.clone()),
    };
    let planned = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::PlanOnly, &word)
        .unwrap();
    let promise = planned
        .plan
        .discard_unused
        .clone()
        .expect("a plan says what a run does with the word");
    let notes = planned.plan.notes.join("\n");
    assert_eq!(w.uncommit_calls(), 0, "a plan asks for no preview");
    assert_eq!(w.queue_count(), 1, "a plan takes nothing back");
    let run = w
        .recovery
        .recover_with(&w.wiki, RecoveryMode::Run, &word)
        .unwrap();
    let held = lock_path.as_deref().is_some_and(|l| Path::new(l).exists());
    match (verb, lock) {
        (false, _) => {
            // No `uncommit`: the plan says so, the run takes nothing back, clears nothing, and says the same.
            assert!(
                promise.contains("this prikk has no `uncommit`"),
                "{promise}"
            );
            assert!(!promise.contains("a run takes the patch back"), "{promise}");
            assert!(!notes.contains("shows prikk's own preview"), "{notes}");
            assert!(run.done.is_empty() && run.plan.steps.is_empty());
            assert_eq!(w.queue_count(), 1);
            assert_eq!(held, lock, "the lock is as it was");
            assert_eq!(run.plan.discard_unused.as_ref(), Some(&promise));
        }
        (true, true) => {
            // A stale lock stands in the way: the plan says the run clears it and stops with the preview.
            assert_eq!(
                promise,
                "a stale lock stands in the way: a run clears it and stops with the preview, and the next run takes the patch back"
            );
            assert_eq!(step_names(&planned), ["clear-locks"]);
            assert!(!promise.contains("only shows"), "{promise}");
            assert_eq!(
                run.done.iter().map(|d| d.step).collect::<Vec<_>>(),
                ["clear-locks"]
            );
            assert_eq!(w.queue_count(), 1, "the patch was not taken back");
            assert!(!held, "the lock was cleared");
            assert_eq!(w.uncommit_calls(), 1);
            assert!(
                run.plan.discard_unused.as_deref().is_some_and(
                    |why| why.contains("this run cleared it and stops with the preview")
                ),
                "{:?}",
                run.plan.discard_unused
            );
        }
        (true, false) => {
            // Nothing in the way: the plan says a run takes the patch back, and it does.
            assert_eq!(
                promise,
                "`recover --plan` only shows; a run takes the patch back"
            );
            assert!(
                run.done.iter().any(|d| d.step == "discard-queued"),
                "{:#?}",
                run.lines()
            );
            assert_eq!(w.queue_count(), 0, "the patch was taken back");
        }
    }
}

#[test]
fn a_plan_promises_what_the_run_does_with_the_queued_patch_on_a_prikk_without_the_verb() {
    // Maintenance M21 (T-b), M28. The plan used to promise the take-back without asking prikk whether it has the verb
    // (the floor has not: the run said so). The first prikk on `PATH` (CI's floor), with and without a stale lock.
    for lock in [false, true] {
        let Some(w) = World::new_floor(
            if lock {
                "promise-lock-floor"
            } else {
                "promise-clear-floor"
            },
            LeaseConfig::default(),
        ) else {
            return;
        };
        plan_then_run(&w, lock, false);
    }
}

#[test]
fn on_0_57_0_a_plan_without_a_lock_says_a_run_takes_the_patch_back_and_it_does() {
    // Maintenance M21 (T-b), M28: the first prikk 0.57.0 or newer on `PATH`, nothing in the way.
    let Some(w) = World::new_057("promise-clear-057", LeaseConfig::default()) else {
        return;
    };
    plan_then_run(&w, false, true);
}

#[test]
fn on_0_57_0_a_plan_with_a_stale_lock_says_the_run_clears_it_and_stops_with_the_preview() {
    // Maintenance M21 (T-b), the stale lock on 0.57.0: the plan's sentence, then the run's deed.
    let Some(w) = World::new_057("promise-lock-057", LeaseConfig::default()) else {
        return;
    };
    plan_then_run(&w, true, true);
}

#[test]
fn on_0_57_0_a_plan_with_a_lock_whose_holder_appears_running_says_the_run_clears_it_and_it_does() {
    // Maintenance M31 (W-f) said the run stops here, because prikk 0.57.0's `unlock --yes` refuses a holder that appears
    // running (measured, M10; the stand-in prints that listing and that refusal, because an old-style body reads
    // `unknown` on 0.57.0 whatever its pid is). M50: the run clears with `--force` on the lease record's word (PL-8),
    // which that prikk accepts, so the plan promises the clear and the preview, exactly as for any stale lock, and the
    // run does exactly that.
    let Some(w) = World::new_057("promise-running-057", LeaseConfig::default()) else {
        return;
    };
    w.control("appears-running", "");
    plan_then_run(&w, true, true);
    let asked = w.unlock_calls();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked
            .iter()
            .all(|a| a.ends_with(" --force") && !a.contains("--yes")),
        "{asked:?}"
    );
    w.uncontrol("appears-running");
}

#[test]
fn on_0_57_0_a_plain_stale_lock_whose_holder_appears_running_is_planned_and_cleared_like_any_other()
{
    // Maintenance M36 (Z-d) planned a stop at a holder prikk calls running (0.57.0's refusal of `--yes`). M50: the run
    // clears it with `--force`, the plan says only what it says of every stale lock, and nothing is left held.
    let Some(w) = World::new_057("plain-running-057", LeaseConfig::default()) else {
        return;
    };
    let lock = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    w.flag(RecoveryReason::Interrupted);
    w.control("appears-running", "");
    let planned = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(nth(&step_names(&planned), 0), &"clear-locks");
    let line = nth(&planned.plan.steps, 0).line();
    assert!(
        line.contains("active recorded pid 2147483000 prikk says \"appears running")
            && !line.contains("stops at this step")
            && !line.contains("does not clear a holder"),
        "{line}"
    );
    let run = w.recover();
    assert_eq!(
        run.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        run.lines()
    );
    assert!(!Path::new(&lock).exists(), "the lock was cleared");
    let asked = w.unlock_calls();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked
            .iter()
            .all(|a| a.ends_with(" --force") && !a.contains("--yes")),
        "{asked:?}"
    );
    w.uncontrol("appears-running");
}

#[test]
fn a_step_that_failed_keeps_its_sentence_when_the_audit_fails_after_it() {
    // Maintenance M31 (W-e). The lock is cleared, the listing that checks it fails, and then the trail refuses the
    // failure's own record: the error used to say *nothing was done*, with the step's sentence lost. Needs the preview
    // to follow (0.57.0).
    let Some(w) = World::new_057("failed-step-then-audit", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    let lock = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    w.flag(RecoveryReason::Interrupted);
    w.control("list-fails-after-clear", &lock);
    let refusing = recovery_writing_to(
        &w,
        Arc::new(TrailThatRefusesFailures {
            inner: w.store.clone(),
        }),
    );
    let err = refusing
        .recover_with(&w.wiki, RecoveryMode::Run, &RecoverOptions::default())
        .unwrap_err();
    w.uncontrol("list-fails-after-clear");
    assert!(!Path::new(&lock).exists(), "the lock was cleared");
    assert!(err.steps_done().is_empty(), "no step finished: {err:?}");
    let failed = err.failed_step().expect("the error carries the step");
    assert_eq!(failed.step, "clear-locks");
    assert!(
        failed
            .why
            .starts_with("1 lock(s) cleared, but the locks could not be listed again to check: "),
        "{failed:?}"
    );
    assert!(matches!(&err, RecoveryError::AfterSteps { cause, .. }
        if matches!(**cause, RecoveryError::Audit(_))));
    let said = err.to_string();
    assert!(
        said.contains("the audit trail could not be written (")
            && said.contains("1 lock(s) cleared, but the locks could not be listed again")
            && !said.contains("nothing was done"),
        "{said}"
    );
}

#[test]
fn a_clear_that_worked_is_not_reported_as_not_cleared_when_what_comes_after_it_fails() {
    // Maintenance M22 (T-g): the lock is cleared, and then either the listing that checks it fails or the lock is
    // there again. The step's reason is a whole sentence and says which. Needs the preview to follow (0.57.0).
    for (case, relock) in [
        ("list-fails-after-clear", false),
        ("relock-after-clear", true),
    ] {
        let Some(w) = World::new_057(&format!("clear-then-{case}"), LeaseConfig::default()) else {
            return;
        };
        w.plant_queued_patch(
            "pages/Home.md",
            "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
        );
        let lock = plant_lock(
            &w.repo,
            "active/default/active.lock",
            2_147_483_000,
            "active",
        );
        w.flag(RecoveryReason::Interrupted);
        w.control(case, &lock);
        let r = w
            .recovery
            .recover_with(&w.wiki, RecoveryMode::Run, &RecoverOptions::default())
            .unwrap();
        w.uncontrol(case);
        let RecoveryOutcome::Stopped {
            at: Some("clear-locks"),
            why,
        } = &r.outcome
        else {
            panic!("{:#?}", r.lines());
        };
        assert!(r.done.is_empty(), "the step did not finish");
        assert!(!why.contains("was not cleared"), "{why}");
        if !relock {
            assert!(
                why.starts_with(
                    "1 lock(s) cleared, but the locks could not be listed again to check: "
                ) && why.contains("injected failure of unlock (listing)"),
                "{why}"
            );
        } else {
            assert_eq!(
                why,
                "1 lock(s) cleared, but still held: active (recorded pid 2147483000)"
            );
        }
        assert_eq!(w.queue_count(), 1);
    }
}

#[test]
fn a_run_with_nothing_to_recover_says_the_word_was_not_used() {
    // Maintenance M4: a person's word on a wiki that needs nothing is not left in the record as an act.
    let Some(w) = World::new("nothing-to-recover", LeaseConfig::default()) else {
        return;
    };
    let given = "ab".repeat(32);
    let r = w
        .recovery
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(given.clone()),
            },
        )
        .unwrap();
    assert_eq!(r.outcome, RecoveryOutcome::NothingToDo);
    assert!(
        r.plan
            .notes
            .iter()
            .any(|n| n.starts_with(&format!("`--discard-queued-patch {given}` was not used: "))),
        "{:?}",
        r.plan.notes
    );
    assert!(r.plan.discard_unused.is_some());
    assert_eq!(w.uncommit_calls(), 0);
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
    // prikk 0.55.0 and 0.56.0 call the live pid of that old-style body "appears running". 0.57.0 asks for the holder's
    // start time, which an old body records none of, and says "unknown" (measured, maintenance M3): the lock is the
    // same, and what prikk said of it is recorded either way. The refusal 0.57.0 adds for a holder that appears
    // running needs a body only a 0.57.0 prikk writes; it is not planted here.
    let said = listed
        .iter()
        .find(|l| l.path == live)
        .map(|l| l.liveness.clone())
        .unwrap_or_default();
    assert!(
        said.starts_with("appears running")
            || said.starts_with("unknown: written by an older prikk"),
        "the setup: prikk does not call the planted lock's process dead ({listed:?})"
    );
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(nth(&step_names(&shown), 0), &"clear-locks");
    // M50: prikk 0.58.0 refuses `unlock --yes` for a holder whose liveness it cannot establish (an old-style body is
    // one), 0.57.0 for one that appears running. The stand-in refuses `--yes` whatever prikk runs underneath, so that
    // this holds on every prikk in the range: the step is `--force`, on the lease record's word.
    w.control("refuse-yes", "");
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(nth(&step_names(&r), 0), &"clear-locks");
    assert!(
        nth(&r.done, 0).detail.contains("2 lock(s) cleared"),
        "{:?}",
        r.done
    );
    let asked = w.unlock_calls();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(
        asked.iter().all(|a| a.starts_with("unlock --lock ")
            && a.ends_with(" --force")
            && !a.contains("--yes")),
        "{asked:?}"
    );
    assert!(witnesses.locks(&w.wiki.id).unwrap().is_empty());
    assert!(!Path::new(&live).exists() && !Path::new(&dead).exists());
    // The plan recorded what prikk said and that it was not consulted.
    let record = w.store.recovery_acts(&w.wiki.id).unwrap().remove(0);
    assert!(record.act.plan.contains(&said), "{}", record.act.plan);
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

// ---- the corrections of the review of 2026-10-31 (handoff 004, P1 – P3, P7, P8) ----

impl World {
    /// A recovery that sees `table` as the host's processes.
    fn recovery_seeing(&self, table: Arc<dyn ProcessTable>) -> Recovery {
        Recovery::new(RecoveryParts {
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            proposals: self.store.clone(),
            storage: self.storage.clone(),
            witnesses: Arc::new(Witnesses::new(
                &self.standin,
                Sandbox::Unconfined,
                self.layout.clone(),
            )),
            signer: self.signer.clone() as Arc<dyn Recover>,
            leases: self.leases.clone(),
            layout: self.layout.clone(),
            audit: self.store.clone(),
        })
        .with_reads(self.reads.clone())
        .with_processes(table)
    }

    /// An acceptance whose `seal` did its work and then reported a failure: the change is in the history, the
    /// records are not written, the wiki is flagged.
    fn sealed_but_not_recorded(&self, p: &ProposalRecord) {
        self.control("runfail-seal", "");
        let err = self.accept(p).unwrap_err();
        self.uncontrol("runfail-seal");
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
        assert_eq!(self.queue_count(), 0, "the seal did its work");
    }
}

/// A table of processes that is only what a test put on it.
struct Table(Vec<ProcessInfo>);

impl ProcessTable for Table {
    fn processes(&self) -> Vec<ProcessInfo> {
        self.0.clone()
    }
}

#[test]
fn an_ambiguous_commit_is_queued_never_undone_and_recovery_seals_it() {
    // P1. `commit` did its work (its patch is durable) and the call reported a failure, as a call stopped at its
    // deadline does. The files stay, the answer is `Queued`, and recovery completes the change with `seal` alone.
    let Some(w) = World::new("ambiguous-commit", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let before = w.history();
    w.control("runfail-commit", "");
    let err = w.accept(&p).unwrap_err();
    w.uncontrol("runfail-commit");
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
    assert_eq!(w.queue_count(), 1, "the patch is durable");
    assert!(
        std::fs::read_to_string(w.repo.join("pages/Home.md"))
            .unwrap()
            .contains("Uno."),
        "the files were not undone"
    );
    assert_eq!(w.history(), before);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(
        step_names(&r),
        [
            "seal-queued",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert_eq!(w.history().len(), before.len() + 1);
    assert!(w.page("pages/Home.md").contains("Uno."));
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert_eq!(w.queue_count(), 0);
    assert!(w.verifies() && w.reconciled().is_clean());
}

#[test]
fn a_retirement_never_freezes_the_wiki_and_recovery_passes_verify_with_a_note() {
    // P2 (DR-10). After a retirement `prikk verify` fails by publication trust for what the retired key sealed.
    // Writes go on, and recovery's `verify` step passes with a note; prikk itself still says the repository does
    // not verify (the badge's *Verification failed* is the known cost).
    let Some(w) = World::new("retired", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    assert!(
        !w.verifies(),
        "the canary: prikk cannot verify what a retired key sealed (PK-34)"
    );
    for (n, text) in ["Uno.", "Dos.", "Tres."].into_iter().enumerate() {
        let p = w.propose(
            &format!("Change {n}"),
            &[("pages/Home.md", Some(&HOME.replace("One.", text)))],
        );
        match w.accept(&p) {
            Ok(inklkunn_core::Accepted::Signed(s)) => {
                assert_eq!(s.unconfirmed, None, "the change {n} was confirmed");
            }
            other => panic!("a retirement froze the wiki at change {n}: {other:?}"),
        }
        assert_eq!(w.state(&p), ProposalState::Accepted);
    }
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    // A queued patch beside the retirement: recovery completes it and its verify step passes with the note.
    let q = w.propose(
        "Half done",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    w.queued_acceptance(&q);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    let verify = r
        .done
        .iter()
        .find(|d| d.step == "verify")
        .expect("verify ran");
    assert!(
        verify.detail.contains("only because of a retired key"),
        "{}",
        verify.detail
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.state(&q), ProposalState::Accepted);
}

/// The real signer for everything but the one question of a retired key, which it answers truthfully as many
/// times as it is told to and "no" after that.
struct ExcusesFirst(Arc<Signer>, std::sync::atomic::AtomicUsize);

impl Recover for ExcusesFirst {
    fn verify_after_retirement(
        &self,
        wiki: &inklkunn_store::WikiId,
    ) -> Result<Option<inklkunn_signer::recover::RetirementExcuse>, inklkunn_core::RecoverError>
    {
        use std::sync::atomic::Ordering;
        let left = self.1.load(Ordering::SeqCst);
        if left == 0 {
            return Ok(None);
        }
        self.1.store(left - 1, Ordering::SeqCst);
        self.0.verify_after_retirement(wiki)
    }

    fn seal_queued(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &AcceptanceToken,
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

#[test]
fn a_failure_the_signer_does_not_excuse_keeps_the_flag_even_after_a_retirement() {
    // The recovery honours the signer's answer, at the plan (where `doctor`'s errors are damage unless the signer
    // excuses them) and again at the `verify` step: when it does not say a retired key accounts for the failure (a
    // second failure beside it, say), the recovery stops and the flag is kept.
    let Some(w) = World::new("retired-plus", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    let q = w.propose(
        "Half done",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    w.queued_acceptance(&q);
    let recovery_with = |excuses: usize| {
        Recovery::new(RecoveryParts {
            acceptances: w.store.clone(),
            acts: w.store.clone(),
            proposals: w.store.clone(),
            storage: w.storage.clone(),
            witnesses: Arc::new(Witnesses::new(
                &w.standin,
                Sandbox::Unconfined,
                w.layout.clone(),
            )),
            signer: Arc::new(ExcusesFirst(
                w.signer.clone(),
                std::sync::atomic::AtomicUsize::new(excuses),
            )) as Arc<dyn Recover>,
            leases: w.leases.clone(),
            layout: w.layout.clone(),
            audit: w.store.clone(),
        })
    };
    // Never excused: `doctor`'s publication-trust errors are damage, and the plan stops on them.
    let r = recovery_with(0)
        .recover(&w.wiki, RecoveryMode::Run)
        .unwrap();
    assert_eq!(r.plan.stop, Some(Stop::DoctorTrouble), "{:#?}", r.lines());
    assert_eq!(w.queue_count(), 1, "nothing was sealed");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    // Excused at the plan and not at the `verify` step: the steps run up to it, and it stops there.
    let r = recovery_with(1)
        .recover(&w.wiki, RecoveryMode::Run)
        .unwrap();
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
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

/// The real signer's answer about a retirement with the blocks left out of what it excuses: `doctor` still names them.
struct ForgetsTheBlocks(Arc<Signer>);

impl Recover for ForgetsTheBlocks {
    fn verify_after_retirement(
        &self,
        wiki: &inklkunn_store::WikiId,
    ) -> Result<Option<inklkunn_signer::recover::RetirementExcuse>, inklkunn_core::RecoverError>
    {
        Ok(self.0.verify_after_retirement(wiki)?.map(|mut excuse| {
            excuse.blocks.clear();
            excuse
        }))
    }

    fn seal_queued(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &AcceptanceToken,
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

#[test]
fn recovery_takes_out_only_the_doctor_findings_the_excuse_names() {
    // P13. `doctor` reports one error for each block, ref state and ref update without a trusted signature. Only
    // those the signer's answer names are a note; a finding of that code that it does not name is damage and the
    // plan stops on it, though the answer is "yes" (before: every finding of the code was taken out at once).
    let Some(w) = World::new("retired-names", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    let q = w.propose(
        "Half done",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    w.queued_acceptance(&q);
    let recovery_with = |signer: Arc<dyn Recover>| {
        Recovery::new(RecoveryParts {
            acceptances: w.store.clone(),
            acts: w.store.clone(),
            proposals: w.store.clone(),
            storage: w.storage.clone(),
            witnesses: Arc::new(Witnesses::new(
                &w.standin,
                Sandbox::Unconfined,
                w.layout.clone(),
            )),
            signer,
            leases: w.leases.clone(),
            layout: w.layout.clone(),
            audit: w.store.clone(),
        })
    };
    let r = recovery_with(Arc::new(ForgetsTheBlocks(w.signer.clone())))
        .recover(&w.wiki, RecoveryMode::Run)
        .unwrap();
    assert_eq!(r.plan.stop, Some(Stop::DoctorTrouble), "{:#?}", r.lines());
    assert_eq!(w.queue_count(), 1, "nothing was sealed");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

#[test]
fn a_disagreement_that_touches_what_the_act_did_stops_the_recovery_with_the_flag_kept() {
    // P3. The act's change is in the history (its seal did its work and reported a failure) and a block that no
    // record explains was sealed after it: the history and the records disagree about a block newer than the
    // act's base, so the wiki is not called recovered.
    let Some(w) = World::new("find-newer", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.sealed_but_not_recorded(&p);
    let stranger = w.plant_sealed_block("pages/Other.md", "planted\n");
    let r = w.recover();
    let RecoveryOutcome::Stopped { at, why } = &r.outcome else {
        panic!("a disagreement was called recovered: {:#?}", r.lines());
    };
    assert_eq!(*at, Some("reconcile"), "{why}");
    assert!(
        why.contains("unrecorded-seal") && why.contains(&stranger),
        "the finding is named: {why}"
    );
    assert!(
        w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
        "the flag is kept"
    );
    // The wiki still refuses writes; the records the recovery did write stay written.
    let q = w.propose(
        "Another",
        &[(
            "pages/Another.md",
            Some("---\ntitle: Another\nid: pn\n---\n\na\n"),
        )],
    );
    assert!(matches!(
        w.accept(&q).unwrap_err(),
        AcceptFailure::NeedsRecovery(_)
    ));
    assert_eq!(w.state(&p), ProposalState::Accepted);
    // The finding is in the audit trail, once.
    let reported = w
        .store
        .read(0, 10_000)
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.event.principal == "system:reconciliation" && r.event.detail.contains(&stranger)
        })
        .count();
    assert_eq!(reported, 1);
}

#[test]
fn a_disagreement_older_than_the_act_is_reported_and_does_not_block_the_recovery() {
    // P3's other half: the same unrecorded block, but sealed before the act began. It is in the audit trail and
    // in the report, and the recovery that finished the act is not held hostage by it.
    let Some(w) = World::new("find-older", LeaseConfig::default()) else {
        return;
    };
    let stranger = w.plant_sealed_block("pages/Other.md", "planted\n");
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.sealed_but_not_recorded(&p);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert!(
        matches!(
            &r.reconciliation,
            Some(inklkunn_core::Reconciled::Findings(f)) if f.len() == 1 && f.first().is_some_and(|f| f.block_id() == stranger)
        ),
        "{:?}",
        r.reconciliation
    );
    let reconcile = r.done.iter().find(|d| d.step == "reconcile").unwrap();
    assert!(
        reconcile.detail.contains("do not block"),
        "{}",
        reconcile.detail
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.state(&p), ProposalState::Accepted);
}

#[test]
fn the_inline_reconciliation_reads_the_blocks_since_the_base_and_the_full_pass_still_finds_the_rest()
 {
    // P7. A block nobody explains, sealed long ago, is the full pass's to find (recovery's and the schedule's);
    // the reconciliation an acceptance runs under the lease looks at what the act wrote and says it is clean.
    let Some(w) = World::new("bounded", LeaseConfig::default()) else {
        return;
    };
    let stranger = w.plant_sealed_block("pages/Other.md", "planted\n");
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let signed = match w.accept(&p) {
        Ok(inklkunn_core::Accepted::Signed(s)) => s,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        signed.reconciliation,
        inklkunn_core::Reconciled::Clean,
        "the inline pass looks at the blocks since the base only"
    );
    let full = w.reconciled();
    assert!(
        full.findings.iter().any(|f| f.block_id() == stranger),
        "the full pass finds the old block: {:?}",
        full.findings
    );
}

#[test]
fn an_unclean_repository_at_the_start_of_an_acceptance_flags_the_wiki_and_names_what_it_found() {
    // P8: a queued patch no record explains, and a dirty worktree no record explains, are each their own error
    // (not a refusal of the change) and flag the wiki.
    let Some(w) = World::new("unclean-queued", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.plant_queued_patch("pages/Other.md", "planted\n");
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    let err = w.accept(&p).unwrap_err();
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
    let flag = w.store.recovery_flag(&w.wiki.id).unwrap().expect("flagged");
    assert_eq!(flag.reason, RecoveryReason::Queued);

    let Some(w) = World::new("unclean-dirty", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    std::fs::write(w.repo.join("pages/Stray.md"), "left behind\n").unwrap();
    let err = w.accept(&p).unwrap_err();
    assert!(
        matches!(
            &err,
            AcceptFailure::Incomplete {
                reason: RecoveryReason::WorktreeDirty,
                flagged: true,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(
        w.store.recovery_flag(&w.wiki.id).unwrap().map(|f| f.reason),
        Some(RecoveryReason::WorktreeDirty)
    );
    // Recovery puts it back and the wiki takes the change.
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert!(!w.repo.join("pages/Stray.md").exists());
    assert!(w.accept(&p).is_ok());
}

#[test]
fn a_foreign_process_in_the_repository_stops_every_step_of_a_recovery_not_only_a_lock() {
    // P8. A queued patch is waiting to be sealed and a `prikk` of somebody else's is working in the repository:
    // nothing is sealed, nothing is recorded as done, and the stop is named. Without the process it completes.
    let Some(w) = World::new("foreign-process", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.queued_acceptance(&p);
    let intruder = ProcessInfo {
        pid: 424_242,
        parent: 1,
        argv: vec![
            "prikk".to_owned(),
            "commit".to_owned(),
            w.repo.display().to_string(),
        ],
        cwd: Some(w.repo.display().to_string()),
        readable: true,
    };
    let seeing = w.recovery_seeing(Arc::new(Table(vec![intruder])));
    let r = seeing.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(r.plan.stop, Some(Stop::ForeignProcess));
    assert!(
        matches!(&r.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:#?}",
        r.lines()
    );
    assert!(r.done.is_empty(), "no step was taken");
    assert_eq!(w.queue_count(), 1, "nothing was sealed beside the process");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    // The process is gone: the same recovery completes.
    let quiet = w.recovery_seeing(Arc::new(Table(Vec::new())));
    let r = quiet.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(w.queue_count(), 0);
}

#[test]
fn an_acceptance_with_no_end_blocks_the_next_act_even_when_its_intent_was_never_written() {
    // P8: the guard saw open intents only. A process that died between the acceptance's record and its intent left
    // a pending acceptance and nothing else.
    let Some(w) = World::new("pending-no-intent", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "First",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let q = w.propose(
        "Second",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    let rec = inklkunn_store::AcceptanceRecord {
        id: inklkunn_store::AcceptanceId::new(),
        proposal: p.id.clone(),
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
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    let before = w.history();
    let err = w.accept(&q).unwrap_err();
    assert!(matches!(err, AcceptFailure::NeedsRecovery(_)), "{err:?}");
    assert_eq!(w.history(), before, "nothing was written beside it");
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

#[test]
fn a_retry_of_a_sealed_acceptance_settles_a_proposal_still_held_for_review() {
    // P8: the seal is in the history and its record says so, and the writes that move the proposal on both failed.
    // The retry says what is true and finishes the proposal.
    let Some(w) = World::new("already-signed", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "First",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let rec = inklkunn_store::AcceptanceRecord {
        id: inklkunn_store::AcceptanceId::new(),
        proposal: p.id.clone(),
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
    w.store.begin_acceptance(&rec).unwrap();
    w.store
        .finish_acceptance(&inklkunn_store::AcceptanceResult {
            acceptance: rec.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: w.tip(),
                patch_id: "e".repeat(64),
            },
            at: 4,
        })
        .unwrap();
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    let again = w.accept(&p).expect("a retry is told what is true");
    assert!(matches!(
        again,
        inklkunn_core::Accepted::AlreadySigned { .. }
    ));
    assert_eq!(w.state(&p), ProposalState::Accepted);
    // A second retry changes nothing.
    assert!(matches!(
        w.accept(&p),
        Ok(inklkunn_core::Accepted::AlreadySigned { .. })
    ));
}

#[test]
fn a_retirement_that_ended_without_its_record_stops_recovery_naming_the_commands_and_keeps_the_flag()
 {
    // Q1, W1. The `verify` after the removal could not run (the stand-in lets the two before it through): the key is out
    // of the trust store, the key record holds only the state to finish from, nothing is excused. The wiki is flagged
    // for a queued patch; recovery stops on the findings nothing excuses, names the operator's two commands, and runs
    // nothing (they need the key authority); the flag is kept.
    let Some(w) = World::new("retiring", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.queued_acceptance(&p);
    w.control("failafter-verify", "2");
    let rotation = w
        .signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    w.uncontrol("failafter-verify");
    assert!(
        matches!(
            &rotation.old_key,
            inklkunn_core::OldKeyOutcome::RetiredUnrecorded(_)
        ),
        "{rotation:?}"
    );
    let command = inklkunn_core::finish_retirement_command(&w.wiki.id);
    let abandon = inklkunn_core::abandon_retirement_command(&w.wiki.id);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    let history = w.history();
    let r = w.recover();
    assert_eq!(
        r.plan.stop,
        Some(Stop::RetirementUnfinished),
        "{:#?}",
        r.lines()
    );
    assert!(r.done.is_empty(), "recovery ran nothing");
    assert!(
        r.lines()
            .iter()
            .any(|l| l.contains(&command) && l.contains(&abandon)),
        "the stop names both commands: {:#?}",
        r.lines()
    );
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert_eq!(w.history(), history);
    // The same, shown without running anything.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, Some(Stop::RetirementUnfinished));
}

#[test]
fn an_acceptance_while_a_retirement_is_unfinished_fails_with_its_own_word_and_the_proposal_stays_open()
 {
    // W1 through the whole acceptance: the signer refuses before it changes anything, the acceptance record ends
    // *failed* with the word `retirement-unfinished` (a word of its own, not `refused`), the intent is abandoned, the
    // proposal stays held for review, the wiki is not flagged (nothing is wrong with the repository), and once the
    // operator has finished the retirement the same proposal is accepted.
    let Some(w) = World::new("retiring-accept", LeaseConfig::default()) else {
        return;
    };
    w.control("failafter-trust", "2");
    let rotation = w
        .signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("the rotation happened");
    w.uncontrol("failafter-trust");
    w.uncontrol("count-trust");
    assert!(
        matches!(
            &rotation.old_key,
            inklkunn_core::OldKeyOutcome::RetireFailed(_)
        ),
        "{rotation:?}"
    );
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let history = w.history();
    let err = w.accept(&p).unwrap_err();
    let AcceptFailure::Failed { word, detail, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        *word,
        inklkunn_core::acceptance::words::RETIREMENT_UNFINISHED
    );
    assert_eq!(*word, "retirement-unfinished");
    assert!(
        detail.contains(&inklkunn_core::finish_retirement_command(&w.wiki.id)),
        "{detail}"
    );
    assert_eq!(err.word(), "retirement-unfinished");
    assert_eq!(w.history(), history);
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(inklkunn_store::AcceptanceOutcome::Failed { word }) if word == "retirement-unfinished"
    ));
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.state(&p), ProposalState::HeldForReview);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    // The person who runs the wiki finishes it; the same proposal is then accepted.
    w.signer
        .finish_retirement(&KeyAuthority::for_tests(), &w.repo)
        .expect("finished");
    assert!(matches!(
        w.accept(&p),
        Ok(inklkunn_core::Accepted::Signed(_))
    ));
    assert_eq!(w.state(&p), ProposalState::Accepted);
}

/// The real signer for everything, with the two questions about a retirement answered as told: after a cleanup the
/// plain question can be made to say "no", and the question beside a cleanup can have its answer changed.
struct Tweaked {
    signer: Arc<Signer>,
    plain_says_no: bool,
    beside: fn(
        inklkunn_signer::recover::RetirementExcuse,
    ) -> inklkunn_signer::recover::RetirementExcuse,
}

impl Recover for Tweaked {
    fn verify_after_retirement(
        &self,
        wiki: &inklkunn_store::WikiId,
    ) -> Result<Option<inklkunn_signer::recover::RetirementExcuse>, inklkunn_core::RecoverError>
    {
        if self.plain_says_no {
            return Ok(None);
        }
        self.signer.verify_after_retirement(wiki)
    }

    fn verify_beside_cleanup(
        &self,
        wiki: &inklkunn_store::WikiId,
    ) -> Result<Option<inklkunn_signer::recover::RetirementExcuse>, inklkunn_core::RecoverError>
    {
        Ok(self.signer.verify_beside_cleanup(wiki)?.map(self.beside))
    }

    fn complete_publication(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &AcceptanceToken,
    ) -> Result<(), inklkunn_core::RecoverError> {
        self.signer
            .complete_publication(wiki, view, act, acceptance)
    }

    fn seal_queued(
        &self,
        wiki: &inklkunn_store::WikiId,
        view: &str,
        act: &inklkunn_core::RecoveryToken,
        acceptance: &AcceptanceToken,
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

impl World {
    fn recovery_with(&self, signer: Arc<dyn Recover>) -> Recovery {
        Recovery::new(RecoveryParts {
            acceptances: self.store.clone(),
            acts: self.store.clone(),
            proposals: self.store.clone(),
            storage: self.storage.clone(),
            witnesses: Arc::new(Witnesses::new(
                &self.standin,
                Sandbox::Unconfined,
                self.layout.clone(),
            )),
            signer,
            leases: self.leases.clone(),
            layout: self.layout.clone(),
            audit: self.store.clone(),
        })
    }
}

#[test]
fn a_killed_seal_on_a_wiki_with_a_retired_key_is_finished_when_every_finding_is_a_recorded_one() {
    // W7, measured on prikk 0.55.0: while a publication's cleanup is pending `verify` fails by two conditions (the ref
    // publication and publication trust). Beside a recorded retirement, recovery finishes the cleanup when every
    // publication-trust finding is in the recorded set and `doctor` shows nothing but the cleanup once those are taken
    // out; the step's `verify` afterwards is the plain rule, which holds.
    let Some(w) = World::new("retired-cleanup", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let blocks = w.history();
    // Shown, nothing is done: the plan has the step, with the retirement's note, and no stop.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert!(
        step_names(&shown).contains(&"complete-publication"),
        "{:#?}",
        shown.lines()
    );
    assert!(
        shown
            .lines()
            .iter()
            .any(|l| l.contains("only publication trust and the cleanup of a killed seal fail")),
        "{:#?}",
        shown.lines()
    );
    assert_eq!(w.history(), blocks);
    assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1);
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert!(step_names(&r).contains(&"complete-publication"));
    let verify = r
        .done
        .iter()
        .find(|d| d.step == "verify")
        .expect("verify ran");
    assert!(
        verify.detail.contains("only because of a retired key"),
        "{}",
        verify.detail
    );
    assert_eq!(w.history(), blocks, "the step makes no block");
    assert_eq!(w.queue_count(), 0);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(w.state(&p), ProposalState::Accepted);
}

#[test]
fn a_killed_seal_beside_findings_the_signer_does_not_name_all_of_is_still_a_stop_and_changes_nothing()
 {
    // W7: the stop `cleanup-beside-trust-findings` stays for anything else. Here the signer's answer beside the cleanup
    // names less than `doctor` does (a ref state is not in what it says is recorded): the finding stays in `doctor`,
    // and the plan stops with its own word. Nothing is changed.
    let Some(w) = World::new("retired-cleanup-less", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let blocks = w.history();
    let recovery = w.recovery_with(Arc::new(Tweaked {
        signer: w.signer.clone(),
        plain_says_no: false,
        beside: |mut excuse| {
            excuse.ref_states.clear();
            excuse
        },
    }));
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = recovery.recover(&w.wiki, mode).unwrap();
        assert_eq!(
            r.plan.stop,
            Some(Stop::CleanupBesideTrustFindings),
            "{:#?}",
            r.lines()
        );
        assert!(r.done.is_empty());
    }
    // And with the signer saying nothing at all beside a cleanup (the default of the trait) the stop is the same.
    let recovery = w.recovery_with(Arc::new(ForgetsTheBlocks(w.signer.clone())));
    let r = recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert_eq!(r.plan.stop, Some(Stop::CleanupBesideTrustFindings));
    assert_eq!(w.history(), blocks);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1);
}

#[test]
fn after_the_step_the_plain_rule_must_hold_or_recovery_stops_with_the_flag_kept() {
    // W7: the question beside a cleanup lets the step be taken; it does not excuse the wiki. If, afterwards, the plain
    // rule (publication trust alone, all of it recorded) does not say yes, the `verify` step fails and the flag stays.
    let Some(w) = World::new("retired-cleanup-after", LeaseConfig::default()) else {
        return;
    };
    w.signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("a rotation that retires the old key");
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &[]);
    let recovery = w.recovery_with(Arc::new(Tweaked {
        signer: w.signer.clone(),
        plain_says_no: true,
        beside: |excuse| excuse,
    }));
    let r = recovery.recover(&w.wiki, RecoveryMode::Run).unwrap();
    assert!(
        step_names(&r).contains(&"complete-publication"),
        "{:#?}",
        r.lines()
    );
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
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

#[test]
fn recovery_does_not_seal_a_queued_patch_while_a_retirement_is_unfinished_and_names_both_commands()
{
    // W1 at the plan: a patch waits to be sealed and a key retirement was begun and not finished (the key is still
    // trusted, so `doctor` is quiet). Sealing makes a block, which is paused; the plan stops with the retirement's word,
    // names both commands, and runs nothing. The signer would refuse the step with the same words.
    let Some(w) = World::new("retiring-queued", LeaseConfig::default()) else {
        return;
    };
    let q = w.propose(
        "Half done",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    w.queued_acceptance(&q);
    // trust calls: adopt the new key (0), read the list (1), remove the old key (2: refused)
    w.control("failafter-trust", "2");
    let rotation = w
        .signer
        .rotate_keys(&KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .expect("the rotation happened");
    w.uncontrol("failafter-trust");
    w.uncontrol("count-trust");
    assert!(
        matches!(
            &rotation.old_key,
            inklkunn_core::OldKeyOutcome::RetireFailed(_)
        ),
        "{rotation:?}"
    );
    let finish = inklkunn_core::finish_retirement_command(&w.wiki.id);
    let abandon = inklkunn_core::abandon_retirement_command(&w.wiki.id);
    let history = w.history();
    let r = w.recover();
    assert_eq!(
        r.plan.stop,
        Some(Stop::RetirementUnfinished),
        "{:#?}",
        r.lines()
    );
    assert!(r.done.is_empty(), "recovery ran nothing");
    assert!(
        r.lines()
            .iter()
            .any(|l| l.contains(&finish) && l.contains(&abandon)),
        "{:#?}",
        r.lines()
    );
    assert_eq!(w.queue_count(), 1, "nothing was sealed");
    assert_eq!(w.history(), history);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

/// A trail that takes every event but the ones of one action: the store's write that fails after steps were done.
struct TrailThatRefuses {
    inner: Arc<SqliteStore>,
    action: &'static str,
}

impl AuditSink for TrailThatRefuses {
    fn record(&self, event: &AuditEvent) -> inklkunn_store::Result<()> {
        if event.action == self.action {
            return Err(inklkunn_store::StoreError::Backend(
                "the trail is full".to_owned(),
            ));
        }
        self.inner.record(event)
    }
}

/// A trail that takes every event but the record of a step that failed: the write that fails after a step did.
struct TrailThatRefusesFailures {
    inner: Arc<SqliteStore>,
}

impl AuditSink for TrailThatRefusesFailures {
    fn record(&self, event: &AuditEvent) -> inklkunn_store::Result<()> {
        if event.action == "recovery-step" && event.outcome.as_str() == "failed" {
            return Err(inklkunn_store::StoreError::Backend(
                "the trail is full".to_owned(),
            ));
        }
        self.inner.record(event)
    }
}

/// The same recovery as the world's, writing its trail to `audit`.
fn recovery_writing_to(w: &World, audit: Arc<dyn AuditSink>) -> Recovery {
    Recovery::new(RecoveryParts {
        acceptances: w.store.clone(),
        acts: w.store.clone(),
        proposals: w.store.clone(),
        storage: w.storage.clone(),
        witnesses: Arc::new(w.recovery_witnesses()),
        signer: w.signer.clone() as Arc<dyn Recover>,
        leases: w.leases.clone(),
        layout: w.layout.clone(),
        audit,
    })
}

#[test]
fn an_error_after_the_take_back_carries_the_steps_that_were_done() {
    // Maintenance M18 (R-b), the order that matters: `discard-queued` finished, then a write failed. The patch is gone
    // for good, and the error says so, so that no surface denies it. Needs `uncommit` (0.57.0).
    let Some(w) = World::new_057("error-after-take-back", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    let refusing = recovery_writing_to(
        &w,
        Arc::new(TrailThatRefuses {
            inner: w.store.clone(),
            action: "recovery",
        }),
    );
    let err = refusing
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(id),
            },
        )
        .unwrap_err();
    assert_eq!(w.queue_count(), 0, "the patch was taken back");
    assert!(
        err.steps_done().contains(&"discard-queued"),
        "the error carries the steps that finished: {err:?}"
    );
    assert!(matches!(&err, RecoveryError::AfterSteps { cause, .. }
        if matches!(**cause, RecoveryError::Audit(_))));
    assert!(
        err.to_string()
            .contains("steps already done: discard-queued"),
        "{err}"
    );

    // The other order: the trail refuses before any step (the first record of the run), so no step is carried.
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted again.\n",
    );
    let id = w.queued_patch_id();
    let early = recovery_writing_to(
        &w,
        Arc::new(TrailThatRefuses {
            inner: w.store.clone(),
            action: "recovery-begin",
        }),
    );
    let err = early
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(id),
            },
        )
        .unwrap_err();
    assert!(err.steps_done().is_empty(), "{err:?}");
    assert!(matches!(err, RecoveryError::Audit(_)), "{err:?}");
    assert_eq!(w.queue_count(), 1, "nothing was taken back this time");
}

#[test]
fn a_lock_that_could_not_be_cleared_is_a_stop_at_that_step_with_nothing_done() {
    // Maintenance M18 (R-c). A clear prikk refuses (the stand-in's `fail-unlock-clear`; before M50 it was 0.57.0's
    // refusal of `--yes` for a holder that appears running); the run stops at `clear-locks`, no preview is shown, and the plan still carries its intent (the sentence the end event no longer
    // takes from it: `inklkunn`'s `discard_word_fate` reads `outcome` and `done`). Needs 0.57.0 for the preview.
    let Some(w) = World::new_057("lock-not-cleared", LeaseConfig::default()) else {
        return;
    };
    w.plant_queued_patch(
        "pages/Home.md",
        "---\ntitle: Home\nid: p-home\n---\n\nPlanted.\n",
    );
    let lock = plant_lock(
        &w.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    w.flag(RecoveryReason::Interrupted);
    let id = w.queued_patch_id();
    w.control("fail-unlock-clear", "");
    let r = w
        .recovery
        .recover_with(
            &w.wiki,
            RecoveryMode::Run,
            &RecoverOptions {
                discard_queued_patch: Some(id),
            },
        )
        .unwrap();
    w.uncontrol("fail-unlock-clear");
    assert!(
        matches!(
            &r.outcome,
            RecoveryOutcome::Stopped {
                at: Some("clear-locks"),
                ..
            }
        ),
        "{:#?}",
        r.lines()
    );
    assert!(r.done.is_empty(), "no step finished");
    assert!(Path::new(&lock).exists(), "the lock was not cleared");
    assert_eq!(w.uncommit_calls(), 0, "no preview was read");
    assert_eq!(w.queue_count(), 1);
    assert!(
        r.plan
            .discard_unused
            .as_deref()
            .is_some_and(|why| why.contains("a run clears it")),
        "the plan's field is its intent: {:?}",
        r.plan.discard_unused
    );
    // The reason is a whole sentence of what happened: the clear itself failed (M22).
    assert!(
        matches!(&r.outcome, RecoveryOutcome::Stopped { why, .. }
            if why.starts_with("the lock was not cleared: ")
                && why.contains("injected failure of unlock")),
        "{:?}",
        r.outcome
    );
}

#[test]
fn a_format_marker_prikk_will_not_read_fails_the_check_flags_the_wiki_and_stops_recovery_until_it_is_put_right()
 {
    // M17: the marker names a number no table knows (`9`), whatever prikk is running. Every prikk call fails with
    // `unsupported format version: 9`. The check records a failed verdict (the last one, Verified, cannot stand) and
    // flags the wiki; recovery stops with its own word and changes nothing; once the marker reads again, recovery
    // finishes and clears the flag.
    let Some(w) = World::new("format-marker", LeaseConfig::default()) else {
        return;
    };
    let verifier = Verifier::new(w.storage.clone(), w.store.clone(), w.layout.clone());
    verifier.flag_format_markers_in(w.store.clone());
    let sound = verifier.check_unthrottled(&w.wiki);
    assert!(
        matches!(sound.outcome, CheckOutcome::Verified),
        "{:?}",
        sound.outcome
    );
    let blocks = w.history().len();
    w.control("format-marker", "9");
    let checked = verifier.check_unthrottled(&w.wiki);
    assert!(
        matches!(checked.outcome, CheckOutcome::Failed { .. }),
        "{:?}",
        checked.outcome
    );
    let operator = checked.operator.expect("the operator's text");
    assert!(
        operator.contains("format 9") && operator.contains(".prikk/FORMAT"),
        "{operator}"
    );
    assert!(
        !operator.contains("nothing in the repository is damaged"),
        "{operator}"
    );
    let record = w.store.get(&w.wiki.id).unwrap();
    assert_eq!(record.verification.state, VerificationState::Failed);
    let flag = w.store.recovery_flag(&w.wiki.id).unwrap().expect("flagged");
    assert_eq!(flag.reason, RecoveryReason::Interrupted);
    // Recovery shows the stop, and a run ends in it without changing anything.
    let shown = w
        .recovery
        .recover(&w.wiki, RecoveryMode::PlanOnly)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        shown.plan.stop,
        Some(Stop::FormatMarker),
        "{:#?}",
        shown.lines()
    );
    assert!(shown.plan.steps.is_empty());
    let run = w.recover();
    assert!(
        matches!(&run.outcome, RecoveryOutcome::Stopped { at: None, .. }),
        "{:?}",
        run.outcome
    );
    assert!(run.done.is_empty(), "recovery ran nothing");
    assert!(
        run.lines()
            .iter()
            .any(|l| l.contains("STOP (format-marker)")),
        "{:#?}",
        run.lines()
    );
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    // The marker reads again: recovery finishes, the flag goes, the history is as it was, and a check verifies.
    w.uncontrol("format-marker");
    assert_eq!(w.history().len(), blocks);
    let done = w.recover();
    assert_eq!(
        done.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        done.lines()
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    let again = verifier.check_unthrottled(&w.wiki);
    assert!(
        matches!(again.outcome, CheckOutcome::Verified),
        "{:?}",
        again.outcome
    );
}

/// M20 (finding T-a): the markers that prikk answers with its own integrity error (`8\r\n`, ` 8\n`, `x\n`, an empty
/// file), on a repository `w` made and read by one real prikk. Each ends the check flagged and failed, and recovery at
/// the `format-marker` stop with a plan that says what the flag is; the sound marker put back, recovery finishes.
fn a_marker_that_is_not_a_number_flags_the_wiki_and_stops_recovery(w: &World) {
    let marker = w.repo.join(".prikk/FORMAT");
    let sound = std::fs::read(&marker).unwrap();
    let verifier = Verifier::new(w.storage.clone(), w.store.clone(), w.layout.clone());
    verifier.flag_format_markers_in(w.store.clone());
    let first = verifier.check_unthrottled(&w.wiki);
    assert!(
        matches!(first.outcome, CheckOutcome::Verified),
        "{:?}",
        first.outcome
    );
    for (what, bytes) in [
        ("8 with a carriage return", b"8\r\n".as_slice()),
        ("8 with a leading space", b" 8\n".as_slice()),
        ("a word", b"x\n".as_slice()),
        ("an empty file", b"".as_slice()),
    ] {
        std::fs::write(&marker, bytes).unwrap();
        let checked = verifier.check_unthrottled(&w.wiki);
        assert!(
            matches!(checked.outcome, CheckOutcome::Failed { .. }),
            "{what}: {:?}",
            checked.outcome
        );
        let operator = checked.operator.expect("the operator's text");
        assert!(
            operator.contains(".prikk/FORMAT") && operator.contains("not a format number"),
            "{what}: {operator}"
        );
        let record = w.store.get(&w.wiki.id).unwrap();
        assert_eq!(
            record.verification.state,
            VerificationState::Failed,
            "{what}"
        );
        let flag = w
            .store
            .recovery_flag(&w.wiki.id)
            .unwrap()
            .unwrap_or_else(|| panic!("{what}: not flagged"));
        assert_eq!(flag.reason, RecoveryReason::Interrupted, "{what}");
        let shown = w
            .recovery
            .recover(&w.wiki, RecoveryMode::PlanOnly)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(
            shown.plan.stop,
            Some(Stop::FormatMarker),
            "{what}: {:#?}",
            shown.lines()
        );
        let lines = shown.lines();
        assert!(
            lines.iter().any(|l| l.contains("is not a format number")),
            "{what}: {lines:#?}"
        );
        // The flag's reason is said for what it is here, and no act is blamed.
        assert!(
            lines.iter().any(|l| l
                .contains("interrupted: the reason the store gives a flag whose act is not known")
                && l.contains("damaged format marker")),
            "{what}: {lines:#?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("were begun and never ended")),
            "{what}: {lines:#?}"
        );
        let run = w.recover();
        assert!(
            matches!(&run.outcome, RecoveryOutcome::Stopped { at: None, .. }),
            "{what}: {:?}",
            run.outcome
        );
        assert!(run.done.is_empty(), "{what}: recovery ran nothing");
        assert!(
            run.lines()
                .iter()
                .any(|l| l.contains("STOP (format-marker)")),
            "{what}: {:#?}",
            run.lines()
        );
        assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
    }
    std::fs::write(&marker, &sound).unwrap();
    let done = w.recover();
    assert_eq!(
        done.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        done.lines()
    );
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    let again = verifier.check_unthrottled(&w.wiki);
    assert!(
        matches!(again.outcome, CheckOutcome::Verified),
        "{:?}",
        again.outcome
    );
}

#[test]
fn a_marker_that_is_not_a_number_flags_the_wiki_and_stops_recovery_on_the_first_prikk_on_path() {
    let Some(w) = World::new("marker-text-055", LeaseConfig::default()) else {
        return;
    };
    a_marker_that_is_not_a_number_flags_the_wiki_and_stops_recovery(&w);
}

#[test]
fn a_marker_that_is_not_a_number_flags_the_wiki_and_stops_recovery_on_prikk_0_57_0() {
    let Some(w) = World::new_057("marker-text-057", LeaseConfig::default()) else {
        return;
    };
    a_marker_that_is_not_a_number_flags_the_wiki_and_stops_recovery(&w);
}

// ---- a `seal` killed after it moved the ref's pointer and before it appended the ref log (maintenance M61) ----

/// How many times prikk was asked for `verb`, as the stand-in logged it.
fn calls_of(w: &World, verb: &str) -> usize {
    std::fs::read_to_string(w.ctl.join("calls.log"))
        .unwrap_or_default()
        .lines()
        .filter(|v| *v == verb)
        .count()
}

/// What `prikk worktree-status --format json` says of a clean worktree (made up, so that the plan is reached on every
/// prikk in states where the real one refuses).
const CLEAN_WORKTREE: &str = r#"{"schema_version":"worktree-status-report-v1","repository":"/x/.prikk","ref":"heads/main","current_branch":"heads/main","tracked_files":0,"unchanged_files":0,"clean":true,"refused_count":0,"refused_declaration_count":0,"queued_elsewhere":null,"behind_ref":null,"changes":[],"declarations":[]}"#;

/// `doctor`'s words for the interrupted publication of `view`, as prikk 0.58.0 prints the error beside a stale lock (the
/// message names the lock), and a summary of `errors` errors.
fn lead_report(view: &str, extra: &str, errors: u32) -> String {
    format!(
        "doctor repository: /x\n{extra}error [PRIKK-VERIFY-REF-POINTER-LEADS-LOG]: authoritative pointer leads committed ref log by one transition (the lock it took is still on disk: its holder does not appear to be running: no process has the recorded pid; not proof it is safe to clear); run `prikk ref complete {view}`\n  recommendation: run `prikk ref complete {view}`; doctor does not sign or append\nissue summary: errors={errors}, warnings=0, info=0\n"
    )
}

/// The locks the real kill left (measured on prikk 0.58.0): the active session's, the ref's, and the ref pointer
/// index's and ref log's containers.
fn plant_the_killed_seals_locks(w: &World) {
    plant_the_killed_seals_locks_of(
        w,
        "c316ccb36a95a977918874d43e722a5a7d9ef74b138f3b76078f6993c14a799f",
    );
}

/// The same for the ref whose name has the SHA-256 `hex` (the ref's own lock is named after it: `heads/main` is
/// `c316ccb3...`, `heads/other` is `34cae7c3...`).
fn plant_the_killed_seals_locks_of(w: &World, hex: &str) {
    for (relative, kind) in [
        ("active/default/active.lock", "active"),
        (&format!("refs/locks/{hex}.lock"), "ref"),
        (
            "refs/containers/pointer-index.lock",
            "container:ref-pointer-index",
        ),
        ("refs/containers/log.lock", "container:ref-log"),
    ] {
        plant_lock(&w.repo, relative, 2_147_483_000, kind);
    }
}

#[test]
fn a_seal_killed_before_its_ref_log_is_finished_with_ref_complete_then_the_cleanup_and_the_act_ends()
 {
    // Maintenance M61. The `seal` moved the ref's pointer to its block and was killed before it appended the ref log: the
    // block is in the history, the queue still holds its patch, `doctor` names exactly one error, the interrupted
    // publication of the view, and `ref complete` is the way out that only the signer (the wiki's key) can take. The
    // plan is `ref complete`, then P15's cleanup, then the records; with the locks the killed process left (cleared
    // first) and without.
    for (tag, locks) in [("seal-lead", false), ("seal-lead-locks", true)] {
        let Some(w) = World::new(tag, LeaseConfig::default()) else {
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
        w.killed_seal(&p, &["leads-the-log"]);
        if locks {
            plant_the_killed_seals_locks(&w);
        }
        assert_eq!(
            w.history().len(),
            before.len() + 1,
            "the block is published"
        );
        let published = w.tip();
        prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &published);
        let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
        assert_eq!(doctor.only_ref_lead(), Some(MAIN), "{doctor:?}");
        assert!(!w.verifies(), "the pointer leads the log");
        assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1);
        let (seals, refs, commits) = (
            calls_of(&w, "seal"),
            calls_of(&w, "ref"),
            calls_of(&w, "commit"),
        );

        // The plan, shown: it changes nothing, and it names the steps and what they rest on.
        let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
        assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
        let mut expected = vec![
            "complete-ref-publication",
            "complete-publication",
            "restore-worktree",
            "end-sealed",
        ];
        if locks {
            expected.insert(0, "clear-locks");
        }
        expected.extend(["verify", "reconcile", "clear-flag"]);
        assert_eq!(step_names(&shown), expected);
        let lines = shown.lines();
        for sentence in [
            "run ref complete heads/main once, as prikk's doctor recommends, to finish the publication of block",
            "run seal --allow-no-audit once, as prikk's doctor recommends",
            "saw: prikk doctor names the error PRIKK-VERIFY-REF-POINTER-LEADS-LOG for heads/main",
        ] {
            assert!(
                lines.iter().any(|l| l.contains(sentence)),
                "{sentence}: {lines:#?}"
            );
        }
        assert_eq!(
            (calls_of(&w, "seal"), calls_of(&w, "ref")),
            (seals, refs),
            "a plan runs nothing"
        );
        assert!(!w.verifies(), "a plan changes nothing");

        // The run.
        let r = w.recover();
        assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
        let done: Vec<&str> = r.done.iter().map(|d| d.step).collect();
        assert_eq!(done, expected);
        let said = r
            .done
            .iter()
            .find(|d| d.step == "complete-ref-publication")
            .map(|d| d.detail.clone())
            .unwrap();
        assert!(
            said.contains(&published) && said.contains("the ref log's missing record is appended"),
            "{said}"
        );
        // `ref complete` runs once (after the plan the signer reads), the cleanup's `seal` once, and nothing commits.
        assert_eq!(
            calls_of(&w, "ref"),
            refs + 2,
            "prikk's plan and its completion"
        );
        assert_eq!(
            calls_of(&w, "seal"),
            seals + 1,
            "prikk's recommendation, once"
        );
        assert_eq!(calls_of(&w, "commit"), commits, "recovery never commits");
        if locks {
            // Each lock was cleared on the lease's word, and recovery went on.
            assert_eq!(w.unlock_calls().len(), 4, "{:?}", w.unlock_calls());
        }
        // No second block: the tip is the one that was published; the queue is drained; the repository verifies.
        assert_eq!(w.history().len(), before.len() + 1);
        assert_eq!(w.tip(), published);
        assert_eq!(w.queue_count(), 0);
        assert!(w.verifies());
        let after = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
        assert!(
            after
                .diagnostics
                .iter()
                .all(|d| !d.is_cleanup_pending() && !d.is_publication_lead()),
            "{after:?}"
        );
        assert!(w.page("pages/Home.md").contains("Uno."));
        // The records agree with the history: the acceptance sealed in that block, the intent completed, the proposal
        // accepted, the flag gone.
        let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
        assert!(matches!(
            entry.result.map(|r| r.outcome),
            Some(AcceptanceOutcome::Sealed { block_id, .. }) if block_id == published
        ));
        assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
        assert_eq!(w.state(&p), ProposalState::Accepted);
        assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
        assert!(w.reconciled().is_clean());
        // Both steps are in the recorded plan, and in the audit trail in order, before the records were written.
        let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
        let plan = &acts.first().unwrap().act.plan;
        assert!(
            plan.contains("\"step\":\"complete-ref-publication\"")
                && plan.contains("\"step\":\"complete-publication\""),
            "{plan}"
        );
        let audit = w.audit();
        let at = |step: &str, from: usize| {
            audit
                .iter()
                .skip(from)
                .position(|(a, _, d)| a == "recovery-step" && d == step)
                .map(|i| i + from)
                .unwrap_or_else(|| panic!("{step} is audited: {audit:?}"))
        };
        let first = at("complete-ref-publication", 0);
        let second = at("complete-publication", first + 1);
        at("end-sealed", second + 1);
        // The wiki takes writes again.
        assert!(matches!(
            w.accept(&q).expect("writes are accepted again"),
            inklkunn_core::Accepted::Signed(_)
        ));
        assert_eq!(w.history().len(), before.len() + 2);
        assert!(w.reconciled().is_clean());
    }
}

#[test]
fn a_seal_lead_that_status_reads_is_decided_as_where_it_refuses() {
    // Maintenance M51 and M61: from prikk 0.58.0 `status --format json` reads the state and says the queue is already
    // sealed in the block the pointer leads to. The answer is made up through the stand-in so that every prikk meets it:
    // the plan and the run are the same as where `status` refuses with the integrity error.
    let Some(w) = World::new("seal-lead-reads", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &["leads-the-log"]);
    let blocks = w.history();
    let tip = blocks.first().unwrap();
    let patches: Vec<String> = tip
        .patch_messages
        .iter()
        .map(|m| m.patch_id.clone())
        .collect();
    w.control(
        "say-status-json",
        &canned_status(Some(&tip.block_id), &patches),
    );
    w.control("say-until-seal", "");
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(
        step_names(&shown),
        [
            "complete-ref-publication",
            "complete-publication",
            "restore-worktree",
            "end-sealed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    assert!(
        shown.lines().iter().any(|l| l.contains(&format!(
            "saw: prikk status: 1 patch(es) queued, all already sealed in block {}",
            tip.block_id
        ))),
        "{:#?}",
        shown.lines()
    );
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    assert_eq!(w.history(), blocks, "no second block");
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_none());
    w.uncontrol("say-status-json");
    w.uncontrol("say-until-seal");
    assert!(w.verifies());
}

#[test]
fn a_seal_lead_is_not_finished_unless_it_is_the_one_error_for_the_acts_block_and_status_agrees() {
    // Maintenance M61: anything else than the state is a stop (or a witness that cannot be read), in a plan and in a run:
    // nothing is written, `ref complete` is never asked for, and the wiki stays flagged. The witnesses are made up
    // through the stand-in, so that the same rows are met on every prikk.
    let Some(w) = World::new("seal-lead-stops", LeaseConfig::default()) else {
        return;
    };
    let p = w.propose(
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    w.killed_seal(&p, &["leads-the-log"]);
    let blocks = w.history();
    let tip = blocks.first().unwrap();
    let patches: Vec<String> = tip
        .patch_messages
        .iter()
        .map(|m| m.patch_id.clone())
        .collect();
    let (seals, refs) = (calls_of(&w, "seal"), calls_of(&w, "ref"));
    let untouched = |what: &str| {
        assert_eq!(w.history(), blocks, "{what}");
        assert_eq!(
            (calls_of(&w, "seal"), calls_of(&w, "ref")),
            (seals, refs),
            "{what}: neither `seal` nor `ref` was asked for"
        );
        assert!(
            w.store.recovery_flag(&w.wiki.id).unwrap().is_some(),
            "{what}: the wiki stays flagged"
        );
        assert_eq!(w.store.open_intents(&w.wiki.id).unwrap().len(), 1, "{what}");
    };
    // The plan stops with a word, in a plan and in a run.
    let stops = |what: &str, stop: Stop| {
        for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
            let r = w
                .recovery
                .recover(&w.wiki, mode)
                .unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_eq!(r.plan.stop, Some(stop), "{what}: {:#?}", r.lines());
            assert!(r.done.is_empty(), "{what}");
            if mode == RecoveryMode::Run {
                assert!(
                    matches!(r.outcome, RecoveryOutcome::Stopped { at: None, .. }),
                    "{what}"
                );
            }
        }
        untouched(what);
    };
    // The witness cannot be read, or two answers of prikk's disagree: a failure to read, in a plan and in a run.
    let unreadable = |what: &str, said: &str| {
        for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
            let r = w.recovery.recover(&w.wiki, mode);
            assert!(
                matches!(&r, Err(RecoveryError::Witness(why)) if why.contains(said)),
                "{what}: {r:?}"
            );
        }
        untouched(what);
    };
    // Every row is made up from the state: `status` reads (0.58.0's form) and the worktree is whole, whichever prikk.
    w.control("say-worktree-status", CLEAN_WORKTREE);
    let reads = canned_status(Some(&tip.block_id), &patches);
    w.control("say-status-json", &reads);

    // A lead for another ref than the act's view.
    w.control("say-doctor", &lead_report("heads/other", "", 1));
    stops("a lead for another ref", Stop::DoctorTrouble);
    // A second error beside the lead, a second lead, and the cleanup's error beside it.
    w.control(
        "say-doctor",
        &lead_report(
            MAIN,
            "error [PRIKK-DOCTOR-OBJECT-MISSING]: an object is missing\n",
            2,
        ),
    );
    stops("a second error", Stop::DoctorTrouble);
    let other = "error [PRIKK-VERIFY-REF-POINTER-LEADS-LOG]: authoritative pointer leads committed ref log by one transition; run `prikk ref complete heads/other`\n  recommendation: run `prikk ref complete heads/other`; doctor does not sign or append\n";
    w.control("say-doctor", &lead_report(MAIN, other, 2));
    stops("a second lead", Stop::DoctorTrouble);
    w.control(
        "say-doctor",
        &lead_report(
            MAIN,
            "error [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree but matching active publication state remains\n  recommendation: run signer-backed `prikk seal --allow-no-audit` for the affected ref\n",
            2,
        ),
    );
    stops("the cleanup's error beside the lead", Stop::DoctorTrouble);
    // The lead in words that are not prikk's own.
    w.control(
        "say-doctor",
        "doctor repository: /x\nerror [PRIKK-VERIFY-REF-POINTER-LEADS-LOG]: authoritative pointer leads committed ref log by one transition\n  recommendation: run `prikk ref complete heads/main`\nissue summary: errors=1, warnings=0, info=0\n",
    );
    stops("another wording", Stop::DoctorTrouble);
    // The lead, and `status` reads an ordinary queue: the queue is not sealed in the block the pointer leads to.
    w.control("say-doctor", &lead_report(MAIN, "", 1));
    w.control("say-status-json", &canned_status(None, &patches));
    stops("an ordinary queue beside the lead", Stop::DoctorTrouble);
    // The lead, and `status` names another block than the history's tip, or a queue that is not the block's patches:
    // two answers of prikk's that disagree, which is not read.
    w.control(
        "say-status-json",
        &canned_status(Some(&"e".repeat(64)), &patches),
    );
    unreadable("status names another block", "already sealed in block");
    w.control(
        "say-status-json",
        &canned_status(Some(&tip.block_id), &["f".repeat(64)]),
    );
    unreadable(
        "status lists another patch",
        "not the patches of the newest block",
    );
    // `status` says the queue is already sealed while `doctor` names no error at all.
    w.control("say-status-json", &reads);
    w.control(
        "say-doctor",
        "doctor repository: /x\ninfo [PRIKK-DOCTOR-VERIFY-OK]: done\nissue summary: errors=0, warnings=0, info=1\n",
    );
    unreadable("doctor names no error", "names no error at all");

    // The block the pointer leads to is not the act's: the history shows the message of another change.
    w.control("say-doctor", &lead_report(MAIN, "", 1));
    w.control("log-other-message", "");
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = w.recovery.recover(&w.wiki, mode).unwrap();
        assert_eq!(
            r.plan.stop,
            Some(Stop::PublishedBlockIsNotTheAct),
            "{:#?}",
            r.lines()
        );
        assert!(r.done.is_empty());
    }
    w.uncontrol("log-other-message");
    untouched("a block that is not the act's");
    // ... or there is no unfinished act at all (its records are gone, say): somebody else's publication.
    let intent = w.store.open_intents(&w.wiki.id).unwrap().remove(0);
    w.store
        .complete_intent(&inklkunn_store::IntentCompletion {
            intent: intent.intent.id.clone(),
            end: IntentEnd::Abandoned,
            at: 9,
        })
        .unwrap();
    w.store
        .finish_acceptance(&inklkunn_store::AcceptanceResult {
            acceptance: intent.intent.acceptance.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: "interrupted".to_owned(),
            },
            at: 9,
        })
        .unwrap();
    for mode in [RecoveryMode::PlanOnly, RecoveryMode::Run] {
        let r = w.recovery.recover(&w.wiki, mode).unwrap();
        assert_eq!(r.plan.stop, Some(Stop::DoctorTrouble), "{:#?}", r.lines());
        assert!(r.done.is_empty());
    }
    assert_eq!(w.history(), blocks);
    assert_eq!((calls_of(&w, "seal"), calls_of(&w, "ref")), (seals, refs));
    assert!(w.store.recovery_flag(&w.wiki.id).unwrap().is_some());
}

// ---- recovery on a view other than the default view, end to end (maintenance M62) ----

/// The wiki's second view in these tests.
const ALT: &str = "heads/other";

/// What prikk says of the worktree: the branch it is on (the view it is a copy of) and whether it is the baseline.
#[derive(Debug, PartialEq, Eq)]
struct WorktreeFacts {
    current_branch: String,
    clean: bool,
}

impl World {
    /// The wiki's second view, `other`, made the way a person makes one (a reviewer's `create-view` through the
    /// acceptance service: the recorded act, the signer's second operation), forked from the default view's tip.
    fn make_other_view(&self) {
        self.acceptor
            .create_view(
                &self.maintainer,
                &self.wiki,
                "other",
                None,
                inklkunn_core::views::ViewKind::Experimental,
            )
            .unwrap_or_else(|e| panic!("{e}"));
    }

    fn history_in(&self, view: &str) -> Vec<inklkunn_prikk::HistoryBlock> {
        self.storage
            .history(&self.repo, &Point::View(view.to_owned()), None)
            .unwrap()
            .blocks
    }

    fn tip_in(&self, view: &str) -> String {
        self.history_in(view).first().unwrap().block_id.clone()
    }

    fn page_in(&self, view: &str, path: &str) -> String {
        String::from_utf8(
            self.storage
                .read(&self.repo, &Point::Block(self.tip_in(view)), path, 1 << 20)
                .unwrap(),
        )
        .unwrap()
    }

    /// The page as the worktree holds it.
    fn worktree_page(&self, path: &str) -> String {
        std::fs::read_to_string(self.repo.join(path)).unwrap()
    }

    /// The branch prikk says is current (`status`) and whether `worktree-status` finds the worktree the baseline; an
    /// error where prikk refuses either (a half-published ref).
    fn worktree_facts(&self) -> Result<WorktreeFacts, String> {
        let status = self.storage.status(&self.repo).map_err(|e| e.to_string())?;
        let worktree = self
            .recovery_witnesses()
            .worktree(&self.wiki.id)
            .map_err(|e| e.to_string())?;
        Ok(WorktreeFacts {
            current_branch: status.current_branch.unwrap_or_default(),
            clean: worktree.clean,
        })
    }
}

/// The ref lock's digest for `heads/other` (the SHA-256 of the ref's name).
const ALT_REF_DIGEST: &str = "34cae7c3059a6b81ebb4137504bd18c1f487d3e71d626a715d241fb2203a4763";

/// The pages as the worktree holds them, by file name (the help page is left out: it carries the wiki's own name and id).
fn pages_of_the_worktree(w: &World) -> Vec<(String, String)> {
    let mut pages: Vec<(String, String)> = std::fs::read_dir(w.repo.join("pages"))
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name() != "Help.md").then(|| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read_to_string(entry.path()).unwrap(),
                )
            })
        })
        .collect();
    pages.sort();
    pages
}

/// The worktree of the wiki as an acceptance into `heads/other` that nothing interrupted leaves it, for the tests below
/// to compare with: its facts and its pages.
fn the_worktree_after_an_uninterrupted_acceptance_into_other(
    tag: &str,
    edited: &str,
) -> Option<(WorktreeFacts, Vec<(String, String)>)> {
    let w = World::new(tag, LeaseConfig::default())?;
    w.make_other_view();
    let p = w.propose_in(
        Some(ALT),
        "Say it in Spanish",
        &[("pages/Home.md", Some(edited))],
    );
    assert!(matches!(
        w.accept(&p).expect("accepted"),
        inklkunn_core::Accepted::Signed(_)
    ));
    Some((w.worktree_facts().unwrap(), pages_of_the_worktree(&w)))
}

/// A `seal` killed on the wiki's second view, `heads/other`, recovered end to end through `wiki recover` on a real prikk
/// (maintenance M62). `lead`: the kill came before the ref log was appended (M61), else after the block was published
/// and before the queue was drained (P15); `locks`: the locks the killed process left are there too. Returns `false`
/// when prikk is not on `PATH` (the skip line was printed).
fn a_killed_seal_on_the_other_view_is_recovered_end_to_end(
    tag: &str,
    lead: bool,
    locks: bool,
) -> bool {
    let Some(w) = World::new(tag, LeaseConfig::default()) else {
        return false;
    };
    let edited = HOME.replace("One.", "Uno.");
    let Some((uninterrupted, uninterrupted_pages)) =
        the_worktree_after_an_uninterrupted_acceptance_into_other(&format!("{tag}-c"), &edited)
    else {
        return false;
    };
    w.make_other_view();
    let p = w.propose_in(
        Some(ALT),
        "Say it in Spanish",
        &[("pages/Home.md", Some(&edited))],
    );
    let q = w.propose_in(
        Some(ALT),
        "Another",
        &[(
            "pages/Another.md",
            Some("---\ntitle: Another\nid: pn\n---\n\na\n"),
        )],
    );
    let r = w.propose(
        "A third, into the default view",
        &[(
            "pages/Third.md",
            Some("---\ntitle: Third\nid: pt\n---\n\nt\n"),
        )],
    );
    let (main_before, other_before) = (w.history(), w.history_in(ALT));
    // The worktree belongs to the default view, as the wiki was made: this is what it was before the acceptance.
    let at_the_start = WorktreeFacts {
        current_branch: MAIN.to_owned(),
        clean: true,
    };
    assert_eq!(w.worktree_facts().unwrap(), at_the_start);
    let pages_at_the_start = pages_of_the_worktree(&w);

    // The acceptance is into `heads/other` (the proposal names it; the recorded intent carries it), and its `seal` is
    // killed.
    let extras: &[&str] = match (lead, locks) {
        (true, _) => &["leads-the-log"],
        (false, true) => &["leaves-the-lock"],
        (false, false) => &[],
    };
    w.killed_seal(&p, extras);
    if lead && locks {
        plant_the_killed_seals_locks_of(&w, ALT_REF_DIGEST);
    }
    let intent = w.store.open_intents(&w.wiki.id).unwrap();
    assert_eq!(intent.len(), 1);
    assert_eq!(
        nth(&intent, 0).intent.view,
        ALT,
        "the intent carries the view"
    );
    let published = w.tip_in(ALT);
    assert_eq!(w.history_in(ALT).len(), other_before.len() + 1);
    assert_eq!(w.history(), main_before, "the default view did not move");
    prikk_probes::assert_status_shows_a_pending_cleanup(&w.storage, &w.repo, &published);
    let doctor = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    if lead {
        assert_eq!(doctor.only_ref_lead(), Some(ALT), "{doctor:?}");
        assert!(!w.verifies(), "the pointer leads the log");
    } else {
        assert!(doctor.is_only_cleanup_pending(), "{doctor:?}");
    }
    let (seals, refs, commits) = (
        calls_of(&w, "seal"),
        calls_of(&w, "ref"),
        calls_of(&w, "commit"),
    );

    // The plan, shown: it is the lead's steps for that view, and it changes nothing.
    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    let mut expected: Vec<&str> = Vec::new();
    if locks {
        expected.push("clear-locks");
    }
    if lead {
        expected.push("complete-ref-publication");
    }
    expected.extend(["complete-publication", "restore-worktree", "end-sealed"]);
    expected.extend(["verify", "reconcile", "clear-flag"]);
    assert_eq!(step_names(&shown), expected);
    let lines = shown.lines();
    let mut sentences = vec![
        "run seal --allow-no-audit once, as prikk's doctor recommends",
        "put the worktree of heads/other back to what its tip says",
    ];
    if lead {
        sentences.extend([
            "run ref complete heads/other once, as prikk's doctor recommends, to finish the publication of block",
            "saw: prikk doctor names the error PRIKK-VERIFY-REF-POINTER-LEADS-LOG for heads/other",
        ]);
    }
    for sentence in sentences {
        assert!(
            lines.iter().any(|l| l.contains(sentence)),
            "{sentence}: {lines:#?}"
        );
    }
    assert!(
        lines.iter().all(|l| !l.contains("ref complete heads/main")),
        "{lines:#?}"
    );
    assert_eq!(
        (calls_of(&w, "seal"), calls_of(&w, "ref")),
        (seals, refs),
        "a plan runs nothing"
    );
    assert_eq!(w.history_in(ALT).len(), other_before.len() + 1);

    // The run.
    let done = w.recover();
    assert_eq!(
        done.outcome,
        RecoveryOutcome::Recovered,
        "{:#?}",
        done.lines()
    );
    let steps: Vec<&str> = done.done.iter().map(|d| d.step).collect();
    assert_eq!(steps, expected);
    // The killed seal left the worktree as its change made it, which is the new tip of the view it belongs to (the
    // acceptance switched it there): the step finds nothing to put back.
    let restored = done
        .done
        .iter()
        .find(|d| d.step == "restore-worktree")
        .unwrap();
    assert_eq!(restored.detail, "0 path(s) put back");
    if lead {
        let said = done
            .done
            .iter()
            .find(|d| d.step == "complete-ref-publication")
            .map(|d| d.detail.clone())
            .unwrap();
        assert!(
            said.contains(&published)
                && said.contains("on heads/other")
                && said.contains("the ref log's missing record is appended"),
            "{said}"
        );
        assert_eq!(
            calls_of(&w, "ref"),
            refs + 2,
            "prikk's plan and its completion"
        );
    } else {
        assert_eq!(calls_of(&w, "ref"), refs, "no ref publication to finish");
    }
    assert_eq!(
        calls_of(&w, "seal"),
        seals + 1,
        "prikk's recommendation, once"
    );
    assert_eq!(calls_of(&w, "commit"), commits, "recovery never commits");
    if locks {
        let wanted = if lead { 4 } else { 1 };
        assert_eq!(w.unlock_calls().len(), wanted, "{:?}", w.unlock_calls());
    }

    // Exactly one new block, on `heads/other`; the default view is as it was; the queue is drained; `verify` is green.
    assert_eq!(w.tip_in(ALT), published, "no second block");
    assert_eq!(w.history_in(ALT).len(), other_before.len() + 1);
    assert_eq!(
        w.history_in(ALT).get(1..).unwrap(),
        other_before.as_slice(),
        "the view's earlier blocks are as they were"
    );
    assert_eq!(w.history(), main_before, "the default view did not move");
    assert_eq!(w.queue_count(), 0);
    assert!(w.verifies());
    let after = w.recovery_witnesses().doctor(&w.wiki.id).unwrap();
    assert!(
        after
            .diagnostics
            .iter()
            .all(|d| !d.is_cleanup_pending() && !d.is_publication_lead()),
        "{after:?}"
    );
    assert!(w.page_in(ALT, "pages/Home.md").contains("Uno."));
    assert!(
        w.page_in(MAIN, "pages/Home.md").contains("One."),
        "the change is not in the default view"
    );

    // The worktree: the one an acceptance into `heads/other` that nothing interrupted leaves (it belongs to that view
    // now: the acceptance switched it there), the view's tip and nothing else, and not the default view's.
    assert_eq!(w.worktree_facts().unwrap(), uninterrupted);
    assert_eq!(pages_of_the_worktree(&w), uninterrupted_pages);
    assert_eq!(
        w.worktree_page("pages/Home.md"),
        w.page_in(ALT, "pages/Home.md")
    );
    assert_ne!(
        pages_of_the_worktree(&w),
        pages_at_the_start,
        "it is the other view's now"
    );

    // The records agree with the history: the acceptance sealed in that block, the intent completed, the proposal
    // accepted, the flag gone.
    let entry = w.store.acceptances_of(&p.id, 1).unwrap().remove(0);
    assert_eq!(entry.acceptance.view, ALT);
    assert!(matches!(
        entry.result.map(|r| r.outcome),
        Some(AcceptanceOutcome::Sealed { block_id, .. }) if block_id == published
    ));
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.state(&p), ProposalState::Accepted);
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert!(w.reconciled().is_clean());
    // The recorded plan and the audit trail name the view, and the steps are in the trail in order.
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    let recorded = &acts.first().unwrap().act.plan;
    assert!(recorded.contains("heads/other"), "{recorded}");
    assert!(!recorded.contains("heads/main"), "{recorded}");
    let audit = w.audit();
    let mut from = 0;
    for step in &steps {
        let at = audit
            .iter()
            .skip(from)
            .position(|(a, _, d)| a == "recovery-step" && d == step)
            .map(|i| i + from)
            .unwrap_or_else(|| panic!("{step} is audited in order: {audit:?}"));
        from = at + 1;
    }

    // A write afterwards on both views.
    assert!(matches!(
        w.accept(&q).expect("the other view takes writes again"),
        inklkunn_core::Accepted::Signed(_)
    ));
    assert_eq!(w.history_in(ALT).len(), other_before.len() + 2);
    assert_eq!(w.history(), main_before, "still untouched");
    assert!(matches!(
        w.accept(&r).expect("the default view takes writes too"),
        inklkunn_core::Accepted::Signed(_)
    ));
    assert_eq!(w.history().len(), main_before.len() + 1);
    assert_eq!(
        w.history_in(ALT).len(),
        other_before.len() + 2,
        "the other view did not move"
    );
    assert!(w.verifies());
    assert!(w.reconciled().is_clean());
    true
}

#[test]
fn a_seal_killed_before_its_ref_log_on_another_view_is_recovered_end_to_end() {
    // Maintenance M62. M61's state on `heads/other`: the real acceptor takes a proposal into the view, the `seal` is
    // killed after it moved the view's pointer and before it appended the view's ref log. `wiki recover`, plan shown
    // and then run, finishes the publication of that view, the cleanup, the worktree and the records.
    for (tag, locks) in [("m62-lead", false), ("m62-lead-locks", true)] {
        if !a_killed_seal_on_the_other_view_is_recovered_end_to_end(tag, true, locks) {
            return;
        }
    }
}

#[test]
fn a_seal_killed_before_it_drained_the_queue_on_another_view_is_recovered_end_to_end() {
    // Maintenance M62. P15's state on `heads/other`: the block is published and the ref log written, the queue still
    // holds the patch. The cleanup is finished for the act's own view, and everything after it.
    for (tag, locks) in [("m62-p15", false), ("m62-p15-locks", true)] {
        if !a_killed_seal_on_the_other_view_is_recovered_end_to_end(tag, false, locks) {
            return;
        }
    }
}

/// A wiki whose second view, `heads/other`, has a change the default view does not (so the two views' trees differ), and
/// whose worktree is a copy of `heads/other` (the last acceptance was into it) or of the default view (the last one was
/// into that); then a stray file and a scribble on a page in the worktree, as a person or a crashed tool leaves them.
/// Returns the wiki and the pages each view holds.
fn a_wiki_with_two_different_views_and_a_dirty_worktree(
    tag: &str,
    worktree_on_other: bool,
) -> Option<World> {
    let w = World::new(tag, LeaseConfig::default())?;
    w.make_other_view();
    let spanish = w.propose_in(
        Some(ALT),
        "Say it in Spanish",
        &[("pages/Home.md", Some(&HOME.replace("One.", "Uno.")))],
    );
    let hola = w.propose(
        "Greet in Spanish",
        &[("pages/Other.md", Some(&OTHER.replace("Hello.", "Hola.")))],
    );
    // The last acceptance decides which view the worktree belongs to.
    let (first, last) = if worktree_on_other {
        (&hola, &spanish)
    } else {
        (&spanish, &hola)
    };
    for p in [first, last] {
        assert!(matches!(
            w.accept(p).expect("accepted"),
            inklkunn_core::Accepted::Signed(_)
        ));
    }
    let belongs = if worktree_on_other { ALT } else { MAIN };
    assert_eq!(
        w.worktree_facts().unwrap(),
        WorktreeFacts {
            current_branch: belongs.to_owned(),
            clean: true,
        }
    );
    // The two views differ in both pages, so a baseline restored from the wrong one shows.
    assert!(w.page_in(ALT, "pages/Home.md").contains("Uno."));
    assert!(w.page_in(MAIN, "pages/Home.md").contains("One."));
    assert!(w.page_in(ALT, "pages/Other.md").contains("Hello."));
    assert!(w.page_in(MAIN, "pages/Other.md").contains("Hola."));
    std::fs::write(w.repo.join("pages/Home.md"), "scribbled\n").unwrap();
    std::fs::write(w.repo.join("pages/Other.md"), "scribbled too\n").unwrap();
    std::fs::write(w.repo.join("pages/Stray.md"), "stray\n").unwrap();
    Some(w)
}

/// An acceptance of a proposal into `view` over the dirty worktree of a wiki made by
/// [`a_wiki_with_two_different_views_and_a_dirty_worktree`]: the signer refuses to start over unclean work, and the wiki
/// is flagged. Then `wiki recover`, plan shown and run: `step` is what the restore step must name; the worktree must end
/// as the copy of `belongs`, with that view's pages and not the other's.
fn a_dirty_worktree_is_put_back_to_the_baseline_it_is_a_copy_of(
    w: &World,
    into: &str,
    belongs: &str,
) {
    let (main_before, other_before) = (w.history(), w.history_in(ALT));
    let into_other = into == ALT;
    let p = w.propose_in(
        into_other.then_some(ALT),
        "Another page",
        &[(
            "pages/Another.md",
            Some("---\ntitle: Another\nid: pn\n---\n\na\n"),
        )],
    );
    let err = w.accept(&p).unwrap_err();
    assert!(
        matches!(
            &err,
            AcceptFailure::Incomplete {
                reason: RecoveryReason::WorktreeDirty,
                flagged: true,
                ..
            }
        ),
        "{err:?}"
    );
    let intent = w.store.open_intents(&w.wiki.id).unwrap();
    assert_eq!(
        intent
            .iter()
            .map(|i| i.intent.view.as_str())
            .collect::<Vec<_>>(),
        [into],
        "the act is into {into}"
    );

    let shown = w.recovery.recover(&w.wiki, RecoveryMode::PlanOnly).unwrap();
    assert_eq!(shown.plan.stop, None, "{:#?}", shown.lines());
    assert_eq!(
        step_names(&shown),
        [
            "restore-worktree",
            "end-failed",
            "verify",
            "reconcile",
            "clear-flag"
        ]
    );
    let sentence = format!("put the worktree of {belongs} back to what its tip says");
    assert!(
        shown.lines().iter().any(|l| l.contains(&sentence)),
        "{sentence}: {:#?}",
        shown.lines()
    );
    let r = w.recover();
    assert_eq!(r.outcome, RecoveryOutcome::Recovered, "{:#?}", r.lines());
    let restored = r.done.first().unwrap();
    assert_eq!(restored.step, "restore-worktree");
    assert!(
        restored.detail.contains("3 path(s) put back"),
        "{restored:?}"
    );
    // The recorded plan names the view the worktree belongs to, and the audit trail has the step.
    let acts = w.store.recovery_acts(&w.wiki.id).unwrap();
    let recorded = &acts.first().unwrap().act.plan;
    assert!(
        recorded.contains(&format!(
            "\"step\":\"restore-worktree\",\"view\":\"{belongs}\""
        )),
        "{recorded}"
    );

    // The worktree is the baseline of the view it is a copy of: that view's pages, not the other view's.
    assert_eq!(
        w.worktree_facts().unwrap(),
        WorktreeFacts {
            current_branch: belongs.to_owned(),
            clean: true,
        }
    );
    for page in ["pages/Home.md", "pages/Other.md"] {
        assert_eq!(w.worktree_page(page), w.page_in(belongs, page), "{page}");
    }
    assert!(!w.repo.join("pages/Stray.md").exists());
    assert!(!w.repo.join("pages/Another.md").exists());
    assert_eq!(w.history(), main_before);
    assert_eq!(w.history_in(ALT), other_before);
    assert!(w.verifies());
    assert!(w.store.open_intents(&w.wiki.id).unwrap().is_empty());
    assert_eq!(w.store.recovery_flag(&w.wiki.id).unwrap(), None);
    assert_eq!(
        w.state(&p),
        ProposalState::HeldForReview,
        "the proposal stays open"
    );
    // And the wiki takes the change, into the view it was meant for, which switches the worktree there.
    assert!(matches!(
        w.accept(&p).expect("accepted now"),
        inklkunn_core::Accepted::Signed(_)
    ));
    let (main_after, other_after) = (w.history().len(), w.history_in(ALT).len());
    if into_other {
        assert_eq!(
            (main_after, other_after),
            (main_before.len(), other_before.len() + 1)
        );
    } else {
        assert_eq!(
            (main_after, other_after),
            (main_before.len() + 1, other_before.len())
        );
    }
    assert!(w.verifies());
    assert!(w.reconciled().is_clean());
}

#[test]
fn a_dirty_worktree_of_the_other_view_is_put_back_to_that_views_tip_and_not_the_default_views() {
    // Maintenance M62, the question the plan's `restore-worktree` raises. The worktree is a copy of `heads/other` (the last
    // acceptance was into it) and is left dirty; a proposal into `heads/other` is accepted, the signer finds the
    // worktree unclean and the wiki is flagged. The two views differ in the pages the scribbles are on: a baseline
    // restored from the default view would show in them.
    let Some(w) = a_wiki_with_two_different_views_and_a_dirty_worktree("m62-restore", true) else {
        return;
    };
    a_dirty_worktree_is_put_back_to_the_baseline_it_is_a_copy_of(&w, ALT, ALT);
}

#[test]
fn a_dirty_worktree_of_the_default_view_is_put_back_to_its_own_tip_when_the_act_is_into_the_other_view()
 {
    // Maintenance M62, the converse and the defect it found: the worktree is a copy of the default view, the act is into
    // `heads/other`. The signer puts back the baseline the worktree is a copy of (the default view's), and the plan, its
    // record and the audit trail used to name the act's view, `heads/other`, which was not the one restored.
    let Some(w) = a_wiki_with_two_different_views_and_a_dirty_worktree("m62-restore-main", false)
    else {
        return;
    };
    a_dirty_worktree_is_put_back_to_the_baseline_it_is_a_copy_of(&w, ALT, MAIN);
}

#[test]
fn a_dirty_worktree_of_the_other_view_is_put_back_to_its_own_tip_when_the_act_is_into_the_default_view()
 {
    // Maintenance M62, the same the other way round.
    let Some(w) = a_wiki_with_two_different_views_and_a_dirty_worktree("m62-restore-alt", true)
    else {
        return;
    };
    a_dirty_worktree_is_put_back_to_the_baseline_it_is_a_copy_of(&w, MAIN, ALT);
}
