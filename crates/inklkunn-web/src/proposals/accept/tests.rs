#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;

use inklkunn_core::knowledge::check::Refusal;
use inklkunn_core::knowledge::{Fault, Rule};

use super::*;

#[test]
fn a_retirement_that_is_not_finished_is_a_page_of_its_own_and_the_log_keeps_the_command() {
    // W1: the signer's refusal reaches the acceptance as the word `retirement-unfinished`; the page says it in plain
    // words and never the sentence the operator reads, which goes to the log.
    let logged = Mutex::new(Vec::new());
    let log = |line: &str| logged.lock().unwrap().push(line.to_owned());
    let outcome = outcome_of(
        Err(AcceptFailure::Failed {
            acceptance: inklkunn_store::AcceptanceId::new(),
            word: accept_words::RETIREMENT_UNFINISHED,
            detail: "run `inklkunn wiki keys finish-retirement w`".to_owned(),
        }),
        &log,
    );
    assert_eq!(
        outcome,
        AcceptOutcome::Unchanged(Unchanged::RetirementUnfinished)
    );
    {
        let lines = logged.lock().unwrap();
        assert!(
            lines.iter().any(|l| l.contains("finish-retirement")),
            "{lines:?}"
        );
    }
    // Another word is not mistaken for it.
    let other = outcome_of(
        Err(AcceptFailure::Failed {
            acceptance: inklkunn_store::AcceptanceId::new(),
            word: accept_words::REFUSED,
            detail: String::new(),
        }),
        &log,
    );
    assert_eq!(other, AcceptOutcome::Unchanged(Unchanged::Refused));
}

#[test]
fn a_knowledge_file_refused_before_the_signer_is_a_page_of_its_own_and_the_log_names_the_file() {
    let logged = Mutex::new(Vec::new());
    let log = |line: &str| logged.lock().unwrap().push(line.to_owned());
    let refusal = Refusal::of("knowledge/stray.txt", Fault::whole(Rule::Path));
    let outcome = outcome_of(Err(AcceptFailure::KnowledgeInvalid(vec![refusal])), &log);
    assert!(matches!(
        outcome,
        AcceptOutcome::Unchanged(Unchanged::KnowledgeInvalid)
    ));
    let lines = logged.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines.first().unwrap().contains("knowledge/stray.txt"));
}
