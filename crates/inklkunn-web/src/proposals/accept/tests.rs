#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;

use inklkunn_core::knowledge::check::{DayBound, Refusal};
use inklkunn_core::knowledge::{Fault, Rule};

use super::*;
use crate::catalog::{EN, JA_DEF, Lang};

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
        AcceptOutcome::Unchanged(Unchanged::KnowledgeInvalid(_))
    ));
    let lines = logged.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines.first().unwrap().contains("knowledge/stray.txt"));
}

/// K80 (the review's H-c): the sentences of a carried check say the source carries a day or an answer **this proposal does
/// not state**, and name the way to state one: `checked` in a curation, the proposal's own id in `last_reviewed.proposal`.
#[test]
fn the_sentences_of_a_carried_check_say_what_is_not_stated_and_name_the_way_to_state_one() {
    for t in [T::new(EN), T::new(Lang::from_def(&JA_DEF))] {
        for key in [Key::InvalidKnowledgeCheckUnstated, Key::KnowCheckUnstated] {
            let text = t.get(key);
            assert!(text.contains("`checked`"), "{text}");
            assert!(text.contains("`last_reviewed.proposal`"), "{text}");
        }
    }
    let english = T::new(EN);
    for key in [Key::InvalidKnowledgeCheckUnstated, Key::KnowCheckUnstated] {
        let text = english.get(key);
        assert!(text.contains("that this proposal does not state"), "{text}");
        // It is not said to be the old reference's alone: a day naming no proposal, or another, is refused too.
        assert!(!text.contains("which were for its old place"), "{text}");
    }
}

fn carried_refusal() -> Refusal {
    Refusal {
        carries: true,
        ..Refusal::of(
            "knowledge/sources/x.yaml",
            Fault::at(Rule::Status, "last_reviewed"),
        )
    }
}

/// K78 (the review's F-c): the page of an acceptance that a knowledge rule refused says that rule's own sentence, never the
/// note about a file that is not in the form the wiki reads; the note stays for such a file.
#[test]
fn a_refused_acceptance_says_each_rules_own_sentence_and_the_file_note_only_for_a_file_that_is_not_in_the_form()
 {
    for t in [T::new(EN), T::new(Lang::from_def(&JA_DEF))] {
        let form_note = t.get(Key::OutcomeNoteKnowledgeInvalid);
        let kept = t.get(Key::OutcomeNoteKnowledgeKept);
        // A rule about what a proposal may set: its sentence once, and nothing about the form of a file.
        let status = Refusal::of(
            "knowledge/claims/x.yaml",
            Fault::at(Rule::Status, "community_status"),
        );
        assert_eq!(
            knowledge_paragraphs(t, &[status.clone(), status]),
            [t.get(Key::HealthRuleStatus), kept.clone()]
        );
        // A source that carries the check of its old reference: the sentence of that refusal.
        assert_eq!(
            knowledge_paragraphs(t, &[carried_refusal()]),
            [t.get(Key::KnowCheckUnstated), kept.clone()]
        );
        // A stated check whose day is out of the bounds: the sentence of that bound (K79), never the rule's.
        for (bound, key) in [
            (DayBound::BeforeReview, Key::KnowCheckBeforeReview),
            (DayBound::NotCome, Key::KnowCheckNotCome),
        ] {
            let bounded = Refusal {
                bound: Some(bound),
                ..Refusal::of(
                    "knowledge/sources/x.yaml",
                    Fault::at(Rule::Status, "last_reviewed"),
                )
            };
            assert_eq!(
                knowledge_paragraphs(t, &[bounded]),
                [t.get(key), kept.clone()]
            );
            assert_ne!(t.get(key), t.get(Key::HealthRuleStatus));
        }
        assert_ne!(
            t.get(Key::KnowCheckBeforeReview),
            t.get(Key::KnowCheckNotCome)
        );
        // A removal is not offered.
        let removal = Refusal {
            removes: true,
            ..Refusal::of(
                "knowledge/claims/x.yaml",
                Fault::at(Rule::Status, "community_status"),
            )
        };
        assert_eq!(
            knowledge_paragraphs(t, &[removal]),
            [t.get(Key::KnowNotByForm), kept]
        );
        // A file that is not in the form the wiki reads keeps the note that says so, after the sentences.
        let syntax = Refusal::of("knowledge/claims/y.yaml", Fault::whole(Rule::Syntax));
        assert_eq!(
            knowledge_paragraphs(t, &[syntax, carried_refusal()]),
            [
                t.get(Key::HealthRuleSyntax),
                t.get(Key::KnowCheckUnstated),
                form_note.clone()
            ]
        );
        // And the file note is said of no rule refusal.
        let dangling = Refusal::of("knowledge/claims/z.yaml", Fault::whole(Rule::Reference));
        for other in [carried_refusal(), dangling] {
            assert!(!knowledge_paragraphs(t, &[other]).contains(&form_note));
        }
    }
}
