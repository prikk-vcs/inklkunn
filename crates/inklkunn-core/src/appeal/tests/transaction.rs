//! **An overturn and its decision are one transaction** (A-1): the unhide of the hide the appeal names, the unhides of the rest of
//! its act, the proposal's reopening and the decision are all written or none is. A moderator's unhide of one page while the
//! decision is made (it takes no decider lock) makes the decision fail whole: no page is left unhidden under an appeal that is still
//! open, and no proposal is left reopened.

use std::sync::{Arc, Barrier};
use std::thread;

use inklkunn_store::{
    AppealState, AppealedKind, AuditOutcome, ProposalState, Role, TombstoneAct, TombstoneActId,
    TombstoneEvent, TombstoneEventId,
};

use super::overturn::{hide_page, hide_revision};
use super::support::Person;
use super::support::{KINDS, Kind, T0, World, now, steady};
use crate::appeal::AppealService;
use crate::appeal::{AppealError, AppealLimits, ContestRef, Outcome, OverturnRefusal};

const WORDS: &str = "That was not me.";
const WHY: &str = "The record supports it.";

/// A moderator's unhide of the hide `hide`, as the moderation service writes it: no appeal, no decider lock.
fn unhide_by_hand(w: &World, hide: &TombstoneEventId, by: &Person) -> TombstoneEvent {
    let of = w
        .store
        .tombstone_event(hide)
        .expect("a read")
        .expect("a hide");
    TombstoneEvent {
        id: TombstoneEventId::new(),
        act: TombstoneAct::Unhide,
        // An unhide by hand is an act of its own.
        act_id: TombstoneActId::new(),
        at: now(),
        category: None,
        note: None,
        by: inklkunn_store::Actor {
            account: by.id.clone(),
            rung: Role::Maintainer,
        },
        of_event: Some(hide.clone()),
        declined_transition: None,
        ..of
    }
}

/// How many of the hides are in force.
fn in_force(w: &World, block: &str) -> Vec<TombstoneEventId> {
    w.tombstones
        .of_change(&w.wiki.id, block)
        .expect("a read")
        .marks()
        .iter()
        .map(|m| m.event.clone())
        .collect()
}

/// Is there an event that names an appeal (the unhide of an overturn) among the events of this change?
fn named_by_an_appeal(w: &World, block: &str) -> bool {
    w.store
        .tombstone_events_of_id(
            &w.wiki.id,
            inklkunn_store::HiddenItemKind::PageRevision,
            block,
        )
        .expect("a read")
        .iter()
        .any(|e| e.appeal.is_some())
}

/// The world's appeal service, whose audit trail runs `hook` on the event of the decision (the moment between the checks and the
/// records), and the trail.
fn service_hooked(
    w: &World,
    hook: impl Fn() + Send + Sync + 'static,
) -> (AppealService, Arc<inklkunn_store::MemoryAudit>) {
    w.service_with(move |event| {
        if event.action == "appeal-decided" && event.outcome == AuditOutcome::Allowed {
            hook();
        }
    })
}

#[test]
fn a_moderators_unhide_during_the_decision_leaves_no_page_unhidden_under_an_open_appeal() {
    let block = "ab".repeat(32);
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        // One act hid a change in three records, one for each page it wrote.
        let act = TombstoneActId::new();
        let first = hide_page(&w, &mover, &author, &block, "pages/One.md", now(), &act);
        let second = hide_page(&w, &mover, &author, &block, "pages/Two.md", now(), &act);
        let third = hide_page(&w, &mover, &author, &block, "pages/Three.md", now(), &act);
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: first.to_string(),
        };
        let filed = w
            .appeals
            .file(&author.principal, &w.wiki, &contest, WORDS)
            .unwrap();
        // Between the decider's checks and the records, a moderator unhides the page the appeal is against.
        let store = w.store.clone();
        let by_hand = unhide_by_hand(&w, &first, &mover);
        let (service, trail) = service_hooked(&w, move || {
            store.record_tombstone_event(&by_hand).expect("an unhide");
        });
        let refused = service
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap_err();
        assert_eq!(refused, AppealError::Changed, "{kind:?}");
        assert_eq!(
            in_force(&w, &block),
            vec![second.clone(), third.clone()],
            "{kind:?}: the two other pages are still hidden: the overturn wrote nothing"
        );
        assert!(
            !named_by_an_appeal(&w, &block),
            "{kind:?}: no unhide of the overturn is left"
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open, "{kind:?}");
        // The trail says the record failed (F-4).
        let failed: Vec<_> = trail
            .events()
            .into_iter()
            .filter(|e| e.action == "appeal-decided" && e.outcome == AuditOutcome::Failed)
            .collect();
        assert_eq!(failed.len(), 1, "{kind:?}");
    }
}

#[test]
fn a_moderators_unhide_of_a_page_the_decision_does_not_name_is_not_a_reason_to_refuse_it() {
    // The siblings are read when the decision is checked: an unhide that landed before then is simply not one of them.
    let block = "cd".repeat(32);
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        let act = TombstoneActId::new();
        let first = hide_page(&w, &mover, &author, &block, "pages/One.md", now(), &act);
        let second = hide_page(&w, &mover, &author, &block, "pages/Two.md", now(), &act);
        let third = hide_page(&w, &mover, &author, &block, "pages/Three.md", now(), &act);
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: first.to_string(),
        };
        let filed = w
            .appeals
            .file(&author.principal, &w.wiki, &contest, WORDS)
            .unwrap();
        w.store
            .record_tombstone_event(&unhide_by_hand(&w, &second, &mover))
            .unwrap();
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("decided");
        assert!(in_force(&w, &block).is_empty(), "{kind:?}: {third}");
        assert!(matches!(
            w.appeal(&filed.id).state,
            AppealState::Overturned(..)
        ));
    }
}

#[test]
fn an_unhide_and_a_decision_at_once_either_all_of_the_overturn_lands_or_none_of_it_does() {
    for round in 0..40 {
        let w = World::with(
            Kind::Sqlite,
            inklkunn_store::Visibility::Public,
            None,
            AppealLimits::default(),
            steady,
        );
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        let block = format!("{round:02x}").repeat(32);
        let act = TombstoneActId::new();
        let first = hide_page(&w, &mover, &author, &block, "pages/One.md", T0, &act);
        let second = hide_page(&w, &mover, &author, &block, "pages/Two.md", T0, &act);
        let third = hide_page(&w, &mover, &author, &block, "pages/Three.md", T0, &act);
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: first.to_string(),
        };
        let filed = w
            .appeals
            .file(&author.principal, &w.wiki, &contest, WORDS)
            .unwrap();
        // One moderator unhides the page the appeal is against while the decider overturns it (and, with it, the whole act).
        let by_hand = unhide_by_hand(&w, &first, &mover);
        let gate = Arc::new(Barrier::new(2));
        let (g1, g2) = (gate.clone(), gate);
        let store = w.store.clone();
        let moderator = thread::spawn(move || {
            g1.wait();
            store.record_tombstone_event(&by_hand)
        });
        let (appeals, wiki, principal, id) = (
            w.appeals.clone(),
            w.wiki.clone(),
            admin.principal.clone(),
            filed.id.clone(),
        );
        let decider = thread::spawn(move || {
            g2.wait();
            appeals.decide(&principal, &wiki, &id, Outcome::Overturned, WHY)
        });
        let unhid = moderator.join().expect("a thread");
        let decided = decider.join().expect("a thread");
        match decided {
            Ok(_) => {
                // The decision landed whole: every page of the act is unhidden, and the appeal says so.
                assert!(
                    in_force(&w, &block).is_empty(),
                    "round {round}: the whole act is restored"
                );
                assert!(matches!(
                    w.appeal(&filed.id).state,
                    AppealState::Overturned(..)
                ));
                assert!(unhid.is_err(), "round {round}: the moderator was late");
            }
            Err(why) => {
                assert!(
                    matches!(
                        why,
                        AppealError::Changed | AppealError::Overturn(OverturnRefusal::HidingEnded)
                    ),
                    "round {round}: {why:?}"
                );
                assert!(unhid.is_ok(), "round {round}");
                // Nothing of the overturn was written: the other pages are still hidden, and no unhide names the appeal.
                assert_eq!(
                    in_force(&w, &block),
                    vec![second, third],
                    "round {round}: the siblings are not left unhidden under an open appeal"
                );
                assert!(!named_by_an_appeal(&w, &block), "round {round}");
                assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
            }
        }
    }
}

#[test]
fn a_proposal_that_moves_on_during_the_decision_of_a_decline_leaves_the_appeal_open_and_nothing_written()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let other = w.person("Eve", Some(Role::Admin));
        let s = super::scenes::scene(&w, AppealedKind::Decline);
        let proposal = s.proposal.clone().unwrap();
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let steps_before = w.store.transitions(&proposal, None).unwrap().len();
        // Between the checks and the records somebody reopens it by hand (and withdraws it).
        let store = w.store.clone();
        let who = other.id.clone();
        let hand = proposal.clone();
        let (service, _) = service_hooked(&w, move || {
            store
                .change_proposal(&inklkunn_store::ProposalChange {
                    transition: inklkunn_store::ProposalTransition {
                        proposal: hand.clone(),
                        op: inklkunn_store::ProposalOp::Reopen,
                        from: Some(ProposalState::Declined),
                        to: ProposalState::HeldForReview,
                        actor: who.clone(),
                        reason: Some("By hand.".to_owned()),
                        at: T0,
                    },
                    text: None,
                    revision: None,
                    superseded_by: None,
                })
                .expect("a reopening");
        });
        let refused = service
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap_err();
        assert_eq!(
            refused,
            AppealError::Overturn(OverturnRefusal::ProposalMovedOn),
            "{kind:?}"
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open, "{kind:?}");
        assert_eq!(
            w.store.transitions(&proposal, None).unwrap().len(),
            steps_before + 1,
            "{kind:?}: only the reopening by hand is in the log"
        );
    }
}

#[test]
fn a_hide_that_declined_a_proposal_is_unhidden_and_the_proposal_reopened_together_or_not_at_all() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        let proposal = w.proposal(&author, "Home");
        let decline = w.decline(&proposal, &mover);
        let position: i64 = decline.id.parse().unwrap();
        let hide = hide_revision(&w, &mover, &author, &proposal, Some(position));
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: hide.to_string(),
        };
        let filed = w
            .appeals
            .file(&author.principal, &w.wiki, &contest, WORDS)
            .unwrap();
        // A moderator unhides it between the checks and the records: the reopening is not left written.
        let store = w.store.clone();
        let by_hand = unhide_by_hand(&w, &hide, &mover);
        let (service, _) = service_hooked(&w, move || {
            store.record_tombstone_event(&by_hand).expect("an unhide");
        });
        assert_eq!(
            service
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Overturned,
                    WHY
                )
                .unwrap_err(),
            AppealError::Changed,
            "{kind:?}"
        );
        assert_eq!(w.state_of(&proposal), ProposalState::Declined, "{kind:?}");
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open, "{kind:?}");
        // Decided again, with the hide gone: moot, and the proposal is as it was.
        assert_eq!(
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Overturned,
                    WHY
                )
                .unwrap_err(),
            AppealError::Overturn(OverturnRefusal::HidingEnded),
            "{kind:?}"
        );
    }
}
