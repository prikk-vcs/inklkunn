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

// ------------------------------------------------------------------------------ the rules of the wiki (RFC 006)

mod governance_pages {
    use inklkunn_core::governance::{
        Fault, GovernanceFile, Limit, OrderingPair, ReadFault, Refusal,
    };
    use inklkunn_store::Role;

    use super::*;
    use crate::proposals::governance;

    fn both() -> [T; 2] {
        [T::new(EN), T::new(Lang::from_def(&JA_DEF))]
    }

    #[test]
    fn every_rule_a_governance_file_can_break_has_its_own_sentence_in_both_languages() {
        for t in both() {
            let sentences: Vec<String> = Refusal::ALL
                .iter()
                .map(|r| t.get(governance::rule_key(*r)))
                .collect();
            for (rule, said) in Refusal::ALL.iter().zip(&sentences) {
                assert!(
                    !said.is_empty() && !said.contains("{"),
                    "{}: {said}",
                    rule.name()
                );
                assert!(!said.to_lowercase().contains("official"), "{said}");
            }
            let distinct: std::collections::BTreeSet<&String> = sentences.iter().collect();
            assert_eq!(distinct.len(), sentences.len(), "no two rules say the same");
        }
    }

    #[test]
    fn a_fault_names_the_file_and_the_line_in_each_language_and_never_what_the_file_said() {
        let at = Fault {
            line: Some(7),
            ..Fault::at(GovernanceFile::Policy, Refusal::UnknownKey, "rungs")
        };
        let whole = Fault::whole(GovernanceFile::Constitution, Refusal::TooLong);
        let en = T::new(EN);
        assert_eq!(
            governance::fault_text(en, &at),
            format!(
                "governance/policy.yaml, line 7: {}",
                en.get(Key::GovernanceRuleUnknownKey)
            )
        );
        assert_eq!(
            governance::fault_text(en, &whole),
            format!(
                "governance/constitution.yaml: {}",
                en.get(Key::GovernanceRuleTooLong)
            )
        );
        let ja = T::new(Lang::from_def(&JA_DEF));
        assert_eq!(
            governance::fault_text(ja, &at),
            format!(
                "governance/policy.yaml の 7 行目: {}",
                ja.get(Key::GovernanceRuleUnknownKey)
            )
        );
        // The same file and rule said twice is said once; the page then says that nothing was changed.
        for t in both() {
            let paragraphs =
                governance::invalid_paragraphs(t, &[at.clone(), at.clone(), whole.clone()]);
            assert_eq!(paragraphs.len(), 3, "{paragraphs:?}");
            assert_eq!(
                paragraphs.last(),
                Some(&t.get(Key::OutcomeNoteGovernanceInvalid))
            );
        }
        // A file outside the two, a platform limit and the ordering rule are each said in their own sentence.
        let en = T::new(EN);
        for (rule, key) in [
            (Refusal::UnknownFile, Key::GovernanceRuleUnknownFile),
            (Refusal::NotAFile, Key::GovernanceRuleNotAFile),
            (
                Refusal::OutsideDefaultView,
                Key::GovernanceRuleOutsideDefaultView,
            ),
            (
                Refusal::PlatformLimit(Limit::EntryMax),
                Key::GovernanceRulePlatformLimitEntryMax,
            ),
            (
                Refusal::Ordering(OrderingPair::ReviewAboveAccept),
                Key::GovernanceRuleOrderingReviewAboveAccept,
            ),
        ] {
            assert_eq!(governance::rule_key(rule), key);
            assert!(
                governance::fault_text(en, &Fault::whole(GovernanceFile::Policy, rule))
                    .contains(&en.get(key))
            );
        }
    }

    #[test]
    fn an_acceptance_refused_by_the_rules_of_the_wiki_is_a_page_of_its_own_each_and_the_log_keeps_the_cause()
     {
        let logged = Mutex::new(Vec::new());
        let log = |line: &str| logged.lock().unwrap().push(line.to_owned());
        let fault = Fault {
            line: Some(3),
            ..Fault::at(GovernanceFile::Policy, Refusal::WrongShape, "rungs")
        };
        let invalid = outcome_of(
            Err(AcceptFailure::GovernanceInvalid(vec![fault.clone()])),
            &log,
        );
        assert_eq!(
            invalid,
            AcceptOutcome::Unchanged(Unchanged::GovernanceInvalid(vec![fault]))
        );
        assert_eq!(
            outcome_of(Err(AcceptFailure::GovernanceRung(Role::Admin)), &log),
            AcceptOutcome::GovernanceRung(Role::Admin)
        );
        assert_eq!(
            outcome_of(
                Err(AcceptFailure::ApprovalsMissing { needed: 3, have: 1 }),
                &log
            ),
            AcceptOutcome::ApprovalsMissing { needed: 3, have: 1 }
        );
        let unreadable =
            ReadFault::from_stored(GovernanceFile::Policy, Some(4), Refusal::UnknownKey);
        assert_eq!(
            outcome_of(
                Err(AcceptFailure::RulesUnreadable(unreadable.clone())),
                &log
            ),
            AcceptOutcome::RulesUnreadable(unreadable)
        );
        let lines = logged.lock().unwrap();
        assert_eq!(lines.len(), 2, "the two that are the operator's: {lines:?}");
        assert!(
            lines.iter().all(|l| l.contains("governance/policy.yaml")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_proposal_whose_author_may_no_longer_replace_a_file_the_wiki_cannot_read_says_so_in_both_languages_and_never_the_restrictions_reason()
     {
        let log = |_: &str| {};
        for restricted in [true, false] {
            assert_eq!(
                outcome_of(Err(AcceptFailure::AuthorNotAllowed { restricted }), &log),
                AcceptOutcome::AuthorNotAllowed { restricted }
            );
        }
        for t in both() {
            let restricted = t.get(Key::OutcomeNoteAuthorRestricted);
            let not_allowed = t.get(Key::OutcomeNoteAuthorNotAllowed);
            // Two sentences, each its own, and neither carries a restriction's reason or end: the outcome has nowhere to put one.
            assert_ne!(restricted, not_allowed);
            assert!(!restricted.is_empty() && !not_allowed.is_empty());
        }
    }

    #[test]
    fn the_notes_say_which_role_and_how_many_approvals_and_the_unreadable_page_says_the_file_and_the_way_out()
     {
        for t in both() {
            assert_eq!(
                governance::rung_note(t, Role::Admin),
                t.get(Key::OutcomeNoteGovernanceRungAdmin)
            );
            for rung in [Role::Maintainer, Role::Reviewer] {
                assert_eq!(
                    governance::rung_note(t, rung),
                    t.get(Key::OutcomeNoteGovernanceRungMaintainer)
                );
            }
            assert_ne!(
                t.get(Key::OutcomeNoteGovernanceRungAdmin),
                t.get(Key::OutcomeNoteGovernanceRungMaintainer)
            );
            let fault =
                ReadFault::from_stored(GovernanceFile::Constitution, Some(2), Refusal::WrongShape);
            let page = governance::unreadable_paragraphs(t, &fault);
            assert!(
                page.first()
                    .unwrap()
                    .contains("governance/constitution.yaml")
            );
            assert_eq!(page.last(), Some(&t.get(Key::OutcomeNoteRulesUnreadable)));
        }
        // The count is said in the number the rules ask for, in English as one and as many.
        let en = T::new(EN);
        let one = governance::approvals_note(en, 1, 0);
        let many = governance::approvals_note(en, 3, 1);
        assert!(
            one.contains("1 approval of the latest revision from someone"),
            "{one}"
        );
        assert!(
            many.contains("3 approvals of the latest revision from people"),
            "{many}"
        );
        assert!(many.contains("it has 1."), "{many}");
        let ja = governance::approvals_note(T::new(Lang::from_def(&JA_DEF)), 3, 1);
        assert!(ja.contains("3 件") && ja.contains("1 件"), "{ja}");
    }

    #[test]
    fn the_approvals_warning_says_that_no_proposal_can_be_accepted_until_more_people_hold_the_role()
    {
        let en = T::new(EN);
        let one = en.msg(Key::ProposalApprovalsUnreachable).n(1).text();
        let many = en.msg(Key::ProposalApprovalsUnreachable).n(4).text();
        assert!(
            one.contains("1 approval from someone other than the proposer"),
            "{one}"
        );
        assert!(
            many.contains("4 approvals from people other than the proposer"),
            "{many}"
        );
        for said in [&one, &many] {
            assert!(said.contains(
                "No proposal can be accepted until more people hold the role that reviews"
            ));
            assert!(!said.to_lowercase().contains("official"));
        }
        let ja = T::new(Lang::from_def(&JA_DEF))
            .msg(Key::ProposalApprovalsUnreachable)
            .n(4)
            .text();
        assert!(ja.contains("4 件"), "{ja}");
    }
}
