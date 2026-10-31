//! THE KILL TEST (RFC 004 D-7; handoff 004 T5; CON-4, FL-10, DUR-2): the acceptance runs in a **child process that is
//! `SIGKILL`ed at each of D-4's steps in turn**, and recovery then leaves `prikk verify` green, exactly the expected
//! blocks in `log`, the acceptance record and the intent consistent with them, and the plan recorded.
//!
//! A step is named two ways. The acceptor's own steps ([`inklkunn_core::AcceptStep`]) are named by its test observer,
//! which the child sets to stop the process there. The prikk calls inside the signer's write sequence (`status`,
//! `worktree-status`, `commit`, `seal`, `verify`) are held by a **stand-in `prikk`** that tells the parent it is there
//! and sleeps until killed (the house pattern of `creation_kill.rs`). The parent then kills the child and the held
//! stand-in, and a fresh recovery, as the next start would run it, reads the witnesses the kill left.
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

use inklkunn_core::proposal::{ProposalEdit, ProposalService, QuotaConfig};
use inklkunn_core::reconcile::reconcile;
use inklkunn_core::{
    AcceptStep, AcceptanceParts, Authorizer, IdentityAuthority, KeyAuthority, LeaseConfig,
    Principal, ProposalAcceptor, ReadConfig, ReadService, Recover, Recovery, RecoveryMode,
    RecoveryOutcome, RecoveryParts, Signer, SignerConfig, StoreActs, StoreAuthz, StoreLedger,
    SystemActs, WikiLeases, Witnesses, WriteLeases, create_wiki,
};
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    AcceptanceOutcome, AcceptanceStore, Account, AccountId, AccountStore, ActStore, DisplayName,
    FileChange, IntentEnd, NewWiki, ProposalId, ProposalRecord, ProposalState, RecoveryEnding,
    Role, RoleGrant, RoleStore, RootLayout, Slug, SqliteStore, WikiId, WikiRecord, WikiStore,
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

/// The stand-in `prikk`: counts its calls per verb and, when `block-<verb>` names the call's index, writes its own
/// process id to `blocked-<verb>-<n>` and becomes a sleep until killed. Every other call goes to the real prikk.
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
n="$(cat "$CTL/count-$verb" 2>/dev/null || echo 0)"
echo "$((n + 1))" > "$CTL/count-$verb"
if [ -e "$CTL/block-$verb" ] && [ "$(cat "$CTL/block-$verb")" = "$n" ]; then
  echo "$$" > "$CTL/blocked-$verb-$n"
  exec sleep 3600
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

/// The child: accept the proposal the spec names, and stop where the spec says. It has no way to finish.
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
    let mut acceptor = s.acceptor();
    if let Some(step) = step {
        let reached = ctl.join("reached");
        acceptor = acceptor.with_observer(Arc::new(move |at: AcceptStep| {
            if at.as_str() == step {
                std::fs::write(&reached, at.as_str()).unwrap();
                std::thread::sleep(Duration::from_secs(3600));
            }
        }));
    }
    let wiki = s.store.get(&WikiId::parse(&text("wiki")).unwrap()).unwrap();
    let who = Principal::session(authority(), AccountId::parse(&text("maintainer")).unwrap());
    let outcome = acceptor.accept(
        &who,
        &wiki,
        &ProposalId::parse(&text("proposal")).unwrap(),
        1,
    );
    // Reaching here means the child was not stopped: tell the parent, which treats it as a failure of the test.
    std::fs::write(ctl.join("finished"), format!("{outcome:?}")).unwrap();
}

/// How a kill point is named.
#[derive(Clone, Copy)]
enum At {
    /// One of the acceptor's steps (D-4), by the observer.
    Step(AcceptStep),
    /// The `n`-th call of a prikk verb since the child began (0-based), held by the stand-in before it runs.
    Prikk(&'static str, u32),
}

impl At {
    fn label(self) -> String {
        match self {
            At::Step(s) => s.as_str().to_owned(),
            At::Prikk(verb, n) => format!("prikk {verb} #{n}"),
        }
    }
}

/// What recovery must have made of a kill at one point.
struct Expect {
    /// The steps of the recorded plan, in order.
    steps: &'static [&'static str],
    /// The change is in the history afterwards (one block more).
    sealed: bool,
    /// An acceptance record exists afterwards (the kill came after it was written).
    record: bool,
}

const TAIL: [&str; 3] = ["verify", "reconcile", "clear-flag"];

/// The matrix: D-4's steps in turn, then the prikk calls of the signer's write sequence between the intent and the
/// signer's return.
fn matrix() -> Vec<(At, Expect)> {
    vec![
        // 1 – 2: only the lease holder's record exists; nothing else was written.
        (
            At::Step(AcceptStep::LeaseTaken),
            Expect {
                steps: &TAIL,
                sealed: false,
                record: false,
            },
        ),
        (
            At::Step(AcceptStep::Prepared),
            Expect {
                steps: &TAIL,
                sealed: false,
                record: false,
            },
        ),
        // 4: the acceptance record is written and the intent is not: the signer was never asked.
        (
            At::Step(AcceptStep::AcceptanceRecorded),
            Expect {
                steps: &["end-failed", "verify", "reconcile", "clear-flag"],
                sealed: false,
                record: true,
            },
        ),
        // The intent is written; the signer has not been asked, or was asked and had done nothing yet.
        (
            At::Step(AcceptStep::IntentRecorded),
            Expect {
                steps: &["end-failed", "verify", "reconcile", "clear-flag"],
                sealed: false,
                record: true,
            },
        ),
        (
            At::Prikk("status", 0),
            Expect {
                steps: &["end-failed", "verify", "reconcile", "clear-flag"],
                sealed: false,
                record: true,
            },
        ),
        (
            At::Prikk("worktree-status", 0),
            Expect {
                steps: &["end-failed", "verify", "reconcile", "clear-flag"],
                sealed: false,
                record: true,
            },
        ),
        // The files are in the worktree and `commit` has not run: the worktree is put back.
        (
            At::Prikk("commit", 0),
            Expect {
                steps: &[
                    "restore-worktree",
                    "end-failed",
                    "verify",
                    "reconcile",
                    "clear-flag",
                ],
                sealed: false,
                record: true,
            },
        ),
        // Committed, not sealed: a patch is queued, and recovery completes it with `seal` alone.
        (
            At::Prikk("seal", 0),
            Expect {
                steps: &[
                    "seal-queued",
                    "end-sealed",
                    "verify",
                    "reconcile",
                    "clear-flag",
                ],
                sealed: true,
                record: true,
            },
        ),
        // Sealed in prikk, none of it recorded.
        (
            At::Prikk("verify", 0),
            Expect {
                steps: &["end-sealed", "verify", "reconcile", "clear-flag"],
                sealed: true,
                record: true,
            },
        ),
        (
            At::Step(AcceptStep::SignerReturned),
            Expect {
                steps: &["end-sealed", "verify", "reconcile", "clear-flag"],
                sealed: true,
                record: true,
            },
        ),
        // The acceptance's end is written; the intent's and the proposal's are not.
        (
            At::Step(AcceptStep::AcceptanceSealed),
            Expect {
                steps: &["end-sealed", "verify", "reconcile", "clear-flag"],
                sealed: true,
                record: true,
            },
        ),
        // The intent is complete; the proposal is not yet accepted.
        (
            At::Step(AcceptStep::IntentCompleted),
            Expect {
                steps: &["end-sealed", "verify", "reconcile", "clear-flag"],
                sealed: true,
                record: true,
            },
        ),
        // Every record is written; the caches, the reconciliation and the lease are what was left.
        (
            At::Step(AcceptStep::ProposalAccepted),
            Expect {
                steps: &TAIL,
                sealed: true,
                record: true,
            },
        ),
        (
            At::Step(AcceptStep::Reconciled),
            Expect {
                steps: &TAIL,
                sealed: true,
                record: true,
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
}

fn fixture(tag: &str) -> Option<Fixture> {
    let Some(real) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
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
    let have = inklkunn_core::import::wiki_state(&*s.storage, &s.layout, &wiki).unwrap();
    let plan = inklkunn_core::import::plan_folder(&folder.0, &have, false).unwrap();
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
    })
}

impl Fixture {
    /// Run the child to the kill point and kill it. Returns once nothing of it is left.
    fn kill_at(&self, at: At) {
        for entry in std::fs::read_dir(&self.ctl).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != "prikk" {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
        let (step, marker) = match at {
            At::Step(s) => (Some(s.as_str().to_owned()), self.ctl.join("reached")),
            At::Prikk(verb, n) => {
                std::fs::write(self.ctl.join(format!("block-{verb}")), n.to_string()).unwrap();
                (None, self.ctl.join(format!("blocked-{verb}-{n}")))
            }
        };
        let spec = serde_json::json!({
            "root": self.paths.root, "keys": self.paths.keys, "secret": self.paths.secret,
            "shm": self.paths.shm, "store": self.paths.store, "prikk": self.paths.prikk,
            "ctl": self.ctl, "wiki": self.wiki.id.as_str(), "maintainer": self.maintainer.as_str(),
            "proposal": self.first.id.as_str(), "step": step,
        });
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
            .spawn()
            .expect("child");
        let deadline = Instant::now() + Duration::from_secs(120);
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "{}: the child never reached the kill point",
                at.label()
            );
            if let Ok(Some(status)) = child.try_wait() {
                let finished =
                    std::fs::read_to_string(self.ctl.join("finished")).unwrap_or_default();
                panic!(
                    "{}: the child ended ({status}) without being stopped: {finished}",
                    at.label()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // The held stand-in, if that is where it is: the one process this test started that is not the child.
        let held: Option<String> = match at {
            At::Prikk(..) => std::fs::read_to_string(&marker)
                .ok()
                .map(|p| p.trim().to_owned()),
            At::Step(_) => None,
        };
        child.kill().expect("SIGKILL");
        let _ = child.wait();
        if let Some(pid) = held {
            // Exactly that process, by its id: it was started by the child and outlives it.
            let _ = Command::new("kill").args(["-9", &pid]).status();
        }
        // Nothing of the kill may go on to write: let the stand-in's block be a pass-through again.
        let _ = std::fs::remove_file(self.ctl.join(match at {
            At::Prikk(verb, _) => format!("block-{verb}"),
            At::Step(_) => "none".to_owned(),
        }));
    }
}

fn log(s: &Services, repo: &Path) -> Vec<inklkunn_prikk::HistoryBlock> {
    s.storage
        .history(repo, &Point::View(MAIN.to_owned()), None)
        .unwrap()
        .blocks
}

#[test]
fn a_sigkill_at_each_step_of_an_acceptance_is_recovered_with_nothing_lost_or_made_twice() {
    if std::env::var_os("INKLKUNN_AK_CHILD").is_some() {
        return; // the child runs `child_accepts_and_is_stopped`, not this test
    }
    let mut table = Vec::new();
    for (at, expect) in matrix() {
        let Some(f) = fixture(&format!("k{}", table.len())) else {
            return;
        };
        let label = at.label();
        f.kill_at(at);

        // What the next start finds: a wiki that needs recovery, by its witnesses, in a process that knows nothing
        // of the one that died.
        let s = open_services(&f.paths, "restarted", false);
        let recovery = s.recovery();
        let reasons = recovery.needs_recovery(&f.wiki.id).unwrap();
        assert!(
            !reasons.is_empty(),
            "{label}: the kill left no witness at all"
        );
        // The plan, shown first, changes nothing and is the plan the run then records.
        let shown = recovery.recover(&f.wiki, RecoveryMode::PlanOnly).unwrap();
        assert_eq!(
            shown
                .plan
                .steps
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>(),
            expect.steps,
            "{label}: the plan shown"
        );
        assert!(
            s.store.recovery_acts(&f.wiki.id).unwrap().is_empty(),
            "{label}: a plan shown is not recorded"
        );
        let blocks_killed = log(&s, &f.repo);

        let report = recovery
            .recover(&f.wiki, RecoveryMode::Run)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(
            report.outcome,
            RecoveryOutcome::Recovered,
            "{label}: {:#?}",
            report.lines()
        );
        assert_eq!(
            report
                .plan
                .steps
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>(),
            expect.steps,
            "{label}: the plan run"
        );

        // `prikk verify` is green, and exactly the expected blocks are in the log.
        assert!(
            s.storage.verify(&f.repo).unwrap().verdict.ok,
            "{label}: verify"
        );
        let blocks = log(&s, &f.repo);
        let expected_len = f.base_blocks + usize::from(expect.sealed);
        assert_eq!(blocks.len(), expected_len, "{label}: the blocks in the log");
        if !expect.sealed {
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

        // The acceptance record and the intent agree with the log.
        let entries = s.store.acceptances_of(&f.first.id, 1).unwrap();
        assert_eq!(
            entries.len(),
            usize::from(expect.record),
            "{label}: acceptance records"
        );
        assert!(
            s.store.open_intents(&f.wiki.id).unwrap().is_empty(),
            "{label}: an intent is still open"
        );
        assert!(
            s.store.pending_acceptances(&f.wiki.id).unwrap().is_empty(),
            "{label}: an acceptance is still pending"
        );
        let proposal = s
            .proposals
            .get(
                &Principal::session(authority(), f.maintainer.clone()),
                &f.wiki,
                &f.first.id,
            )
            .unwrap();
        if expect.sealed {
            let newest = blocks.first().unwrap();
            let message = inklkunn_core::message::Message::parse(
                &newest.patch_messages.first().unwrap().message,
            )
            .unwrap();
            assert_eq!(
                message.proposal_id(),
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
            expect.sealed,
            "{label}: the worktree is the tip"
        );

        // The plan is recorded, and ended.
        let acts = s.store.recovery_acts(&f.wiki.id).unwrap();
        assert_eq!(acts.len(), 1, "{label}: recovery acts");
        let act = acts.first().unwrap();
        assert_eq!(
            act.end.as_ref().map(|e| e.ending),
            Some(RecoveryEnding::Recovered),
            "{label}"
        );
        assert_eq!(Some(&act.act.id), report.act.as_ref(), "{label}");
        for step in expect.steps {
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

        // And the wiki takes writes: the other proposal is accepted, and a retry of the first is told what is true.
        let acceptor = s.acceptor();
        let who = Principal::session(authority(), f.maintainer.clone());
        acceptor
            .accept(&who, &f.wiki, &f.second.id, 1)
            .unwrap_or_else(|e| panic!("{label}: a write after recovery: {e}"));
        assert_eq!(log(&s, &f.repo).len(), expected_len + 1, "{label}");
        let retry = acceptor.accept(&who, &f.wiki, &f.first.id, 1);
        if expect.sealed {
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
        table.push((
            label,
            expect.sealed,
            report
                .plan
                .steps
                .iter()
                .map(|x| x.name())
                .collect::<Vec<_>>()
                .join(" > "),
        ));
    }
    println!("kill-test matrix (step killed -> recovery outcome):");
    for (label, sealed, steps) in &table {
        println!(
            "  {label:<26} -> {} ; plan: {steps}",
            if *sealed {
                "change in the history, records completed"
            } else {
                "nothing in the history, act failed (interrupted)"
            }
        );
    }
    assert_eq!(table.len(), matrix().len());
}
