//! Filing an appeal (RFC 006 D-9; handoff G-T5; threat model C-14g): who may, against what, in how many words, with what
//! audited, and what it cannot be used for. Each test names the rule it holds and runs on both stores.

use inklkunn_store::{AppealState, AppealedKind, AuditOutcome, Role, Visibility};

use super::scenes::scene;
use super::support::{DAY, KINDS, Kind, T0, World, set_now};
use crate::Action;
use crate::appeal::{AppealError, AppealLimits, ContestRef, MAX_TEXT_BYTES, Offer, TextFault};
use crate::authorize::Resource;

const WORDS: &str = "I did not do what I am said to have done.";

#[test]
fn the_person_an_action_affects_files_an_appeal_against_each_of_the_five_kinds_and_it_is_listed_for_them_and_for_the_decider()
 {
    for kind in KINDS {
        for what in AppealedKind::ALL {
            let w = World::new(kind);
            let admin = w.person("Dee", Some(Role::Admin));
            let s = scene(&w, what);
            assert_eq!(
                w.appeals.offer(&s.appellant.principal, &w.wiki, &s.contest),
                Offer::Contest,
                "{what:?}: the person it affects is offered it"
            );
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap_or_else(|e| panic!("{what:?}: {e}"));
            assert_eq!(filed.kind, what);
            let record = w.appeal(&filed.id);
            assert_eq!(record.appellant, s.appellant.id);
            assert_eq!(record.action, s.contest.action());
            assert_eq!(record.state, AppealState::Open);
            assert_eq!(record.text, WORDS);
            assert_eq!(
                record.action_actor.as_ref(),
                Some(&s.actor.id),
                "{what:?}: the one who took the action is kept, so that they cannot decide it"
            );
            assert_eq!(
                w.appeals.offer(&s.appellant.principal, &w.wiki, &s.contest),
                Offer::Filed(filed.id.clone()),
                "{what:?}: once filed, the link says so"
            );

            let queue = w.appeals.appeals(&admin.principal, &w.wiki).unwrap();
            assert_eq!(queue.decided.len(), 0);
            let [queued] = queue.open.as_slice() else {
                panic!("one open appeal: {queue:?}");
            };
            assert_eq!(queued.row.id, filed.id);
            assert_eq!(queued.row.text, WORDS);
            assert_eq!(queued.row.appellant, s.appellant.id);
            assert_eq!(queued.row.appellant_name, "Pat");
            assert_eq!(queued.row.action_by.as_deref(), Some(s.actor.display()));
            assert_eq!(queued.blocked, None);

            let mine = w.appeals.mine(&s.appellant.principal, &w.wiki).unwrap();
            assert_eq!(mine.appeals.len(), 1, "{what:?}");
            assert_eq!(mine.appeals.first().map(|r| &r.id), Some(&filed.id));
        }
    }
}

#[test]
fn a_person_the_action_did_not_affect_cannot_appeal_it_and_is_told_what_they_are_told_of_an_action_that_is_not_there()
 {
    for kind in KINDS {
        for what in AppealedKind::ALL {
            let w = World::new(kind);
            let s = scene(&w, what);
            let stranger = w.person("Sam", Some(Role::Contributor));
            let by_a_stranger = w
                .appeals
                .file(&stranger.principal, &w.wiki, &s.contest, WORDS)
                .unwrap_err();
            assert_eq!(by_a_stranger, AppealError::NotAffected, "{what:?}");
            let nothing = ContestRef {
                kind: what,
                id: match what {
                    AppealedKind::Decline => "999999".to_owned(),
                    _ => inklkunn_store::AppealId::new().to_string(),
                },
            };
            let by_the_affected = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &nothing, WORDS)
                .unwrap_err();
            assert_eq!(
                by_the_affected,
                AppealError::NotAffected,
                "{what:?}: an action that is not there is answered as one that is not theirs"
            );
            assert_eq!(
                w.appeals.offer(&stranger.principal, &w.wiki, &s.contest),
                Offer::Nothing,
                "{what:?}: no link for a person it does not affect"
            );
            assert!(
                w.store.open_appeals(&w.wiki.id, 10).unwrap().is_empty(),
                "{what:?}: nothing was recorded"
            );
        }
    }
}

#[test]
fn a_protection_is_appealed_by_someone_it_holds_back_and_not_by_a_bystander_with_a_proposal_elsewhere()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let mover = w.person("Max", Some(Role::Maintainer));
        let held = w.person("Pat", Some(Role::Contributor));
        let elsewhere = w.person("Eli", Some(Role::Contributor));
        let _held = w.proposal(&held, "Home");
        let _other = w.proposal(&elsewhere, "Other");
        let protection = w.protect(&mover, "Home", 3);
        assert_eq!(
            w.appeals.offer(&held.principal, &w.wiki, &protection),
            Offer::Contest
        );
        assert_eq!(
            w.appeals.offer(&elsewhere.principal, &w.wiki, &protection),
            Offer::Nothing,
            "a proposal the protection does not cover is not held back by it"
        );
        assert_eq!(
            w.appeals
                .file(&elsewhere.principal, &w.wiki, &protection, WORDS)
                .unwrap_err(),
            AppealError::NotAffected
        );
        w.appeals
            .file(&held.principal, &w.wiki, &protection, WORDS)
            .unwrap();
    }
}

#[test]
fn a_second_open_appeal_against_the_same_action_by_the_same_person_is_refused_and_names_the_first()
{
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        let first = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let again = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, "Once more.")
            .unwrap_err();
        assert_eq!(again, AppealError::AlreadyOpen { appeal: first.id });
        assert_eq!(w.store.open_appeals(&w.wiki.id, 10).unwrap().len(), 1);
    }
}

#[test]
fn a_restricted_account_files_the_appeal_of_its_own_restriction_and_the_path_cannot_be_used_to_comment_or_propose()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        let wiki = Resource::Wiki {
            wiki: w.wiki.id.clone(),
        };
        // The restriction holds them down: they cannot comment, propose or review, and an appeal is not one of those.
        for action in [Action::Comment, Action::Propose, Action::Review] {
            assert!(
                !w.authorizer
                    .authorize(&s.appellant.principal, action, &wiki)
                    .is_allowed(),
                "{action:?}: held down"
            );
        }
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .expect("a restricted person may appeal their own restriction");
        // Filing did not lift anything, and it opened no other way of writing.
        for action in [Action::Comment, Action::Propose, Action::Review] {
            assert!(
                !w.authorizer
                    .authorize(&s.appellant.principal, action, &wiki)
                    .is_allowed(),
                "{action:?}: still held down after the appeal"
            );
        }
        assert!(w.restricted(&s.appellant));
        // It is no other act's door: they cannot appeal what is not theirs, restricted or not.
        let other = scene_other_action(&w);
        assert_eq!(
            w.appeals
                .file(&s.appellant.principal, &w.wiki, &other, WORDS)
                .unwrap_err(),
            AppealError::NotAffected
        );
        assert_eq!(w.store.open_appeals(&w.wiki.id, 10).unwrap().len(), 1);
        let decision = w.events("file-appeal");
        let last = decision.last().expect("the decision is in the trail");
        assert_eq!(last.outcome, AuditOutcome::Allowed);
        assert!(
            last.detail.starts_with("rung=observer;"),
            "the trail names the rung the restriction left them: {last:?}"
        );
        assert_eq!(filed.kind, AppealedKind::Restriction);
    }
}

/// A restriction of somebody else.
fn scene_other_action(w: &World) -> ContestRef {
    let someone = w.person("Una", Some(Role::Contributor));
    let by = w.person("Max2", Some(Role::Maintainer));
    w.restrict(&by, &someone, 3)
}

#[test]
fn an_anonymous_visitor_files_nothing_and_a_wiki_the_person_may_not_read_is_not_there() {
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        assert_eq!(
            w.appeals
                .file(&crate::Principal::Anonymous, &w.wiki, &s.contest, WORDS)
                .unwrap_err(),
            AppealError::NotAllowed
        );
        assert_eq!(
            w.appeals
                .offer(&crate::Principal::Anonymous, &w.wiki, &s.contest),
            Offer::Nothing
        );

        // A restricted wiki: a person with no role in it is answered as if the wiki were not there, whoever they are.
        let closed = World::with(
            kind,
            Visibility::Restricted,
            None,
            AppealLimits::default(),
            super::support::now,
        );
        let s = scene(&closed, AppealedKind::Restriction);
        let outsider = closed.person("Olga", None);
        let from_outside = closed
            .appeals
            .file(&outsider.principal, &closed.wiki, &s.contest, WORDS)
            .unwrap_err();
        assert_eq!(from_outside, AppealError::NotFound);
        let absent = w
            .appeals
            .file(
                &outsider.principal,
                &inklkunn_store::WikiRecord {
                    id: inklkunn_store::WikiId::new(),
                    ..w.wiki.clone()
                },
                &s.contest,
                WORDS,
            )
            .unwrap_err();
        assert_eq!(
            from_outside, absent,
            "a closed wiki and an absent one answer alike"
        );
        assert_eq!(
            closed
                .appeals
                .appeals(&outsider.principal, &closed.wiki)
                .unwrap_err(),
            AppealError::NotFound
        );
        assert_eq!(
            closed
                .appeals
                .mine(&outsider.principal, &closed.wiki)
                .unwrap_err(),
            AppealError::NotFound
        );
    }
}

#[test]
fn an_appeal_is_one_bounded_text_with_line_breaks_and_without_control_characters() {
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        let file = |text: &str| {
            w.appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, text)
        };
        assert_eq!(file("").unwrap_err(), AppealError::Text(TextFault::Empty));
        assert_eq!(
            file("  \n\t ").unwrap_err(),
            AppealError::Text(TextFault::Empty)
        );
        assert_eq!(
            file(&"x".repeat(MAX_TEXT_BYTES + 1)).unwrap_err(),
            AppealError::Text(TextFault::TooLong)
        );
        for bad in [
            "a\u{0}b",
            "a\u{202e}b",
            "a\u{2028}b",
            "a\u{7}b",
            "a\u{feff}b",
        ] {
            assert_eq!(
                file(bad).unwrap_err(),
                AppealError::Text(TextFault::NotText),
                "{bad:?}"
            );
        }
        assert!(
            w.store.open_appeals(&w.wiki.id, 10).unwrap().is_empty(),
            "a refused text records nothing"
        );
        let filed = file("  First line.\r\n\tSecond line.\r\n").expect("a text with line breaks");
        assert_eq!(w.appeal(&filed.id).text, "First line.\n\tSecond line.");
    }
    // The longest text is accepted.
    let w = World::new(Kind::Sqlite);
    let s = scene(&w, AppealedKind::Restriction);
    w.appeals
        .file(
            &s.appellant.principal,
            &w.wiki,
            &s.contest,
            &"y".repeat(MAX_TEXT_BYTES),
        )
        .expect("the bound itself");
}

#[test]
fn an_appeal_against_a_restriction_that_has_run_out_is_answered_as_one_that_is_not_there_and_nothing_is_recorded()
 {
    // A restriction that has run out is not in force: it is no longer there to contest, and the person is told no more than so.
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        set_now(T0 + 4 * DAY);
        assert_eq!(
            w.appeals.offer(&s.appellant.principal, &w.wiki, &s.contest),
            Offer::Nothing
        );
        assert_eq!(
            w.appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap_err(),
            AppealError::NotAffected
        );
        set_now(T0);
    }
}

#[test]
fn the_filing_is_audited_before_the_record_and_a_record_that_fails_leaves_a_failed_event() {
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);

        // Before the record: when the act's event is kept, the appeal is not there yet.
        let store = w.store.clone();
        let wiki = w.wiki.id.clone();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let (service, trail) = w.service_with(move |event| {
            if event.action == "appeal-filed" {
                log.lock()
                    .unwrap()
                    .push(store.open_appeals(&wiki, 10).unwrap().len());
            }
        });
        service
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![0],
            "the event is written before the record"
        );
        let events = trail.events();
        let filed: Vec<_> = events
            .iter()
            .filter(|e| e.action == "appeal-filed")
            .collect();
        assert_eq!(filed.len(), 1);
        let event = filed.first().expect("an event");
        assert_eq!(event.outcome, AuditOutcome::Allowed);
        assert!(event.resource.starts_with("appeal:"), "{event:?}");
        assert!(
            event
                .detail
                .starts_with("rung=observer; policy=defaults; filing; kind=restriction"),
            "the rung, the block and the kind, and nothing of the text: {event:?}"
        );
        assert!(!event.detail.contains("did not do"), "{event:?}");
        // The decision of the authorizer is in the world's own trail, with the rung and the block.
        let decision = w.events("file-appeal");
        assert_eq!(decision.len(), 1);
        assert!(
            decision
                .first()
                .is_some_and(|d| d.detail.contains("policy=defaults")),
            "{decision:?}"
        );
    }

    // A record that fails: another appeal of the same person's gets in between the event and the record.
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Restriction);
        let (store, wiki) = (w.store.clone(), w.wiki.id.clone());
        let (appellant, action) = (s.appellant.id.clone(), s.contest.action());
        let (service, trail) = w.service_with(move |event| {
            if event.action == "appeal-filed" && event.outcome == AuditOutcome::Allowed {
                store
                    .file_appeal(&inklkunn_store::AppealRecord {
                        id: inklkunn_store::AppealId::new(),
                        wiki: wiki.clone(),
                        at: T0,
                        action: action.clone(),
                        action_actor: None,
                        appellant: appellant.clone(),
                        text: "First.".to_owned(),
                        state: AppealState::Open,
                    })
                    .unwrap();
            }
        });
        let refused = service
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap_err();
        assert!(
            matches!(refused, AppealError::AlreadyOpen { .. }),
            "{refused:?}"
        );
        let events: Vec<_> = trail
            .events()
            .into_iter()
            .filter(|e| e.action == "appeal-filed")
            .collect();
        let [first, second] = events.as_slice() else {
            panic!("two events: {events:?}");
        };
        assert_eq!(first.outcome, AuditOutcome::Allowed);
        assert_eq!(second.outcome, AuditOutcome::Failed);
        assert_eq!(first.resource, second.resource);
        assert!(second.detail.ends_with("failed=already-open"), "{second:?}");
    }
}
