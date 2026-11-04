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
fn the_pause_sentence_does_not_promise_that_finishing_resumes_writes() {
    // Z8: with findings nothing traces, `finish-retirement` only refuses, so the sentence people whose proposal is
    // held read names the person who runs the wiki finishing *or giving up* a change to its keys, not a command or a
    // finish that resumes writes. English and the Japanese preview say the same.
    use crate::catalog::{EN, JA_DEF, Lang, T};
    for (lang, give_up) in [(EN, "gives up"), (Lang::from_def(&JA_DEF), "取りやめ")] {
        let t = T::new(lang);
        for key in [
            Key::ProblemRetirementUnfinishedDetail,
            Key::OutcomeNoteRetirementUnfinished,
        ] {
            let text = t.get(key);
            assert!(text.contains(give_up), "{text}");
            assert!(!text.contains("finish-retirement"), "{text}");
        }
    }
    let english = T::new(EN).get(Key::OutcomeNoteRetirementUnfinished);
    assert!(
        english.contains(
            "until the person who runs this wiki finishes or gives up a change to its keys"
        ),
        "{english}"
    );
    assert!(!english.contains("has finished it"), "{english}");
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
