//! Deciding an appeal (RFC 006 D-9; handoff G-T5; threat model C-14g): by whom, once, with a reason, and what the trail says.

use std::sync::{Arc, Mutex};

use inklkunn_store::{AppealState, AppealedKind, AuditOutcome, Role, RoleGrant};

use super::scenes::scene;
use super::support::{KINDS, T0, World, set_now};
use crate::appeal::{AppealError, DeciderRefusal, Outcome};
use crate::people::ReasonFault;

const WORDS: &str = "That was not me.";
const WHY: &str = "The record supports it.";

/// Maintainers decide appeals in this wiki, so that the person who took an action is a person who could decide it.
const MAINTAINERS_DECIDE: &str = "rungs:\n  decide_appeal: maintainer\n";

#[test]
fn nobody_decides_an_appeal_against_their_own_action_and_it_is_said_by_name() {
    for kind in KINDS {
        for what in AppealedKind::ALL {
            let w = World::under(kind, MAINTAINERS_DECIDE);
            // For a grant the one who acted is an admin, who always decides.
            let s = scene(&w, what);
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap();
            let by_the_actor = w
                .appeals
                .decide(
                    &s.actor.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Overturned,
                    WHY,
                )
                .unwrap_err();
            assert_eq!(
                by_the_actor,
                AppealError::Decider(DeciderRefusal::TheActor),
                "{what:?}"
            );
            let by_the_actor_upholding = w
                .appeals
                .decide(&s.actor.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
                .unwrap_err();
            assert_eq!(
                by_the_actor_upholding,
                AppealError::Decider(DeciderRefusal::TheActor),
                "{what:?}: upholding one's own act is refused as well"
            );
            assert_eq!(w.appeal(&filed.id).state, AppealState::Open, "{what:?}");
            // The queue says so beside the appeal, to the person looking.
            let queue = w.appeals.appeals(&s.actor.principal, &w.wiki).unwrap();
            assert_eq!(
                queue.open.first().map(|q| q.blocked),
                Some(Some(DeciderRefusal::TheActor)),
                "{what:?}"
            );
        }
    }
}

#[test]
fn nobody_decides_their_own_appeal() {
    // The appellant is given the rung that decides, so that only the rule stops them.
    for kind in KINDS {
        for what in [
            AppealedKind::Decline,
            AppealedKind::Tombstone,
            AppealedKind::Protection,
        ] {
            let w = World::under(kind, MAINTAINERS_DECIDE);
            let s = scene(&w, what);
            w.store
                .grant(RoleGrant {
                    account: s.appellant.id.clone(),
                    wiki: w.wiki.id.clone(),
                    role: Role::Maintainer,
                    granted_by: None,
                    granted_at: 0,
                })
                .unwrap();
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap();
            assert_eq!(
                w.appeals
                    .decide(
                        &s.appellant.principal,
                        &w.wiki,
                        &filed.id,
                        Outcome::Overturned,
                        WHY
                    )
                    .unwrap_err(),
                AppealError::Decider(DeciderRefusal::TheAppellant),
                "{what:?}"
            );
            assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
            // Someone else holding the rung decides it.
            let other = w.person("Una", Some(Role::Maintainer));
            w.appeals
                .decide(&other.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
                .unwrap();
        }
    }
}

#[test]
fn only_the_rung_the_rules_name_decides_and_a_person_who_may_not_is_told_nothing_of_the_appeal() {
    for kind in KINDS {
        // The default is the admin.
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Decline);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let reviewer = w.person("Rae", Some(Role::Reviewer));
        for who in [&maintainer, &reviewer] {
            assert_eq!(
                w.appeals
                    .decide(&who.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
                    .unwrap_err(),
                AppealError::NotAllowed
            );
            assert_eq!(
                w.appeals.appeals(&who.principal, &w.wiki).unwrap_err(),
                AppealError::NotAllowed,
                "the queue is the deciding rung's"
            );
        }
        assert_eq!(
            w.appeals
                .decide(
                    &crate::Principal::Anonymous,
                    &w.wiki,
                    &filed.id,
                    Outcome::Upheld,
                    WHY
                )
                .unwrap_err(),
            AppealError::NotAllowed
        );
        // An appeal that is not there is answered alike to a person who does not decide.
        assert_eq!(
            w.appeals
                .decide(
                    &maintainer.principal,
                    &w.wiki,
                    &inklkunn_store::AppealId::new(),
                    Outcome::Upheld,
                    WHY
                )
                .unwrap_err(),
            AppealError::NotAllowed
        );
        let admin = w.person("Dee", Some(Role::Admin));
        assert_eq!(
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &inklkunn_store::AppealId::new(),
                    Outcome::Upheld,
                    WHY
                )
                .unwrap_err(),
            AppealError::NoSuchAppeal
        );

        // The rules can say a maintainer decides.
        let w = World::under(kind, MAINTAINERS_DECIDE);
        let s = scene(&w, AppealedKind::Decline);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let other = w.person("Mia", Some(Role::Maintainer));
        w.appeals
            .decide(&other.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
            .unwrap();
    }
}

#[test]
fn a_maintainer_who_is_restricted_does_not_decide_and_is_told_why() {
    for kind in KINDS {
        let w = World::under(kind, MAINTAINERS_DECIDE);
        let s = scene(&w, AppealedKind::Decline);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let held = w.person("Hal", Some(Role::Maintainer));
        let by = w.person("Ada", Some(Role::Admin));
        w.restrict(&by, &held, 3);
        let refused = w
            .appeals
            .decide(&held.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
            .unwrap_err();
        let AppealError::Restricted(notice) = refused else {
            panic!("restricted: {refused:?}");
        };
        assert_eq!(notice.reason, "Edit-warring.");
        assert_eq!(notice.until, T0 + 3 * super::support::DAY);
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
    }
}

#[test]
fn an_appeal_is_decided_once_and_the_reason_is_required_and_one_line() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let second = w.person("Eve", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        for bad in ["", "   "] {
            assert_eq!(
                w.appeals
                    .decide(&admin.principal, &w.wiki, &filed.id, Outcome::Upheld, bad)
                    .unwrap_err(),
                AppealError::Reason(ReasonFault::Missing)
            );
        }
        assert_eq!(
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Upheld,
                    "a\nb"
                )
                .unwrap_err(),
            AppealError::Reason(ReasonFault::NotOneLine)
        );
        assert_eq!(
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Upheld,
                    &"x".repeat(501)
                )
                .unwrap_err(),
            AppealError::Reason(ReasonFault::TooLong)
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);

        let made = w
            .appeals
            .decide(&admin.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
            .unwrap();
        assert_eq!(made.outcome, Outcome::Upheld);
        // Upheld: the action stands, and the appeal records who said so and why.
        assert!(w.restricted(&s.appellant), "the restriction stands");
        let AppealState::Upheld(decided) = w.appeal(&filed.id).state else {
            panic!("upheld");
        };
        assert_eq!(
            (decided.by, decided.reason.as_str()),
            (admin.id.clone(), WHY)
        );
        for outcome in [Outcome::Upheld, Outcome::Overturned] {
            assert_eq!(
                w.appeals
                    .decide(&second.principal, &w.wiki, &filed.id, outcome, WHY)
                    .unwrap_err(),
                AppealError::AlreadyDecided,
                "decided once, and for good"
            );
        }
        assert!(
            w.restricted(&s.appellant),
            "a second decision changed nothing"
        );
        // Decided, the person may appeal the same action again (it is no longer open).
        w.appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, "Again.")
            .expect("a new appeal after a decision");
    }
}

#[test]
fn each_decision_is_audited_with_the_rung_and_the_block_before_the_record_and_never_the_words() {
    for kind in KINDS {
        let w = World::under(kind, "rungs:\n  decide_appeal: admin\n");
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let (store, id) = (w.store.clone(), filed.id.clone());
        let at_event = Arc::new(Mutex::new(Vec::new()));
        let seen = at_event.clone();
        let (service, trail) = w.service_with(move |event| {
            if event.action == "appeal-decided" {
                seen.lock()
                    .unwrap()
                    .push(store.appeal(&id).unwrap().unwrap().state.as_str());
            }
        });
        service
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap();
        assert_eq!(
            *at_event.lock().unwrap(),
            vec!["open"],
            "the event is written before the record"
        );
        let events: Vec<_> = trail
            .events()
            .into_iter()
            .filter(|e| e.action == "appeal-decided")
            .collect();
        let [event] = events.as_slice() else {
            panic!("one event: {events:?}");
        };
        assert_eq!(event.outcome, AuditOutcome::Allowed);
        assert_eq!(event.principal, format!("account:{}", admin.id));
        assert!(
            event.detail.starts_with(
                "rung=admin; policy=0123456789ab; decision; kind=restriction; outcome=overturned"
            ),
            "{event:?}"
        );
        assert!(
            !event.detail.contains("supports") && !event.detail.contains("not me"),
            "never the words: {event:?}"
        );
        let decision = w.events("decide-appeal");
        assert_eq!(decision.len(), 1);
        assert_eq!(
            decision.first().map(|d| d.outcome),
            Some(AuditOutcome::Allowed)
        );
        assert!(
            decision
                .first()
                .is_some_and(|d| d.detail.starts_with("rung=admin; policy=0123456789ab")),
            "{decision:?}"
        );
        // A person who may not decide leaves a denied decision, and nothing else.
        let before = w.events("appeal-decided").len();
        let mia = w.person("Mia", Some(Role::Maintainer));
        let _ = w
            .appeals
            .decide(&mia.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY);
        assert_eq!(
            w.events("decide-appeal").last().map(|d| d.outcome),
            Some(AuditOutcome::Denied)
        );
        assert_eq!(w.events("appeal-decided").len(), before);
    }
}

#[test]
fn a_decision_whose_record_fails_leaves_a_failed_event_and_the_reversal_is_not_kept() {
    for kind in KINDS {
        // Someone else gets in first, in the moment between the event and the record.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let first = w.person("Eve", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let (service, trail) = {
            let (store, id, by) = (w.store.clone(), filed.id.clone(), first.id.clone());
            w.service_with(move |event| {
                if event.action == "appeal-decided" && event.outcome == AuditOutcome::Allowed {
                    store
                        .decide_appeal(
                            &id,
                            &inklkunn_store::AppealDecision {
                                at: T0,
                                by: by.clone(),
                                reason: "First.".to_owned(),
                                outcome: inklkunn_store::AppealOutcome::Upheld,
                            },
                        )
                        .unwrap();
                }
            })
        };
        let refused = service
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap_err();
        assert_eq!(refused, AppealError::AlreadyDecided);
        let events: Vec<_> = trail
            .events()
            .into_iter()
            .filter(|e| e.action == "appeal-decided")
            .collect();
        let [allowed, failed] = events.as_slice() else {
            panic!("two events: {events:?}");
        };
        assert_eq!(allowed.outcome, AuditOutcome::Allowed);
        assert_eq!(failed.outcome, AuditOutcome::Failed);
        assert_eq!(allowed.resource, failed.resource);
        assert!(
            failed.detail.ends_with("failed=already-decided"),
            "{failed:?}"
        );
        assert!(
            w.restricted(&s.appellant),
            "the one that lost did not reverse anything: the first said upheld"
        );
    }

    // The reversal and the decision are one transaction (the store's guarantee, held by its own tests): where the decision cannot be
    // made, the reversal is not kept. A role that changed between the check and the record makes the record fail.
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Grant);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        assert_eq!(w.grant_of(&s.appellant), Some(Role::Reviewer));
        let (service, trail) = {
            let (store, wiki, target, by) = (
                w.store.clone(),
                w.wiki.id.clone(),
                s.appellant.id.clone(),
                admin.id.clone(),
            );
            w.service_with(move |event| {
                if event.action == "appeal-decided" && event.outcome == AuditOutcome::Allowed {
                    // Someone else lowers the role again.
                    store
                        .record_grant_decision(&inklkunn_store::GrantDecisionRecord {
                            id: inklkunn_store::GrantDecisionId::new(),
                            wiki: wiki.clone(),
                            at: T0,
                            by: Some(inklkunn_store::Actor {
                                account: by.clone(),
                                rung: Role::Admin,
                            }),
                            policy_block: None,
                            target: target.clone(),
                            before: Some(Role::Reviewer),
                            after: Some(Role::Contributor),
                            reason: None,
                            appeal: None,
                        })
                        .unwrap();
                }
            })
        };
        let refused = service
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap_err();
        assert_eq!(refused, AppealError::Changed);
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open, "not decided");
        assert_eq!(
            w.grant_of(&s.appellant),
            Some(Role::Contributor),
            "the reversal was not kept: the role is the later change's"
        );
        assert!(trail.events().iter().any(|e| e.action == "appeal-decided"
            && e.outcome == AuditOutcome::Failed
            && e.detail.ends_with("failed=state-changed")));
    }
    set_now(T0);
}

#[test]
fn a_wiki_whose_rules_cannot_be_read_refuses_to_file_and_to_decide_like_every_write_and_says_why() {
    for kind in KINDS {
        let w = World::faulted(kind);
        let s = scene(&w, AppealedKind::Restriction);
        let admin = w.person("Dee", Some(Role::Admin));
        let refused = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap_err();
        assert!(
            matches!(refused, AppealError::RulesUnreadable(_)),
            "{refused:?}"
        );
        let refused = w
            .appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &inklkunn_store::AppealId::new(),
                Outcome::Upheld,
                WHY,
            )
            .unwrap_err();
        assert!(
            matches!(refused, AppealError::RulesUnreadable(_)),
            "{refused:?}"
        );
        assert!(w.store.open_appeals(&w.wiki.id, 10).unwrap().is_empty());
    }
}

#[test]
fn everyone_who_set_or_renewed_a_restriction_is_barred_from_deciding_its_appeal_and_a_third_person_is_not()
 {
    for kind in KINDS {
        // The appeal is against the renewal in force: the person who set it and the person who renewed it are both barred.
        let w = World::under(kind, MAINTAINERS_DECIDE);
        let setter = w.person("Max", Some(Role::Maintainer));
        let renewer = w.person("Ren", Some(Role::Maintainer));
        let third = w.person("Tia", Some(Role::Maintainer));
        let pat = w.person("Pat", Some(Role::Contributor));
        let set = w.restrict(&setter, &pat, 3);
        let renewal = w.renew(&renewer, &pat, &set, 5);
        let filed = w
            .appeals
            .file(&pat.principal, &w.wiki, &renewal, WORDS)
            .unwrap();
        for barred in [&setter, &renewer] {
            assert_eq!(
                w.appeals
                    .decide(&barred.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
                    .unwrap_err(),
                AppealError::Decider(DeciderRefusal::TheActor),
                "{kind:?}"
            );
            let queue = w.appeals.appeals(&barred.principal, &w.wiki).unwrap();
            assert_eq!(
                queue.open.first().map(|q| q.blocked),
                Some(Some(DeciderRefusal::TheActor)),
                "{kind:?}: the queue says so"
            );
        }
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
        w.appeals
            .decide(
                &third.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("a third person decides");
        assert!(!w.restricted(&pat), "{kind:?}");

        // The appeal is against the setting, and the restriction is renewed after it was filed: the renewer is barred as well.
        let w = World::under(kind, MAINTAINERS_DECIDE);
        let setter = w.person("Max", Some(Role::Maintainer));
        let renewer = w.person("Ren", Some(Role::Maintainer));
        let third = w.person("Tia", Some(Role::Maintainer));
        let pat = w.person("Pat", Some(Role::Contributor));
        let set = w.restrict(&setter, &pat, 3);
        let filed = w
            .appeals
            .file(&pat.principal, &w.wiki, &set, WORDS)
            .unwrap();
        w.renew(&renewer, &pat, &set, 5);
        for barred in [&setter, &renewer] {
            assert_eq!(
                w.appeals
                    .decide(
                        &barred.principal,
                        &w.wiki,
                        &filed.id,
                        Outcome::Overturned,
                        WHY
                    )
                    .unwrap_err(),
                AppealError::Decider(DeciderRefusal::TheActor),
                "{kind:?}"
            );
        }
        assert!(w.restricted(&pat), "{kind:?}: nothing was reversed");
        w.appeals
            .decide(
                &third.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("a third person decides");
        assert!(!w.restricted(&pat), "{kind:?}");
    }
}

#[test]
fn everyone_who_set_or_renewed_a_protection_is_barred_from_deciding_its_appeal() {
    use inklkunn_store::{
        MeasureEvent, ProtectionEvent, ProtectionEventId, ProtectionKind, ProtectionRequirement,
    };
    for kind in KINDS {
        let w = World::under(kind, MAINTAINERS_DECIDE);
        let setter = w.person("Max", Some(Role::Maintainer));
        let renewer = w.person("Ren", Some(Role::Maintainer));
        let third = w.person("Tia", Some(Role::Maintainer));
        let pat = w.person("Pat", Some(Role::Contributor));
        w.proposal(&pat, "Home");
        let set = w.protect(&setter, "Home", 3);
        let filed = w
            .appeals
            .file(&pat.principal, &w.wiki, &set, WORDS)
            .unwrap();
        w.store
            .record_protection_event(&ProtectionEvent {
                id: ProtectionEventId::new(),
                wiki: w.wiki.id.clone(),
                at: T0,
                event: MeasureEvent::Renewed,
                kind: ProtectionKind::Page,
                target: "Home".to_owned(),
                requirement: Some(ProtectionRequirement::Review),
                by: inklkunn_store::Actor {
                    account: renewer.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                reason: Some("Still vandalism.".to_owned()),
                expires_at: Some(T0 + 5 * 86_400),
                of_event: ProtectionEventId::parse(&set.id),
                appeal: None,
            })
            .unwrap();
        for barred in [&setter, &renewer] {
            assert_eq!(
                w.appeals
                    .decide(&barred.principal, &w.wiki, &filed.id, Outcome::Upheld, WHY)
                    .unwrap_err(),
                AppealError::Decider(DeciderRefusal::TheActor),
                "{kind:?}"
            );
        }
        w.appeals
            .decide(
                &third.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("a third person decides");
        assert!(!w.protected(), "{kind:?}");
    }
}
