//! The signer's recovery operations against a real prikk (RFC 004 D-7; handoff 004 T5): a patch queued by a seal that
//! failed on cue is sealed alone, a torn log is discarded, stale locks are cleared on the lease's word and not prikk's
//! (ledger PL-8), a dirty worktree is put back; and every operation refuses without the lent lease or a recorded act
//! that names it. Self-skips with a visible line when prikk is not on `PATH`.
//!
//! A stand-in `prikk` fails the verb a control file names, so that an acceptance ends after `commit`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use inklkunn_prikk::Sandbox;
use inklkunn_signer::message::{Actor, BlockId, Message, Summary, SystemLabel};
use inklkunn_signer::recover::{
    ACCEPTANCE_LOCK_KINDS, Recover, RecoverError, RecoveryFacts, RecoveryOp, RecoveryToken,
    Witnesses,
};
use inklkunn_signer::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, LedgerError,
    PreparedChange, PreparedFile, RepositoryBootstrap, Signer, SignerConfig, ViewCreationAct,
    ViewCreationFacts, WriteLeases,
};
use inklkunn_store::{AccountId, DisplayName, ProposalId, RootLayout, WikiId};

const SECRET: &str = "an operator secret for the recovery tests, long enough";
const MAIN: &str = "heads/main";

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
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
    let p = base.join(format!("inklkunn-rc-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

#[derive(Default)]
struct Ledger {
    pending: Mutex<HashMap<String, ExpectedMessage>>,
    acts: Mutex<HashMap<String, RecoveryFacts>>,
}

impl AcceptanceLedger for Ledger {
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(self.pending.lock().unwrap().get(token.as_str()).cloned())
    }

    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }

    fn recovery(&self, act: &RecoveryToken) -> Result<Option<RecoveryFacts>, LedgerError> {
        Ok(self.acts.lock().unwrap().get(act.as_str()).cloned())
    }
}

fn account(name: &str) -> Actor {
    Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new(name).expect("name"),
    }
}

/// The stand-in: every call goes to the real prikk unless a control file names the verb (`fail-<verb>`).
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
if [ -e "$CTL/fail-$verb" ]; then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
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

struct Fixture {
    _tmp: Dir,
    _shm: Dir,
    ctl: PathBuf,
    wiki: WikiId,
    repo: PathBuf,
    ledger: Arc<Ledger>,
    signer: Signer,
    witnesses: Witnesses,
    leases: Arc<WriteLeases>,
    prikk: PathBuf,
    first_block: String,
    proposer: Actor,
    acceptor: Actor,
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
    let prikk = write_standin(&ctl, &real);
    let root = RootLayout::new(tmp.0.join("root"));
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, format!("{SECRET}\n")).unwrap();
    let mut cfg = SignerConfig::new(
        &prikk,
        Sandbox::Unconfined,
        root.clone(),
        tmp.0.join("keys"),
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let ledger = Arc::new(Ledger::default());
    let leases = WriteLeases::new(root.clone());
    let signer = Signer::open_creating_store(cfg, ledger.clone())
        .expect("signer opens")
        .with_leases(&leases);
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).expect("directories");
    signer
        .create_repository(&repo, &inklkunn_signer::KeyAuthority::for_tests())
        .expect("repository and keys");
    for (path, text) in [
        ("pages/Home.md", "---\ntitle: Home\n---\n\nHello.\n"),
        ("pages/Edit.md", "---\ntitle: Edit\n---\n\nbefore\n"),
        ("pages/Old A.md", "---\ntitle: A\n---\n\nA\n"),
        ("pages/Old B.md", "---\ntitle: B\n---\n\nB\n"),
        ("pages/Old C.md", "---\ntitle: C\n---\n\nC\n"),
    ] {
        let p = repo.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let first = Message::system(
        Summary::from_text("Create the wiki").unwrap(),
        SystemLabel::inklkunn(),
    );
    let sealed = signer
        .commit_and_seal(&repo, &first.to_text())
        .expect("the first change");
    let witnesses = Witnesses::new(&prikk, Sandbox::Unconfined, root);
    Some(Fixture {
        _tmp: tmp,
        _shm: shm,
        ctl,
        wiki,
        repo,
        ledger,
        signer,
        witnesses,
        leases,
        prikk,
        first_block: sealed.block_id,
        proposer: account("Aiko Tanaka"),
        acceptor: account("Ben (maintainer)"),
    })
}

fn file(path: &str, content: Option<&str>) -> PreparedFile {
    PreparedFile {
        path: PathBuf::from(path),
        content: content.map(|c| c.as_bytes().to_vec()),
    }
}

/// A change and the record the ledger holds for it.
struct Prepared {
    token: AcceptanceToken,
    change: PreparedChange,
}

impl Fixture {
    fn prepare(&self, files: Vec<PreparedFile>, renames: Vec<(&str, &str)>) -> Prepared {
        let proposal = ProposalId::new();
        let base = BlockId::new(&self.first_block).unwrap();
        let message = Message::proposal(
            Summary::from_text("Improve the wiki").unwrap(),
            proposal.clone(),
            self.proposer.clone(),
            self.acceptor.clone(),
            base.clone(),
        )
        .unwrap();
        let token = AcceptanceToken::new(format!("acceptance-{proposal}"));
        let change = PreparedChange {
            files,
            renames: renames
                .into_iter()
                .map(|(a, b)| (PathBuf::from(a), PathBuf::from(b)))
                .collect(),
            message: message.to_text(),
        };
        let expected = ExpectedMessage {
            wiki: self.wiki.clone(),
            view: MAIN.to_owned(),
            kind: message.kind(),
            proposal: Some(proposal),
            proposed_by: self.proposer.clone(),
            accepted_by: self.acceptor.clone(),
            base: Some(base),
            imported_from: None,
            pinned_tip: Some(BlockId::new(&self.first_block).unwrap()),
            change_digest: change.digest(),
        };
        self.ledger
            .pending
            .lock()
            .unwrap()
            .insert(token.as_str().to_owned(), expected);
        Prepared { token, change }
    }

    /// Accept `p` with the seal made to fail: one patch is queued, nothing is sealed.
    fn queue(&self, p: &Prepared) {
        std::fs::write(self.ctl.join("fail-seal"), "").unwrap();
        let r = self.signer.accept(&self.wiki, MAIN, &p.token, &p.change);
        std::fs::remove_file(self.ctl.join("fail-seal")).unwrap();
        assert!(matches!(r, Err(AcceptError::Queued { .. })), "{r:?}");
        assert_eq!(self.queue_count(), 1);
    }

    /// A recorded act that allows `ops`.
    fn act(&self, ops: Vec<RecoveryOp>) -> RecoveryToken {
        let token = RecoveryToken::new(format!("recovery-{}", ProposalId::new()));
        self.ledger.acts.lock().unwrap().insert(
            token.as_str().to_owned(),
            RecoveryFacts {
                wiki: self.wiki.clone(),
                ops,
            },
        );
        token
    }

    fn prikk_json(&self, args: &[&str]) -> serde_json::Value {
        let out = Command::new(&self.prikk)
            .args(args)
            .current_dir(&self.repo)
            .output()
            .expect("prikk runs");
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
    }

    fn queue_count(&self) -> u64 {
        self.prikk_json(&["status", "--format", "json"])
            .pointer("/queue/count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(u64::MAX)
    }

    fn worktree_clean(&self) -> bool {
        self.prikk_json(&["worktree-status", "--format", "json"])
            .get("clean")
            == Some(&serde_json::Value::Bool(true))
    }

    fn blocks(&self) -> usize {
        self.prikk_json(&["log", "--ref", MAIN, "--format", "json"])
            .pointer("/blocks")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len)
    }

    fn verify_ok(&self) -> bool {
        Command::new(&self.prikk)
            .args(["verify"])
            .current_dir(&self.repo)
            .output()
            .expect("prikk runs")
            .status
            .success()
    }

    fn seal_op(&self, p: &Prepared) -> RecoveryOp {
        RecoveryOp::SealQueued {
            acceptance: p.token.as_str().to_owned(),
            view: MAIN.to_owned(),
        }
    }

    /// The wiki's lease, lent to this thread for the closure.
    fn leased<T>(&self, f: impl FnOnce() -> T) -> T {
        let lease = self
            .leases
            .try_acquire(&self.wiki)
            .expect("the lease is free");
        let scope = lease.scope();
        let out = f();
        drop(scope);
        out
    }
}

const HOME_EDITED: &str = "---\ntitle: Home\n---\n\nHello, world.\n";

#[test]
fn a_queued_patch_is_sealed_alone_once_and_the_log_shows_one_new_block() {
    let Some(f) = fixture("seal") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let before = f.blocks();
    let act = f.act(vec![f.seal_op(&p)]);
    f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token))
        .expect("sealed");
    assert_eq!(f.blocks(), before + 1, "one new block");
    assert_eq!(f.queue_count(), 0);
    assert!(f.verify_ok());
    assert!(f.worktree_clean());
    // The message in the log is the one that was queued: nothing was committed a second time.
    let log = f.prikk_json(&["log", "--ref", MAIN, "--limit", "1", "--format", "json"]);
    assert_eq!(
        log.pointer("/blocks/0/patch_messages/0/message")
            .and_then(serde_json::Value::as_str),
        Some(p.change.message.as_str())
    );
    assert_eq!(
        log.pointer("/blocks/0/patch_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    // A second seal finds no queue and refuses.
    let again = f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token));
    assert!(
        matches!(again, Err(RecoverError::DoesNotMatch(_))),
        "{again:?}"
    );
}

#[test]
fn a_queued_patch_that_is_not_the_recorded_change_is_not_sealed() {
    let Some(f) = fixture("mismatch") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let before = f.blocks();
    // The record describes another change (another proposal, another files): the queued patch is not it.
    let other = f.prepare(
        vec![file(
            "pages/Home.md",
            Some("---\ntitle: Home\n---\n\nOther.\n"),
        )],
        vec![],
    );
    let act = f.act(vec![f.seal_op(&other)]);
    let r = f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &other.token));
    assert!(
        matches!(r, Err(RecoverError::DoesNotMatch(_))),
        "message differs: {r:?}"
    );
    // The right message with other files: the record's digest is not the queued patch's.
    let proposal_id = {
        let m = Message::parse(&p.change.message).unwrap();
        m.proposal_id().cloned().unwrap()
    };
    let liar = f.prepare(
        vec![file(
            "pages/Home.md",
            Some("---\ntitle: Home\n---\n\nNot this.\n"),
        )],
        vec![],
    );
    let mut expected = f
        .ledger
        .pending
        .lock()
        .unwrap()
        .get(p.token.as_str())
        .cloned()
        .unwrap();
    expected.change_digest = liar.change.digest();
    f.ledger
        .pending
        .lock()
        .unwrap()
        .insert(p.token.as_str().to_owned(), expected);
    let act = f.act(vec![f.seal_op(&p)]);
    let r = f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token));
    assert!(
        matches!(&r, Err(RecoverError::DoesNotMatch(why)) if why.contains("files")),
        "files differ: {r:?} ({proposal_id})"
    );
    assert_eq!(f.blocks(), before, "nothing was sealed");
    assert_eq!(f.queue_count(), 1, "the patch is still queued");
}

#[test]
fn renames_declared_in_any_order_are_proven_against_the_recorded_digest() {
    let Some(f) = fixture("renames") else {
        return;
    };
    // prikk lists a patch's renames sorted; the digest binds the order they were declared in.
    let p = f.prepare(
        vec![],
        vec![
            ("pages/Old C.md", "pages/New Z.md"),
            ("pages/Old A.md", "pages/New Y.md"),
            ("pages/Old B.md", "pages/New X.md"),
        ],
    );
    f.queue(&p);
    let act = f.act(vec![f.seal_op(&p)]);
    f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token))
        .expect("sealed");
    assert!(f.verify_ok());
    assert_eq!(f.queue_count(), 0);
}

#[test]
fn every_operation_refuses_without_the_lease_without_a_record_and_without_the_act_naming_it() {
    let Some(f) = fixture("guards") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let all = vec![
        f.seal_op(&p),
        RecoveryOp::DiscardDamaged,
        RecoveryOp::ClearLocks {
            paths: vec!["/x".to_owned()],
        },
        RecoveryOp::RestoreWorktree {
            view: MAIN.to_owned(),
        },
    ];
    let act = f.act(all);
    let gone = RecoveryToken::new("never-recorded");
    let narrow = f.act(vec![RecoveryOp::DiscardDamaged]);
    let run = |a: &RecoveryToken| -> Vec<Result<(), RecoverError>> {
        vec![
            f.signer.seal_queued(&f.wiki, MAIN, a, &p.token),
            f.signer.discard_damaged(&f.wiki, a).map(|_| ()),
            f.signer
                .clear_locks(&f.wiki, a, &["/x".to_owned()])
                .map(|_| ()),
            f.signer.restore_worktree(&f.wiki, MAIN, a).map(|_| ()),
        ]
    };
    // No lease lent: every one refuses, whatever the record says, and nothing is taken or waited for.
    for r in run(&act) {
        assert_eq!(r, Err(RecoverError::NoLease));
    }
    // A lease that exists but is not lent to this thread is not the lease either.
    let lease = f.leases.try_acquire(&f.wiki).expect("free");
    for r in run(&act) {
        assert_eq!(r, Err(RecoverError::NoLease));
    }
    let scope = lease.scope();
    // Lent, but the act is not recorded: nothing is done.
    for r in run(&gone) {
        assert_eq!(r, Err(RecoverError::NotRecorded));
    }
    // Lent and recorded, but the act names only the discard: the other three refuse.
    let rs = run(&narrow);
    assert_eq!(nth(&rs, 0), &Err(RecoverError::NotRecorded));
    assert_ne!(
        nth(&rs, 1),
        &Err(RecoverError::NotRecorded),
        "the discard is named"
    );
    assert_eq!(nth(&rs, 2), &Err(RecoverError::NotRecorded));
    assert_eq!(nth(&rs, 3), &Err(RecoverError::NotRecorded));
    // An act of another wiki is not this wiki's.
    let foreign = RecoveryToken::new("foreign");
    f.ledger.acts.lock().unwrap().insert(
        foreign.as_str().to_owned(),
        RecoveryFacts {
            wiki: WikiId::new(),
            ops: vec![RecoveryOp::DiscardDamaged],
        },
    );
    assert_eq!(
        f.signer.discard_damaged(&f.wiki, &foreign).map(|_| ()),
        Err(RecoverError::NotRecorded)
    );
    drop(scope);
    assert_eq!(f.queue_count(), 1, "nothing of the above touched the queue");
}

#[test]
fn a_torn_log_is_discarded_and_the_repository_verifies_again() {
    let Some(f) = fixture("discard") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let wal = f.repo.join(".prikk/active/default/queue.wal");
    let len = std::fs::metadata(&wal).unwrap().len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&wal)
        .unwrap()
        .set_len(len - 30)
        .unwrap();
    // The witnesses see it: no queue, torn bytes, an acknowledged damage in `doctor`, and a failing `verify`.
    assert_eq!(f.queue_count(), 0);
    let report = f.witnesses.doctor(&f.wiki).expect("doctor reads");
    assert!(report.has_acknowledged_damage(), "{report:?}");
    assert!(!f.verify_ok());
    // Sealing is not an option (there is no queue to seal), and a discard needs its act.
    let act = f.act(vec![RecoveryOp::DiscardDamaged]);
    let done = f
        .leased(|| f.signer.discard_damaged(&f.wiki, &act))
        .expect("discarded");
    assert!(done.patch_id.is_some(), "{done:?}");
    assert!(f.verify_ok());
    assert_eq!(f.blocks(), 1, "the history is as it was");
    assert!(
        !f.witnesses
            .doctor(&f.wiki)
            .unwrap()
            .has_acknowledged_damage()
    );
}

fn plant_lock(repo: &Path, relative: &str, pid: u32, kind: &str) -> String {
    let path = repo.join(".prikk").join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("pid={pid}\nkind={kind}\nnote=planted\n")).unwrap();
    std::fs::canonicalize(&path).unwrap().display().to_string()
}

#[test]
fn a_stale_lock_is_cleared_on_the_record_and_prikks_own_liveness_check_decides_nothing() {
    let Some(f) = fixture("locks") else {
        return;
    };
    // A lock whose recorded process id is a live process of this namespace: prikk says "appears running".
    let me = std::process::id();
    let live = plant_lock(&f.repo, "active/default/active.lock", me, "active");
    // And one whose process is gone.
    let dead = plant_lock(
        &f.repo,
        "containers/objects.lock",
        2_147_483_000,
        "container:object-store",
    );
    let listed = f.witnesses.locks(&f.wiki).expect("the listing is read");
    assert_eq!(listed.len(), 2, "{listed:?}");
    let by = |p: &str| listed.iter().find(|l| l.path == p).unwrap();
    assert_eq!(by(&live).recorded_pid, me);
    assert!(
        by(&live).liveness.starts_with("appears running"),
        "{:?}",
        by(&live)
    );
    assert!(by(&dead).liveness.starts_with("does not appear"));
    let act = f.act(vec![RecoveryOp::ClearLocks {
        paths: vec![live.clone(), dead.clone()],
    }]);
    let cleared = f
        .leased(|| {
            f.signer
                .clear_locks(&f.wiki, &act, &[live.clone(), dead.clone()])
        })
        .expect("cleared");
    assert_eq!(
        cleared.len(),
        2,
        "the lock that appears to be held is cleared too: PL-8"
    );
    assert!(f.witnesses.locks(&f.wiki).unwrap().is_empty());
    // A lock that is already gone is not an error, and is not listed as cleared.
    let again = f
        .leased(|| {
            f.signer
                .clear_locks(&f.wiki, &act, std::slice::from_ref(&live))
        })
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn a_lock_the_plan_did_not_name_or_of_a_kind_an_acceptance_does_not_leave_is_not_cleared() {
    let Some(f) = fixture("lockkinds") else {
        return;
    };
    let ours = plant_lock(
        &f.repo,
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    let policy = plant_lock(
        &f.repo,
        "trust/policy.lock",
        2_147_483_000,
        "container:trust-policy",
    );
    assert!(!ACCEPTANCE_LOCK_KINDS.contains(&"container:trust-policy"));
    // Named by the plan, but of a kind an acceptance does not leave: nothing is cleared, not even the other.
    let act = f.act(vec![RecoveryOp::ClearLocks {
        paths: vec![ours.clone(), policy.clone()],
    }]);
    let r = f.leased(|| {
        f.signer
            .clear_locks(&f.wiki, &act, &[ours.clone(), policy.clone()])
    });
    assert!(matches!(r, Err(RecoverError::DoesNotMatch(_))), "{r:?}");
    assert_eq!(
        f.witnesses.locks(&f.wiki).unwrap().len(),
        2,
        "all checked before any is cleared"
    );
    // Not named by the plan: the signer does not clear it.
    let narrow = f.act(vec![RecoveryOp::ClearLocks {
        paths: vec![ours.clone()],
    }]);
    let r = f.leased(|| {
        f.signer
            .clear_locks(&f.wiki, &narrow, std::slice::from_ref(&policy))
    });
    assert_eq!(r, Err(RecoverError::NotRecorded));
    assert_eq!(f.witnesses.locks(&f.wiki).unwrap().len(), 2);
}

#[test]
fn a_dirty_worktree_is_put_back_to_the_tip_and_a_queued_patch_is_never_touched() {
    let Some(f) = fixture("restore") else {
        return;
    };
    let act = f.act(vec![RecoveryOp::RestoreWorktree {
        view: MAIN.to_owned(),
    }]);
    // Clean already: nothing to put back.
    assert_eq!(
        f.leased(|| f.signer.restore_worktree(&f.wiki, MAIN, &act)),
        Ok(0)
    );
    // Modified, missing, untracked (in a directory of its own), a declared rename.
    std::fs::write(f.repo.join("pages/Home.md"), "scribbled\n").unwrap();
    std::fs::remove_file(f.repo.join("pages/Edit.md")).unwrap();
    std::fs::create_dir_all(f.repo.join("pages/sub")).unwrap();
    std::fs::write(f.repo.join("pages/sub/New.md"), "new\n").unwrap();
    std::fs::write(f.repo.join("pages/Plain.md"), "plain\n").unwrap();
    let mv = Command::new(&f.prikk)
        .args(["mv", "pages/Old A.md", "pages/Moved A.md"])
        .current_dir(&f.repo)
        .output()
        .unwrap();
    assert!(mv.status.success());
    assert!(!f.worktree_clean());
    let n = f
        .leased(|| f.signer.restore_worktree(&f.wiki, MAIN, &act))
        .expect("restored");
    assert!(n >= 5, "{n}");
    assert!(f.worktree_clean());
    assert_eq!(
        std::fs::read_to_string(f.repo.join("pages/Home.md")).unwrap(),
        "---\ntitle: Home\n---\n\nHello.\n"
    );
    assert!(f.repo.join("pages/Edit.md").is_file() && f.repo.join("pages/Old A.md").is_file());
    assert!(!f.repo.join("pages/Plain.md").exists() && !f.repo.join("pages/sub/New.md").exists());
    assert_eq!(f.blocks(), 1, "the history is untouched");
    // With a patch queued the worktree is its result: refused, and nothing changes.
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let r = f.leased(|| f.signer.restore_worktree(&f.wiki, MAIN, &act));
    assert!(matches!(r, Err(RecoverError::DoesNotMatch(_))), "{r:?}");
    assert_eq!(f.queue_count(), 1);
    assert!(
        std::fs::read_to_string(f.repo.join("pages/Home.md"))
            .unwrap()
            .contains("world")
    );
}
