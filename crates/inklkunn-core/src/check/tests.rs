#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, Materialized, Point, Result as PrikkResult,
    Stage, Storage, Tree, Verdict, Verification as Report, Views,
};
use inklkunn_signer::AcceptanceToken;
use inklkunn_signer::recover::text::Discarded;
use inklkunn_signer::recover::{RecoverError, RecoveryToken, RetirementExcuse};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, Slug, WikiId};

use super::*;

/// A storage that answers `verify` with what it was given and nothing else. Given a gate, `verify` says it has
/// begun and then waits to be let go, so a test can hold a check open.
/// `verify` says it has begun (the first) and waits to be let go (the second).
type Gate = (Mutex<Sender<()>>, Mutex<Receiver<()>>);

struct Canned {
    answers: Mutex<VecDeque<PrikkResult<Report>>>,
    calls: AtomicUsize,
    gate: Option<Gate>,
}

impl Canned {
    fn of(answers: Vec<PrikkResult<Report>>) -> Canned {
        Canned {
            answers: Mutex::new(answers.into()),
            calls: AtomicUsize::new(0),
            gate: None,
        }
    }
}

fn nope<T>(verb: &'static str) -> PrikkResult<T> {
    Err(StorageError::Driver {
        verb,
        detail: "not in this double".to_owned(),
    })
}

impl Storage for Canned {
    fn views(&self, _: &Path) -> PrikkResult<Views> {
        nope("branch")
    }
    fn all_views(&self, _: &Path) -> PrikkResult<Views> {
        nope("branch")
    }
    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> PrikkResult<Tree> {
        nope("tree")
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> PrikkResult<FileMeta> {
        nope("cat")
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> PrikkResult<Vec<u8>> {
        nope("cat")
    }
    fn history(&self, _: &Path, _: &Point, _: Option<u64>) -> PrikkResult<History> {
        nope("log")
    }
    fn change(&self, _: &Path, _: &str) -> PrikkResult<Change> {
        nope("show")
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> PrikkResult<Diff> {
        nope("diff")
    }
    fn plan(&self, _: &Path, _: &Point) -> PrikkResult<Materialized> {
        nope("extract")
    }
    fn materialize(
        &self,
        _: &Path,
        _: &Point,
        _: &Path,
        _: &str,
        _: ExtractBounds,
    ) -> PrikkResult<Materialized> {
        nope("extract")
    }
    fn verify(&self, _: &Path) -> PrikkResult<Report> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, release)) = &self.gate {
            entered.lock().unwrap().send(()).unwrap();
            release.lock().unwrap().recv().unwrap();
        }
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("one answer per check")
    }
}

fn report(ok: bool, stages: &[(&str, &str)]) -> Report {
    Report {
        schema_version: "verify-report-v1".to_owned(),
        verdict: Verdict {
            ok,
            failed_conditions: Vec::new(),
        },
        stages: stages
            .iter()
            .map(|(stage, status)| Stage {
                stage: (*stage).to_owned(),
                status: (*status).to_owned(),
                message: None,
                blocked_by: None,
            })
            .collect(),
    }
}

fn new_wiki(store: &MemoryStore, slug: &str) -> WikiRecord {
    store
        .create(NewWiki {
            slug: Slug::new(slug).unwrap(),
            display_name: DisplayName::new(slug).unwrap(),
            default_view: None,
            default_license: None,
            languages: vec![],
            visibility: inklkunn_store::Visibility::Public,
            policy_bootstrap: None,
        })
        .unwrap()
}

const NOW: i64 = 1_700_000_000;

fn verifier(store: &Arc<MemoryStore>, storage: Arc<Canned>) -> Verifier {
    Verifier::new(storage, store.clone(), RootLayout::new("/nowhere")).with_clock(|| NOW)
}

fn run(answer: PrikkResult<Report>) -> (Checked, WikiRecord, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "handbook");
    let checked = verifier(&store, Arc::new(Canned::of(vec![answer]))).check(&wiki);
    let after = store.get(&wiki.id).unwrap();
    (checked, after, store)
}

#[test]
fn a_clean_run_is_verified_and_recorded() {
    let (c, after, _) = run(Ok(report(true, &[("objects", "evaluated")])));
    assert_eq!(c.outcome, CheckOutcome::Verified);
    assert!(c.recorded && c.operator.is_none());
    assert_eq!(after.verification.state, VerificationState::Verified);
    assert_eq!(after.verification.at, Some(1_700_000_000));
}

#[test]
fn a_finding_is_a_failure_that_names_the_stages_that_did_not_come_out_well() {
    let (c, after, _) = run(Ok(report(
        false,
        &[
            ("objects", "failed"),
            ("refs", "evaluated"),
            ("ref-update-schema-trust", "not_evaluated"),
        ],
    )));
    assert_eq!(
        c.outcome,
        CheckOutcome::Failed {
            stages: vec![
                StageFinding {
                    stage: "objects".into(),
                    status: "failed".into()
                },
                StageFinding {
                    stage: "ref-update-schema-trust".into(),
                    status: "not_evaluated".into()
                },
            ],
            explained_by_retirement: Vec::new(),
        }
    );
    assert_eq!(after.verification.state, VerificationState::Failed);
}

#[test]
fn a_check_that_could_not_run_is_never_a_verdict() {
    use StorageError as E;
    let cases = [
        (
            E::Unavailable {
                verb: "verify",
                reason: Unavailable::Busy,
            },
            NotChecked::Busy,
        ),
        (
            E::Unavailable {
                verb: "verify",
                reason: Unavailable::CeilingExceeded { bytes: 1 },
            },
            NotChecked::Limit,
        ),
        (
            E::Unavailable {
                verb: "verify",
                reason: Unavailable::TimedOut { seconds: 300 },
            },
            NotChecked::Limit,
        ),
        (
            E::TooLarge {
                verb: "verify",
                needed: 2,
                pool: 1,
            },
            NotChecked::Limit,
        ),
        (
            E::Unavailable {
                verb: "verify",
                reason: Unavailable::Spawn("no prikk".into()),
            },
            NotChecked::Unavailable,
        ),
        (
            E::Damaged {
                verb: "verify",
                detail: "garbage".into(),
            },
            NotChecked::Unavailable,
        ),
    ];
    for (err, expected) in cases {
        let (c, after, _) = run(Err(err));
        assert_eq!(c.outcome, CheckOutcome::CouldNotBeChecked(expected));
        assert!(c.operator.is_some(), "the operator is told why");
        assert_eq!(
            after.verification,
            Verification::default(),
            "no verdict is made up"
        );
        assert_eq!(
            after.last_attempt.map(|a| a.at),
            Some(NOW),
            "the attempt is recorded, apart"
        );
    }
}

fn limit() -> StorageError {
    StorageError::Unavailable {
        verb: "verify",
        reason: Unavailable::CeilingExceeded { bytes: 1 },
    }
}

fn busy() -> StorageError {
    StorageError::Unavailable {
        verb: "verify",
        reason: Unavailable::Busy,
    }
}

fn timeout() -> StorageError {
    StorageError::Unavailable {
        verb: "verify",
        reason: Unavailable::TimedOut { seconds: 300 },
    }
}

/// R1: whatever stops a check, the wiki whose last verdict was *failed* stays failed.
#[test]
fn a_busy_a_limit_and_a_timeout_each_leave_a_failed_record_failed() {
    for (err, why) in [
        (busy(), NotRun::Busy),
        (limit(), NotRun::Limit),
        (timeout(), NotRun::Limit),
    ] {
        let store = Arc::new(MemoryStore::new());
        let wiki = new_wiki(&store, "failing");
        let failed = Verification {
            state: VerificationState::Failed,
            at: Some(NOW - 10_000),
        };
        store.set_verification(&wiki.id, failed).unwrap();
        let wiki = store.get(&wiki.id).unwrap();
        let c = verifier(&store, Arc::new(Canned::of(vec![Err(err)]))).check(&wiki);
        assert!(matches!(c.outcome, CheckOutcome::CouldNotBeChecked(_)));
        let after = store.get(&wiki.id).unwrap();
        assert_eq!(after.verification, failed, "{why:?} replaced the verdict");
        assert_eq!(after.last_attempt, Some(Attempt { at: NOW, why }));
    }
}

#[test]
fn a_verdict_after_an_attempt_clears_the_attempt() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "recovering");
    let c = verifier(&store, Arc::new(Canned::of(vec![Err(limit())]))).check(&wiki);
    assert!(c.recorded);
    assert!(store.get(&wiki.id).unwrap().last_attempt.is_some());
    // Later: the check runs and finds nothing.
    store
        .record_attempt(
            &wiki.id,
            Attempt {
                at: NOW - 3600,
                why: NotRun::Limit,
            },
        )
        .unwrap();
    let c = verifier(
        &store,
        Arc::new(Canned::of(vec![Ok(report(
            true,
            &[("objects", "evaluated")],
        ))])),
    )
    .check(&wiki);
    assert_eq!(c.outcome, CheckOutcome::Verified);
    let after = store.get(&wiki.id).unwrap();
    assert_eq!(after.verification.state, VerificationState::Verified);
    assert_eq!(after.last_attempt, None);
}

#[test]
fn a_second_request_for_a_wiki_whose_check_is_running_is_told_so_and_verify_runs_once() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "slow");
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let storage = Arc::new(Canned {
        answers: Mutex::new(vec![Ok(report(true, &[("objects", "evaluated")]))].into()),
        calls: AtomicUsize::new(0),
        gate: Some((Mutex::new(entered_tx), Mutex::new(release_rx))),
    });
    let v = Arc::new(verifier(&store, storage.clone()));
    let first = {
        let (v, wiki) = (v.clone(), wiki.clone());
        std::thread::spawn(move || v.check(&wiki))
    };
    entered_rx.recv().unwrap(); // the first check is inside `verify`
    for _ in 0..3 {
        let second = v.check(&wiki);
        assert_eq!(second.outcome, CheckOutcome::Declined(Declined::Running));
        assert!(!second.ran());
    }
    release_tx.send(()).unwrap();
    let first = first.join().unwrap();
    assert_eq!(first.outcome, CheckOutcome::Verified);
    assert_eq!(storage.calls.load(Ordering::SeqCst), 1);
    // The claim is given back; the next request is only held back by the minimum interval.
    assert_eq!(
        v.check(&wiki).outcome,
        CheckOutcome::Declined(Declined::Recent)
    );
}

#[test]
fn no_more_checks_than_the_cap_run_at_once() {
    let store = Arc::new(MemoryStore::new());
    let (a, b) = (new_wiki(&store, "alpha"), new_wiki(&store, "beta"));
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let storage = Arc::new(Canned {
        answers: Mutex::new(vec![Ok(report(true, &[("objects", "evaluated")]))].into()),
        calls: AtomicUsize::new(0),
        gate: Some((Mutex::new(entered_tx), Mutex::new(release_rx))),
    });
    let v = Arc::new(verifier(&store, storage.clone()).with_concurrency(1));
    let first = {
        let (v, a) = (v.clone(), a.clone());
        std::thread::spawn(move || v.check(&a))
    };
    entered_rx.recv().unwrap();
    let crowded = v.check(&b);
    assert_eq!(crowded.outcome, CheckOutcome::Declined(Declined::Crowded));
    assert_eq!(
        store.get(&b.id).unwrap().last_attempt,
        None,
        "a request turned away made no attempt"
    );
    release_tx.send(()).unwrap();
    first.join().unwrap();
    assert_eq!(storage.calls.load(Ordering::SeqCst), 1);
}

static CLOCK: AtomicI64 = AtomicI64::new(NOW);

#[test]
fn a_check_is_not_repeated_within_the_minimum_interval_and_is_after_it() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "recent");
    let storage = Arc::new(Canned::of(vec![
        Ok(report(true, &[("objects", "evaluated")])),
        Ok(report(true, &[("objects", "evaluated")])),
    ]));
    let v = Verifier::new(storage.clone(), store.clone(), RootLayout::new("/nowhere"))
        .with_clock(|| CLOCK.load(Ordering::SeqCst));
    assert_eq!(v.check(&wiki).outcome, CheckOutcome::Verified);
    CLOCK.store(NOW + MIN_RECHECK_SECS - 1, Ordering::SeqCst);
    assert_eq!(
        v.check(&wiki).outcome,
        CheckOutcome::Declined(Declined::Recent)
    );
    CLOCK.store(NOW + MIN_RECHECK_SECS, Ordering::SeqCst);
    assert_eq!(v.check(&wiki).outcome, CheckOutcome::Verified);
    assert_eq!(storage.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_recorded_time_in_the_future_counts_as_recent() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "skewed");
    store
        .set_verification(
            &wiki.id,
            Verification {
                state: VerificationState::Failed,
                at: Some(NOW + 86_400),
            },
        )
        .unwrap();
    let wiki = store.get(&wiki.id).unwrap();
    let storage = Arc::new(Canned::of(vec![]));
    let c = verifier(&store, storage.clone()).check(&wiki);
    assert_eq!(c.outcome, CheckOutcome::Declined(Declined::Recent));
    assert_eq!(storage.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_attempt_the_gate_turned_away_does_not_hold_back_the_next_check_but_a_stopped_one_does() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "gate");
    let attempt = |why| Attempt { at: NOW - 1, why };
    store
        .record_attempt(&wiki.id, attempt(NotRun::Busy))
        .unwrap();
    let storage = Arc::new(Canned::of(vec![Ok(report(
        true,
        &[("objects", "evaluated")],
    ))]));
    let c = verifier(&store, storage.clone()).check(&store.get(&wiki.id).unwrap());
    assert_eq!(c.outcome, CheckOutcome::Verified, "busy cost nothing");

    let wiki = new_wiki(&store, "heavy");
    store
        .record_attempt(&wiki.id, attempt(NotRun::Limit))
        .unwrap();
    let c = verifier(&store, Arc::new(Canned::of(vec![]))).check(&store.get(&wiki.id).unwrap());
    assert_eq!(c.outcome, CheckOutcome::Declined(Declined::Recent));
}

// ---- a failure a key retirement recorded (handoff 004, P2′)

type Excuse = Result<Option<RetirementExcuse>, RecoverError>;

/// A signer that answers the one question of a retirement and nothing else.
struct Retirements(Excuse);

impl Recover for Retirements {
    fn verify_after_retirement(&self, _: &WikiId) -> Excuse {
        self.0.clone()
    }
    fn seal_queued(
        &self,
        _: &WikiId,
        _: &str,
        _: &RecoveryToken,
        _: &AcceptanceToken,
    ) -> Result<(), RecoverError> {
        Err(RecoverError::NoLease)
    }
    fn discard_damaged(&self, _: &WikiId, _: &RecoveryToken) -> Result<Discarded, RecoverError> {
        Err(RecoverError::NoLease)
    }
    fn clear_locks(
        &self,
        _: &WikiId,
        _: &RecoveryToken,
        _: &[String],
    ) -> Result<Vec<String>, RecoverError> {
        Err(RecoverError::NoLease)
    }
    fn restore_worktree(
        &self,
        _: &WikiId,
        _: &str,
        _: &RecoveryToken,
    ) -> Result<usize, RecoverError> {
        Err(RecoverError::NoLease)
    }
}

fn excuse(keys: &[&str]) -> RetirementExcuse {
    RetirementExcuse {
        note: "note".to_owned(),
        keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        blocks: Vec::new(),
        ref_states: Vec::new(),
        ref_updates: Vec::new(),
    }
}

fn failed_with(answer: Excuse) -> (CheckOutcome, VerificationState) {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "handbook");
    let v = verifier(
        &store,
        Arc::new(Canned::of(vec![Ok(report(false, &[("refs", "failed")]))])),
    );
    v.explain_retirements_with(Arc::new(Retirements(answer)));
    let checked = v.check(&wiki);
    (
        checked.outcome,
        store.get(&wiki.id).unwrap().verification.state,
    )
}

#[test]
fn a_failure_a_retirement_recorded_carries_the_key_and_leaves_the_state_failed() {
    let (outcome, state) = failed_with(Ok(Some(excuse(&["old-key"]))));
    let CheckOutcome::Failed {
        explained_by_retirement,
        ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(explained_by_retirement, ["old-key"]);
    // The badge is the record's state, and it is Failed, as DR-10 ruled.
    assert_eq!(state, VerificationState::Failed);
}

#[test]
fn a_failure_the_signer_does_not_explain_or_cannot_tell_is_reported_as_it_always_was() {
    for answer in [Ok(None), Err(RecoverError::NoLease)] {
        let (outcome, state) = failed_with(answer);
        assert!(
            matches!(&outcome, CheckOutcome::Failed { explained_by_retirement, .. } if explained_by_retirement.is_empty()),
            "{outcome:?}"
        );
        assert_eq!(state, VerificationState::Failed);
    }
    // No signer at all (a server that cannot sign): nothing is asked.
    let (c, _, _) = run(Ok(report(false, &[("refs", "failed")])));
    assert!(
        matches!(&c.outcome, CheckOutcome::Failed { explained_by_retirement, .. } if explained_by_retirement.is_empty())
    );
}

#[test]
fn a_clean_check_never_asks_the_signer() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "handbook");
    let v = verifier(
        &store,
        Arc::new(Canned::of(vec![Ok(report(
            true,
            &[("objects", "evaluated")],
        ))])),
    );
    v.explain_retirements_with(Arc::new(Retirements(Ok(Some(excuse(&["old-key"]))))));
    assert_eq!(v.check(&wiki).outcome, CheckOutcome::Verified);
}

// ---- the explanation is stored with the verdict (handoff 004, Q5)

static EXPLAIN_CLOCK: AtomicI64 = AtomicI64::new(NOW);

/// One check of `wiki` at a later time, with the signer answering `answer` and prikk answering `says`.
fn check_later(
    store: &Arc<MemoryStore>,
    wiki: &WikiRecord,
    says: PrikkResult<Report>,
    answer: Excuse,
) -> Checked {
    EXPLAIN_CLOCK.fetch_add(MIN_RECHECK_SECS * 2, Ordering::SeqCst);
    let v = Verifier::new(
        Arc::new(Canned::of(vec![says])),
        store.clone(),
        RootLayout::new("/nowhere"),
    )
    .with_clock(|| EXPLAIN_CLOCK.load(Ordering::SeqCst));
    v.explain_retirements_with(Arc::new(Retirements(answer)));
    v.check(wiki)
}

#[test]
fn the_explanation_is_stored_with_its_verdict_and_follows_it() {
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "handbook");
    let failed = || Ok(report(false, &[("refs", "failed")]));
    let explained = || Ok(Some(excuse(&["old-key", "older-key"])));
    // A failure a retirement recorded: the record says so, beside a verdict that is still *failed*.
    let c = check_later(&store, &wiki, failed(), explained());
    assert!(c.recorded);
    let now = store.get(&wiki.id).unwrap();
    assert_eq!(now.verification.state, VerificationState::Failed);
    assert_eq!(now.verification_explained_by, ["old-key", "older-key"]);
    // A check that could not run is an attempt: the verdict and what explains it stand (R1).
    let busy = Err(StorageError::Unavailable {
        verb: "verify",
        reason: Unavailable::Busy,
    });
    check_later(&store, &wiki, busy, explained());
    let now = store.get(&wiki.id).unwrap();
    assert!(now.last_attempt.is_some());
    assert_eq!(now.verification.state, VerificationState::Failed);
    assert_eq!(now.verification_explained_by, ["old-key", "older-key"]);
    // A later failure nothing explains replaces it: the explanation is the latest verdict's own.
    check_later(&store, &wiki, failed(), Ok(None));
    let now = store.get(&wiki.id).unwrap();
    assert_eq!(now.verification.state, VerificationState::Failed);
    assert!(now.verification_explained_by.is_empty());
    assert!(now.last_attempt.is_none());
    // ... and a failure that is explained again, then a check that passes, which clears it.
    check_later(&store, &wiki, failed(), explained());
    assert_eq!(
        store.get(&wiki.id).unwrap().verification_explained_by,
        ["old-key", "older-key"]
    );
    let c = check_later(
        &store,
        &wiki,
        Ok(report(true, &[("objects", "evaluated")])),
        explained(),
    );
    assert_eq!(c.outcome, CheckOutcome::Verified);
    let now = store.get(&wiki.id).unwrap();
    assert_eq!(now.verification.state, VerificationState::Verified);
    assert!(now.verification_explained_by.is_empty());
}

#[test]
fn an_explanation_the_store_will_not_keep_never_costs_the_verdict() {
    // Key ids as prikk accepts them are all the store takes; anything else is dropped, and the verdict is recorded.
    let store = Arc::new(MemoryStore::new());
    let wiki = new_wiki(&store, "handbook");
    let c = check_later(
        &store,
        &wiki,
        Ok(report(false, &[("refs", "failed")])),
        Ok(Some(excuse(&["not a key id"]))),
    );
    assert!(c.recorded);
    let now = store.get(&wiki.id).unwrap();
    assert_eq!(now.verification.state, VerificationState::Failed);
    assert!(now.verification_explained_by.is_empty());
}
