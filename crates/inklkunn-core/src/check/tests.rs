#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::path::Path;
use std::sync::Mutex;

use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, Materialized, Point, Result as PrikkResult,
    Stage, Storage, Tree, Verdict, Verification as Report, Views,
};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, Slug};

use super::*;

/// A storage that answers `verify` with what it was given and nothing else.
struct Canned(Mutex<Option<PrikkResult<Report>>>);

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
        self.0.lock().unwrap().take().expect("one answer per check")
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

fn run(answer: PrikkResult<Report>) -> (Checked, WikiRecord, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let wiki = store
        .create(NewWiki {
            slug: Slug::new("handbook").unwrap(),
            display_name: DisplayName::new("Handbook").unwrap(),
            default_view: None,
            default_license: None,
            languages: vec![],
            visibility: inklkunn_store::Visibility::Public,
            policy_bootstrap: None,
        })
        .unwrap();
    let verifier = Verifier::new(
        Arc::new(Canned(Mutex::new(Some(answer)))),
        store.clone(),
        RootLayout::new("/nowhere"),
    )
    .with_clock(|| 1_700_000_000);
    let checked = verifier.check(&wiki);
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
            ]
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
        assert_eq!(after.verification.state, VerificationState::CouldNotCheck);
    }
}
