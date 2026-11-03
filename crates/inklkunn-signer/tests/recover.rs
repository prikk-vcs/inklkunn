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

/// The stand-in: every call goes to the real prikk unless a control file names the verb (`fail-<verb>`), or says what
/// the plain-text `status` (`say-status-text`) or `verify` (`say-verify-text`, `say-verify-json`) prints.
fn write_standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
if [ -e "$CTL/fail-$verb" ]; then echo "error: precondition not met: injected failure of $verb" >&2; exit 1; fi
if [ "$1 $2 $3" = "trust maintainer remove" ] && [ -e "$CTL/fail-trust-remove" ]; then echo "error: precondition not met: injected failure of the removal" >&2; exit 1; fi
if [ "$verb" = status ] && [ $# -eq 1 ] && [ -e "$CTL/say-status-text" ]; then cat "$CTL/say-status-text"; exit 0; fi
if [ "$verb" = verify ] && [ $# -eq 1 ] && [ -e "$CTL/say-verify-text" ]; then cat "$CTL/say-verify-text"; exit 1; fi
if [ "$verb" = verify ] && [ $# -gt 1 ] && [ -e "$CTL/say-verify-json" ]; then cat "$CTL/say-verify-json"; exit 1; fi
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
    /// The real prikk, whatever stands in front of it.
    real: PathBuf,
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
        real,
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
            pinned_tip: BlockId::new(&self.first_block).unwrap(),
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

    fn publish_op(&self, p: &Prepared) -> RecoveryOp {
        RecoveryOp::CompletePublication {
            acceptance: p.token.as_str().to_owned(),
            view: MAIN.to_owned(),
        }
    }

    /// What a `seal` killed after it published its block and before it drained the queue leaves (handoff 004, P15;
    /// measured with real kills on prikk 0.55.0): `p` is queued, then sealed, and then the three files of the active
    /// session that the seal had emptied are put back as they were, so that the block is in the history **and** the
    /// patch is still in the queue. `status --format json` refuses, and `doctor` names exactly
    /// `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`. Returns the block.
    fn publish_and_leave_the_queue(&self, p: &Prepared) -> String {
        let block = self.leave_the_queue_after_publishing(p);
        assert!(
            self.witnesses
                .doctor(&self.wiki)
                .unwrap()
                .is_only_cleanup_pending()
        );
        block
    }

    /// [`Fixture::publish_and_leave_the_queue`] without the claim that the cleanup is the only thing `doctor` names
    /// (on a wiki with a retired key it is not).
    fn leave_the_queue_after_publishing(&self, p: &Prepared) -> String {
        self.queue(p);
        let active = self.repo.join(".prikk/active/default");
        let kept: Vec<(PathBuf, Vec<u8>)> = ["queue.wal", "ref-name", "witness"]
            .iter()
            .map(|name| {
                let path = active.join(name);
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let act = self.act(vec![self.seal_op(p)]);
        self.leased(|| self.signer.seal_queued(&self.wiki, MAIN, &act, &p.token))
            .expect("sealed");
        for (path, bytes) in kept {
            std::fs::write(path, bytes).unwrap();
        }
        assert_eq!(
            self.queue_count(),
            u64::MAX,
            "`status --format json` refuses over the state a killed seal leaves"
        );
        assert!(self.cleanup_is_pending());
        self.prikk_json(&["log", "--ref", MAIN, "--limit", "1", "--format", "json"])
            .pointer("/blocks/0/block_id")
            .and_then(serde_json::Value::as_str)
            .unwrap()
            .to_owned()
    }

    fn cleanup_is_pending(&self) -> bool {
        self.witnesses
            .doctor(&self.wiki)
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.is_cleanup_pending())
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
        f.publish_op(&p),
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
            f.signer.complete_publication(&f.wiki, MAIN, a, &p.token),
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
    // Lent and recorded, but the act names only the discard: the other four refuse.
    let rs = run(&narrow);
    assert_eq!(nth(&rs, 0), &Err(RecoverError::NotRecorded));
    assert_eq!(nth(&rs, 1), &Err(RecoverError::NotRecorded));
    assert_ne!(
        nth(&rs, 2),
        &Err(RecoverError::NotRecorded),
        "the discard is named"
    );
    assert_eq!(nth(&rs, 3), &Err(RecoverError::NotRecorded));
    assert_eq!(nth(&rs, 4), &Err(RecoverError::NotRecorded));
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

#[test]
fn an_empty_lock_file_is_unreadable_to_recovery_and_the_runbooks_step_clears_exactly_that_file() {
    // Handoff 004, P15 / ruling 1: an empty lock file (a process killed the instant prikk created it) stays
    // fail-closed, and the operator's way out is in docs/RECOVERY.md. The commands there are run as written.
    let Some(f) = fixture("emptylock") else {
        return;
    };
    let runbook = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RECOVERY.md"),
    )
    .unwrap()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
    let lock = f.repo.join(".prikk/active/default/active.lock");
    std::fs::write(&lock, "").unwrap();
    // Another lock that is somebody's, to show the step clears one file and not the others.
    let other = plant_lock(
        &f.repo,
        "containers/objects.lock",
        2_147_483_000,
        "container:object-store",
    );
    let listed = |f: &Fixture| {
        let out = Command::new(&f.prikk)
            .arg("unlock")
            .current_dir(&f.repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    // What recovery's reader makes of it, and what the runbook says it will see.
    let err = f.witnesses.locks(&f.wiki).unwrap_err().to_string();
    assert!(err.contains("a recorded pid is not a number"), "{err}");
    for said in [
        "a recorded pid is not a number",
        "recorded pid: (unparseable)",
        "prikk unlock --lock <that path>",
        "`no locks currently held`",
    ] {
        assert!(runbook.contains(said), "the runbook says `{said}`");
    }
    let before = listed(&f);
    assert!(before.contains("recorded pid: (unparseable)"), "{before}");
    assert!(before.contains(&lock.display().to_string()), "{before}");
    assert_eq!(std::fs::metadata(&lock).unwrap().len(), 0);
    // The step, as written (`--yes` is the question answered): `prikk unlock --lock <that path>`.
    let cleared = Command::new(&f.prikk)
        .args(["unlock", "--lock"])
        .arg(&lock)
        .arg("--yes")
        .current_dir(&f.repo)
        .output()
        .unwrap();
    assert!(cleared.status.success(), "{cleared:?}");
    assert!(!lock.exists(), "that file is gone");
    assert!(Path::new(&other).exists(), "and only that file");
    // After: the reader reads (the other lock is a real one, of a kind recovery knows), and the history is untouched.
    let held = f.witnesses.locks(&f.wiki).expect("the listing is read");
    assert_eq!(held.len(), 1);
    assert_eq!(nth(&held, 0).path, other);
    assert_eq!(f.blocks(), 1);
    assert!(f.verify_ok());
    std::fs::remove_file(&other).unwrap();
    assert!(listed(&f).contains("no locks currently held"));
}

#[test]
fn a_publication_a_killed_seal_left_is_finished_with_one_more_seal_and_no_second_block() {
    let Some(f) = fixture("publish") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    let block = f.publish_and_leave_the_queue(&p);
    let blocks = f.blocks();
    assert_eq!(blocks, 2, "the first block and the published one");
    let act = f.act(vec![f.publish_op(&p)]);
    f.leased(|| f.signer.complete_publication(&f.wiki, MAIN, &act, &p.token))
        .expect("finished");
    // No block was made; the queue is drained; prikk reads the repository again and is content with it.
    assert_eq!(f.blocks(), blocks);
    let log = f.prikk_json(&["log", "--ref", MAIN, "--limit", "1", "--format", "json"]);
    assert_eq!(
        log.pointer("/blocks/0/block_id")
            .and_then(serde_json::Value::as_str),
        Some(block.as_str())
    );
    assert_eq!(f.queue_count(), 0);
    assert!(!f.cleanup_is_pending());
    assert!(f.verify_ok());
    assert!(f.worktree_clean());
    assert_eq!(
        std::fs::read_to_string(f.repo.join("pages/Home.md")).unwrap(),
        HOME_EDITED
    );
    // Once is once: with nothing pending, the operation refuses (and does not seal an empty queue).
    let again = f.leased(|| f.signer.complete_publication(&f.wiki, MAIN, &act, &p.token));
    assert!(
        matches!(again, Err(RecoverError::DoesNotMatch(_))),
        "{again:?}"
    );
    assert_eq!(f.blocks(), blocks);
}

#[test]
fn a_published_block_with_renames_and_a_removal_is_proven_against_the_recorded_digest() {
    let Some(f) = fixture("publish-renames") else {
        return;
    };
    // The block's operations are read back (`show`) and its files from the tip, with the renames in any order.
    let p = f.prepare(
        vec![
            file("pages/Edit.md", None),
            file("pages/Home.md", Some(HOME_EDITED)),
        ],
        vec![
            ("pages/Old C.md", "pages/New Z.md"),
            ("pages/Old A.md", "pages/New Y.md"),
        ],
    );
    f.publish_and_leave_the_queue(&p);
    let act = f.act(vec![f.publish_op(&p)]);
    f.leased(|| f.signer.complete_publication(&f.wiki, MAIN, &act, &p.token))
        .expect("finished");
    assert_eq!(f.queue_count(), 0);
    assert!(f.verify_ok());
    assert!(f.worktree_clean());
    assert!(!f.repo.join("pages/Edit.md").exists() && f.repo.join("pages/New Z.md").is_file());
}

#[test]
fn a_publication_that_is_not_the_recorded_change_is_not_finished() {
    let Some(f) = fixture("publish-mismatch") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.publish_and_leave_the_queue(&p);
    let ledger_entry = |token: &AcceptanceToken| {
        f.ledger
            .pending
            .lock()
            .unwrap()
            .get(token.as_str())
            .cloned()
            .unwrap()
    };
    let try_it = |token: &AcceptanceToken| {
        let act = f.act(vec![RecoveryOp::CompletePublication {
            acceptance: token.as_str().to_owned(),
            view: MAIN.to_owned(),
        }]);
        f.leased(|| f.signer.complete_publication(&f.wiki, MAIN, &act, token))
    };
    let refused = |r: Result<(), RecoverError>, what: &str| {
        assert!(
            matches!(&r, Err(RecoverError::DoesNotMatch(why)) if why.contains(what)),
            "{what}: {r:?}"
        );
        // Nothing was done: the cleanup is still pending, and the history is as the kill left it.
        assert!(f.cleanup_is_pending(), "{what}");
        assert_eq!(f.blocks(), 2, "{what}");
    };
    // Another change's message.
    let other = f.prepare(
        vec![file(
            "pages/Home.md",
            Some("---\ntitle: Home\n---\n\nOther.\n"),
        )],
        vec![],
    );
    refused(try_it(&other.token), "message");
    // The published message with other files: the record's digest is not the block's.
    let mut liar = ledger_entry(&p.token);
    liar.change_digest = other.change.digest();
    let liar_token = AcceptanceToken::new("acceptance-liar");
    f.ledger
        .pending
        .lock()
        .unwrap()
        .insert(liar_token.as_str().to_owned(), liar);
    refused(try_it(&liar_token), "files");
    // The published message and files, but the change was made on another block than the one it sits on.
    let mut elsewhere = ledger_entry(&p.token);
    elsewhere.pinned_tip = BlockId::new(&"a".repeat(64)).unwrap();
    let elsewhere_token = AcceptanceToken::new("acceptance-elsewhere");
    f.ledger
        .pending
        .lock()
        .unwrap()
        .insert(elsewhere_token.as_str().to_owned(), elsewhere);
    refused(try_it(&elsewhere_token), "made on");
    // Another wiki's or another view's record is not this one's.
    let mut foreign = ledger_entry(&p.token);
    foreign.wiki = WikiId::new();
    let foreign_token = AcceptanceToken::new("acceptance-foreign");
    f.ledger
        .pending
        .lock()
        .unwrap()
        .insert(foreign_token.as_str().to_owned(), foreign);
    assert_eq!(try_it(&foreign_token), Err(RecoverError::NotRecorded));
    // And the record that is right finishes it.
    try_it(&p.token).expect("the right record");
    assert!(!f.cleanup_is_pending());
    assert_eq!(f.blocks(), 2);
}

// ---- the publication a killed seal left: the queue is read, the doctor is filtered, the base is every record's
// (handoff 004, Q2, Q6, Q7)

impl Fixture {
    /// What `prikk status` prints now (the real one, plain text).
    fn status_text(&self) -> String {
        let out = Command::new(&self.real)
            .args(["status"])
            .current_dir(&self.repo)
            .output()
            .expect("prikk runs");
        assert!(out.status.success(), "the plain text reads in this state");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn say(&self, name: &str, text: &str) {
        std::fs::write(self.ctl.join(name), text).unwrap();
    }

    fn unsay(&self, name: &str) {
        let _ = std::fs::remove_file(self.ctl.join(name));
    }

    /// One try at finishing the publication of `p`.
    fn finish_publication(&self, p: &Prepared) -> Result<(), RecoverError> {
        let act = self.act(vec![self.publish_op(p)]);
        self.leased(|| {
            self.signer
                .complete_publication(&self.wiki, MAIN, &act, &p.token)
        })
    }
}

#[test]
fn the_queue_is_read_from_the_plain_text_of_status_and_only_the_published_patch_may_be_in_it() {
    let Some(f) = fixture("publish-queue") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.publish_and_leave_the_queue(&p);
    let real = f.status_text();
    // The measured shape: the plain text reads where the JSON refuses.
    assert!(
        real.contains("queued patches: 1 targeting heads/main"),
        "{real}"
    );
    assert!(real.contains("active WAL records: 1"), "{real}");
    let blocks = f.blocks();
    // Each of these is a queue that is not exactly the published patch: refused, and nothing is done (the cleanup
    // is still pending, no block is made, the queue is as it was).
    let refused = |what: &str, text: String| {
        f.say("say-status-text", &text);
        let r = f.finish_publication(&p);
        assert!(
            matches!(&r, Err(RecoverError::DoesNotMatch(why)) if why.contains("queue")),
            "{what}: {r:?}"
        );
        assert!(f.cleanup_is_pending(), "{what}");
        assert_eq!(f.blocks(), blocks, "{what}");
    };
    refused(
        "a second patch queued",
        real.replace("queued patches: 1 targeting", "queued patches: 2 targeting")
            .replace("active WAL records: 1", "active WAL records: 2"),
    );
    refused(
        "a second patch queued, the count of records alone",
        real.replace("active WAL records: 1", "active WAL records: 2"),
    );
    refused(
        "nothing queued, which is not the state doctor names",
        real.replace(
            "queued patches: 1 targeting heads/main",
            "queued patches: 0",
        ),
    );
    refused(
        "queued for another view",
        real.replace("targeting heads/main", "targeting heads/other"),
    );
    refused(
        "a torn byte",
        real.replace(
            "trailing partial WAL bytes: 0",
            "trailing partial WAL bytes: 5",
        ),
    );
    // A text that cannot be read as a queue is not guessed at: a failure to read, not a mismatch.
    for (what, text) in [
        (
            "no queue line",
            real.replace("queued patches: 1 targeting heads/main\n", ""),
        ),
        (
            "a queue line twice",
            format!("{real}queued patches: 1 targeting heads/main\n"),
        ),
        (
            "a count that is not a number",
            real.replace("queued patches: 1", "queued patches: many"),
        ),
    ] {
        f.say("say-status-text", &text);
        let r = f.finish_publication(&p);
        assert!(
            matches!(&r, Err(RecoverError::Unavailable(why)) if why.contains("queue could not be read")),
            "{what}: {r:?}"
        );
        assert!(f.cleanup_is_pending(), "{what}");
        assert_eq!(f.blocks(), blocks, "{what}");
    }
    // And a `status` that cannot be asked at all.
    f.unsay("say-status-text");
    f.say("fail-status", "");
    assert!(matches!(
        f.finish_publication(&p),
        Err(RecoverError::Refused(_) | RecoverError::Unavailable(_))
    ));
    f.unsay("fail-status");
    assert!(f.cleanup_is_pending());
    // The state as prikk prints it finishes.
    f.finish_publication(&p)
        .expect("the queue is the published patch");
    assert!(!f.cleanup_is_pending());
    assert_eq!(f.blocks(), blocks);
}

/// A retirement that is recorded, and a publication a killed seal left: the state W7 is about.
fn retired_and_killed(tag: &str) -> Option<(Fixture, Prepared, String)> {
    let f = fixture(tag)?;
    f.signer
        .rotate_keys(
            &inklkunn_signer::KeyAuthority::for_tests(),
            &f.repo,
            inklkunn_signer::OldKey::Retire,
        )
        .expect("a retirement");
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    let block = f.leave_the_queue_after_publishing(&p);
    Some((f, p, block))
}

#[test]
fn a_cleanup_beside_the_findings_a_retirement_recorded_is_finished_and_the_plain_rule_holds_after()
{
    // W7, with real prikk. While the cleanup is pending `verify` fails by two conditions, so the plain question (the
    // retirement's record excuses a failure by publication trust alone) says no. The question beside a cleanup says yes
    // when every publication-trust finding is in a recorded set; the step is then taken; and afterwards the plain
    // question says yes again, with nothing changed but the queue.
    let Some((f, p, _block)) = retired_and_killed("w7-publish-retired") else {
        return;
    };
    let blocks = f.blocks();
    // The raw `doctor` holds the cleanup and the retired key's findings.
    let raw = f.witnesses.doctor(&f.wiki).unwrap();
    assert!(!raw.is_only_cleanup_pending());
    assert!(raw.diagnostics.iter().any(|d| d.is_cleanup_pending()));
    // The failure is exactly the two conditions.
    let json = f.prikk_json(&["verify", "--format", "json"]);
    let mut ids: Vec<&str> = json
        .pointer("/verdict/failed_conditions")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .filter_map(|c| c.get("id").and_then(serde_json::Value::as_str))
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["blocking-ref-publication", "publication-trust"]);
    // The plain question says no; the one beside a cleanup says yes, and names what `doctor` names.
    assert!(
        !matches!(
            Recover::verify_after_retirement(&f.signer, &f.wiki),
            Ok(Some(_))
        ),
        "two conditions fail: the plain rule does not apply (it refuses the text, Q3)"
    );
    let excuse = Recover::verify_beside_cleanup(&f.signer, &f.wiki)
        .expect("asked")
        .expect("every finding is a recorded one");
    assert_eq!(
        (
            excuse.blocks.len(),
            excuse.ref_states.len(),
            excuse.ref_updates.len()
        ),
        (1, 1, 1)
    );
    assert!(
        excuse
            .note
            .contains("only publication trust and the cleanup of a killed seal fail")
    );
    // The step is taken as real prikk answers.
    f.finish_publication(&p).expect(
        "the retired key's findings are the retirement's, and the cleanup is the only thing else",
    );
    assert!(!f.cleanup_is_pending());
    assert_eq!(f.blocks(), blocks);
    assert_eq!(f.queue_count(), 0);
    // After the step the unwidened rule holds: publication trust alone, all of it recorded.
    assert!(
        Recover::verify_after_retirement(&f.signer, &f.wiki)
            .expect("asked")
            .is_some()
    );
    assert_eq!(Recover::verify_beside_cleanup(&f.signer, &f.wiki), Ok(None));
}

#[test]
fn a_cleanup_is_not_finished_beside_anything_a_retirement_did_not_record() {
    // W7: the question beside a cleanup is `judge`'s rule for the findings and exactly two conditions. Each way it can be
    // wider is made with the stand-in (the real prikk is asked for the rest): a finding no retirement recorded, a third
    // failed condition, the cleanup named twice, a run that did not go to the end. The step is refused, nothing is
    // changed, and it is finished once real prikk answers again.
    let Some((f, p, _block)) = retired_and_killed("w7-not-wider") else {
        return;
    };
    let blocks = f.blocks();
    let real_json = f.prikk_json(&["verify", "--format", "json"]);
    let real_text = String::from_utf8_lossy(
        &Command::new(&f.real)
            .args(["verify"])
            .current_dir(&f.repo)
            .output()
            .unwrap()
            .stdout,
    )
    .into_owned();
    let with_condition = |extra: &str| {
        let mut json = real_json.clone();
        json.pointer_mut("/verdict/failed_conditions")
            .and_then(serde_json::Value::as_array_mut)
            .unwrap()
            .push(serde_json::json!({"id": extra, "message": "m"}));
        json.to_string()
    };
    let stranger = "9".repeat(64);
    let cleanup_line = real_text
        .lines()
        .find(|l| l.starts_with("ref-publication [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]"))
        .unwrap()
        .to_owned();
    let mut halted = real_json.clone();
    if let Some(status) = halted.pointer_mut("/stages/0/status") {
        *status = "halted".into();
    }
    for (what, json, text) in [
        (
            "a block nobody recorded",
            real_json.to_string(),
            real_text.replace("publication trust issues: 3", "publication trust issues: 4")
                + &format!(
                    "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block {stranger} has no trusted MAINTAINER signature\n"
                ),
        ),
        (
            "a third failed condition",
            with_condition("object-integrity"),
            real_text.clone(),
        ),
        (
            "the cleanup named twice",
            real_json.to_string(),
            real_text.replace(&cleanup_line, &format!("{cleanup_line}\n{cleanup_line}")),
        ),
        (
            "a run that did not go to the end",
            halted.to_string(),
            real_text.clone(),
        ),
    ] {
        f.say("say-verify-json", &json);
        f.say("say-verify-text", &text);
        assert_eq!(
            Recover::verify_beside_cleanup(&f.signer, &f.wiki)
                .ok()
                .flatten()
                .map(|e| e.keys),
            None,
            "{what}: the signer does not say yes"
        );
        let r = f.finish_publication(&p);
        assert!(r.is_err(), "{what}: {r:?}");
        assert!(f.cleanup_is_pending(), "{what}");
        assert_eq!(f.blocks(), blocks, "{what}");
        f.unsay("say-verify-json");
        f.unsay("say-verify-text");
    }
    f.finish_publication(&p)
        .expect("real prikk answers again, and every finding is a recorded one");
    assert!(!f.cleanup_is_pending());
}

#[test]
fn a_cleanup_beside_a_wiki_with_no_recorded_retirement_is_not_offered() {
    // W7 is for a *recorded* retirement. A wiki that never retired a key has nothing to excuse and the step is as it was.
    let Some(f) = fixture("w7-none") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.publish_and_leave_the_queue(&p);
    assert_eq!(Recover::verify_beside_cleanup(&f.signer, &f.wiki), Ok(None));
    f.finish_publication(&p)
        .expect("the cleanup alone, as before");
    assert!(!f.cleanup_is_pending());
}

#[test]
fn a_queued_patch_is_not_sealed_while_a_retirement_is_unfinished_and_is_after_it_is_finished() {
    // W1 for recovery's `seal_queued`: it makes a block, so it is paused like an acceptance; nothing is touched, and the
    // refusal is one sentence naming the command. After the operator finishes the retirement the very same step seals it.
    let Some(f) = fixture("w1-seal-queued") else {
        return;
    };
    let p = f.prepare(vec![file("pages/Home.md", Some(HOME_EDITED))], vec![]);
    f.queue(&p);
    let blocks = f.blocks();
    f.say("fail-trust-remove", "");
    let rotation = f
        .signer
        .rotate_keys(
            &inklkunn_signer::KeyAuthority::for_tests(),
            &f.repo,
            inklkunn_signer::OldKey::Retire,
        )
        .expect("the rotation happened");
    f.unsay("fail-trust-remove");
    assert!(
        matches!(
            &rotation.old_key,
            inklkunn_signer::OldKeyOutcome::RetireFailed(_)
        ),
        "{rotation:?}"
    );
    let command = inklkunn_signer::finish_retirement_command(&f.wiki);
    let act = f.act(vec![f.seal_op(&p)]);
    let r = f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token));
    assert!(
        matches!(&r, Err(RecoverError::RetirementUnfinished(why))
            if why.contains(&command) && why.contains("nothing was written")),
        "{r:?}"
    );
    assert_eq!(f.queue_count(), 1, "the patch is still queued");
    assert_eq!(f.blocks(), blocks);
    f.signer
        .finish_retirement(&inklkunn_signer::KeyAuthority::for_tests(), &f.repo)
        .expect("finished");
    f.leased(|| f.signer.seal_queued(&f.wiki, MAIN, &act, &p.token))
        .expect("sealed after the retirement is finished");
    assert_eq!(f.blocks(), blocks + 1);
    assert_eq!(f.queue_count(), 0);
}
