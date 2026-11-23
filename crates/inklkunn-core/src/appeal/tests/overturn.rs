//! What an overturn does, for each of the five kinds (RFC 006 D-9; handoff G-T5; acceptance criterion 9): **the action is reversed,
//! in the same step, as its own record**; and what a later change makes of an overturn (it is refused by name, nothing is written,
//! and the appeal can still be upheld).

use inklkunn_store::{
    AppealState, AppealedKind, GrantDecisionRecord, HiddenItem, HiddenItemKind, HiddenParts,
    MeasureEvent, ProposalId, ProposalOp, ProposalState, ReasonCategory, Role, TombstoneAct,
    TombstoneEvent, TombstoneEventId,
};

use super::scenes::scene;
use super::support::{DAY, KINDS, T0, World, now, set_now};
use crate::appeal::{AppealError, ContestRef, Outcome, OverturnRefusal};
use crate::proposal::machine;

const WORDS: &str = "That was not me.";
const WHY: &str = "The record supports it.";

/// File the appeal of the scene's appellant and overturn it as `decider`.
fn overturned(
    w: &World,
    appellant: &super::support::Person,
    contest: &ContestRef,
    decider: &super::support::Person,
) -> Result<crate::appeal::OverturnMade, AppealError> {
    let filed = w
        .appeals
        .file(&appellant.principal, &w.wiki, contest, WORDS)
        .expect("filed");
    w.appeals.decide(
        &decider.principal,
        &w.wiki,
        &filed.id,
        Outcome::Overturned,
        WHY,
    )
}

#[test]
fn an_overturned_decline_reopens_the_proposal_which_is_held_for_review_again_and_the_appeal_names_the_step()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Decline);
        let proposal = s.proposal.clone().unwrap();
        assert_eq!(w.state_of(&proposal), ProposalState::Declined);
        let made = overturned(&w, &s.appellant, &s.contest, &admin).expect("overturned");
        assert_eq!(made.outcome, Outcome::Overturned);
        assert_eq!(made.reopened, Some(proposal.clone()));
        assert_eq!(w.state_of(&proposal), ProposalState::HeldForReview);
        // The machine says what can be done to it now: it can be accepted, declined, and so on.
        assert!(machine::legal_in(ProposalState::HeldForReview).contains(&ProposalOp::Accept));
        let record = w.store.proposal(&proposal, None).unwrap().unwrap();
        assert_eq!(record.closed_at, None, "it is open again");
        // The reopening is the proposal log's own step, by the decider, with the reason.
        let log = w.store.transitions(&proposal, None).unwrap();
        let last = log.last().unwrap();
        assert_eq!(last.op, ProposalOp::Reopen);
        assert_eq!(
            (last.from, last.to),
            (Some(ProposalState::Declined), ProposalState::HeldForReview)
        );
        assert_eq!(last.actor, admin.id);
        assert_eq!(last.reason.as_deref(), Some(WHY));
        // The appeal names that step.
        let position = w
            .store
            .last_transition_position(&proposal)
            .unwrap()
            .unwrap();
        let filed = w
            .store
            .appeals_about(&w.wiki.id, &s.contest.action())
            .unwrap();
        let AppealState::Overturned(decided, named) = &filed.first().unwrap().state else {
            panic!("overturned: {filed:?}");
        };
        assert_eq!(decided.by, admin.id);
        assert_eq!(named, &position.to_string());
        // The decline is no longer in force, so there is nothing more to contest.
        assert_eq!(
            w.appeals.offer(&s.appellant.principal, &w.wiki, &s.contest),
            crate::appeal::Offer::Nothing
        );
    }
}

#[test]
fn an_overturned_decline_is_refused_by_name_when_the_proposal_moved_on_or_its_text_is_still_hidden_and_the_appeal_stays_open()
 {
    for kind in KINDS {
        // Moved on: somebody brought it back some other way.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Decline);
        let proposal = s.proposal.clone().unwrap();
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        w.store
            .change_proposal(&inklkunn_store::ProposalChange {
                transition: inklkunn_store::ProposalTransition {
                    proposal: proposal.clone(),
                    op: ProposalOp::Reopen,
                    from: Some(ProposalState::Declined),
                    to: ProposalState::HeldForReview,
                    actor: admin.id.clone(),
                    reason: None,
                    at: T0,
                },
                text: None,
                revision: None,
                superseded_by: None,
            })
            .unwrap();
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
            AppealError::Overturn(OverturnRefusal::ProposalMovedOn)
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Upheld,
                "It is open again already.",
            )
            .expect("it can still be decided, upheld");

        // Still hidden: a proposal nobody can read cannot be reviewed.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Decline);
        let proposal = s.proposal.clone().unwrap();
        hide_revision(&w, &s.actor, &s.appellant, &proposal, None);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
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
            AppealError::Overturn(OverturnRefusal::StillHidden)
        );
        assert_eq!(
            w.state_of(&proposal),
            ProposalState::Declined,
            "nothing was written"
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
    }
}

/// A hiding of the content of the proposal's first revision, by `by` about `author`, as one act that declined the proposal at
/// `declined` when it is one (the position of the decline in the proposal log).
pub(super) fn hide_revision(
    w: &World,
    by: &super::support::Person,
    author: &super::support::Person,
    proposal: &ProposalId,
    declined: Option<i64>,
) -> TombstoneEventId {
    let id = TombstoneEventId::new();
    w.store
        .record_tombstone_event(&TombstoneEvent {
            id: id.clone(),
            wiki: w.wiki.id.clone(),
            at: now(),
            act: TombstoneAct::Hide,
            item: HiddenItem {
                kind: HiddenItemKind::ProposalRevision,
                id: proposal.to_string(),
                reference: Some("1".to_owned()),
            },
            parts: HiddenParts {
                content: true,
                summary: false,
                author: false,
            },
            category: Some(ReasonCategory::PersonalData),
            note: None,
            affected: Some(author.id.clone()),
            by: inklkunn_store::Actor {
                account: by.id.clone(),
                rung: Role::Maintainer,
            },
            policy_block: None,
            of_event: None,
            appeal: None,
            declined_transition: declined,
        })
        .unwrap();
    id
}

#[test]
fn an_overturned_tombstone_is_unhidden_by_an_unhide_that_names_the_appeal() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Tombstone);
        let comment = s.comment.clone().unwrap();
        assert!(w.hidden(&comment), "hidden to begin with");
        let made = overturned(&w, &s.appellant, &s.contest, &admin).expect("overturned");
        assert_eq!(made.reopened, None);
        assert!(!w.hidden(&comment), "restored");
        // The unhide is its own record: of the hide, by the decider, naming the appeal.
        let events = w
            .store
            .tombstone_events_of(
                &w.wiki.id,
                &HiddenItem {
                    kind: HiddenItemKind::ProposalComment,
                    id: comment.to_string(),
                    reference: None,
                },
            )
            .unwrap();
        let [hide, unhide] = events.as_slice() else {
            panic!("a hide and an unhide: {events:?}");
        };
        assert_eq!(unhide.act, TombstoneAct::Unhide);
        assert_eq!(unhide.of_event.as_ref(), Some(&hide.id));
        assert_eq!(unhide.by.account, admin.id);
        assert_eq!(unhide.parts, hide.parts, "exactly what was hidden");
        let appeals = w
            .store
            .appeals_about(&w.wiki.id, &s.contest.action())
            .unwrap();
        let appeal = appeals.first().unwrap();
        assert_eq!(unhide.appeal.as_ref(), Some(&appeal.id));
        let AppealState::Overturned(_, named) = &appeal.state else {
            panic!("overturned");
        };
        assert_eq!(named, &unhide.id.to_string());
        // Nothing is left to contest.
        assert_eq!(
            w.appeals.offer(&s.appellant.principal, &w.wiki, &s.contest),
            crate::appeal::Offer::Nothing
        );
    }
}

/// A hiding of the content of one page of a change (the block), recorded at `at`: a change's text is recorded once for each page it wrote.
pub(super) fn hide_page(
    w: &World,
    by: &super::support::Person,
    author: &super::support::Person,
    block: &str,
    path: &str,
    at: i64,
) -> TombstoneEventId {
    let id = TombstoneEventId::new();
    w.store
        .record_tombstone_event(&TombstoneEvent {
            id: id.clone(),
            wiki: w.wiki.id.clone(),
            at,
            act: TombstoneAct::Hide,
            item: HiddenItem {
                kind: HiddenItemKind::PageRevision,
                id: block.to_owned(),
                reference: Some(path.to_owned()),
            },
            parts: HiddenParts {
                content: true,
                summary: false,
                author: false,
            },
            category: Some(ReasonCategory::Copyright),
            note: None,
            affected: Some(author.id.clone()),
            by: inklkunn_store::Actor {
                account: by.id.clone(),
                rung: Role::Maintainer,
            },
            policy_block: None,
            of_event: None,
            appeal: None,
            declined_transition: None,
        })
        .unwrap();
    id
}

#[test]
fn an_overturned_tombstone_restores_every_hide_the_same_act_made_and_not_another_act_s() {
    let block = "ab".repeat(32);
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let other = w.person("Olly", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        // One act hid a change's text in two records (one for each page it wrote); another moderator, another act, hid a third.
        let first = hide_page(&w, &mover, &author, &block, "pages/One.md", now() - 2);
        let second = hide_page(&w, &mover, &author, &block, "pages/Two.md", now() - 1);
        let elsewhere = hide_page(&w, &other, &author, &block, "pages/Three.md", now() - 1);
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: first.to_string(),
        };
        overturned(&w, &author, &contest, &admin).expect("overturned");
        let marks: Vec<TombstoneEventId> = w
            .tombstones
            .of_change(&w.wiki.id, &block)
            .unwrap()
            .marks()
            .iter()
            .map(|m| m.event.clone())
            .collect();
        assert_eq!(
            marks,
            vec![elsewhere],
            "the two records of one act are restored ({first} and {second}); the other act's stays"
        );
    }
}

#[test]
fn an_overturned_hide_that_declined_a_proposal_reopens_it_when_it_is_as_the_hide_left_it() {
    for kind in KINDS {
        // The proposal was declined by the act that hid its text.
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
        let made = overturned(&w, &author, &contest, &admin).expect("overturned");
        assert_eq!(
            made.reopened,
            Some(proposal.clone()),
            "the person is made whole in one decision"
        );
        assert_eq!(w.state_of(&proposal), ProposalState::HeldForReview);
        assert!(
            w.tombstones
                .proposal_revision(&w.wiki.id, &proposal, 1)
                .unwrap()
                .content()
                .is_none()
        );

        // Declined again since (a later step): left as it is.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        let proposal = w.proposal(&author, "Home");
        let decline = w.decline(&proposal, &mover);
        let position: i64 = decline.id.parse().unwrap();
        let hide = hide_revision(&w, &mover, &author, &proposal, Some(position));
        // It was reopened and withdrawn by hand: not as the hide left it.
        for (op, from, to) in [
            (
                ProposalOp::Reopen,
                ProposalState::Declined,
                ProposalState::HeldForReview,
            ),
            (
                ProposalOp::Withdraw,
                ProposalState::HeldForReview,
                ProposalState::Withdrawn,
            ),
        ] {
            w.store
                .change_proposal(&inklkunn_store::ProposalChange {
                    transition: inklkunn_store::ProposalTransition {
                        proposal: proposal.clone(),
                        op,
                        from: Some(from),
                        to,
                        actor: author.id.clone(),
                        reason: Some("x".to_owned()),
                        at: T0,
                    },
                    text: None,
                    revision: None,
                    superseded_by: None,
                })
                .unwrap();
        }
        let contest = ContestRef {
            kind: AppealedKind::Tombstone,
            id: hide.to_string(),
        };
        let made = overturned(&w, &author, &contest, &admin).expect("overturned");
        assert_eq!(made.reopened, None);
        assert_eq!(w.state_of(&proposal), ProposalState::Withdrawn);
    }
}

#[test]
fn an_overturned_restriction_ends_it_with_a_reversed_event_and_the_person_may_write_again() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        assert!(w.restricted(&s.appellant));
        let wiki = crate::Resource::Wiki {
            wiki: w.wiki.id.clone(),
        };
        assert!(
            !w.authorizer
                .authorize(&s.appellant.principal, crate::Action::Propose, &wiki)
                .is_allowed()
        );
        overturned(&w, &s.appellant, &s.contest, &admin).expect("overturned");
        assert!(!w.restricted(&s.appellant), "the restriction ended");
        assert!(
            w.authorizer
                .authorize(&s.appellant.principal, crate::Action::Propose, &wiki)
                .is_allowed(),
            "and the person may propose again"
        );
        let events = w
            .store
            .restriction_events(&w.wiki.id, &s.appellant.id)
            .unwrap();
        let [set, reversed] = events.as_slice() else {
            panic!("two events: {events:?}");
        };
        assert_eq!(reversed.event, MeasureEvent::Reversed);
        assert_eq!(reversed.of_event.as_ref(), Some(&set.id));
        assert_eq!(reversed.by.account, admin.id);
        assert_eq!(reversed.reason.as_deref(), Some(WHY));
        let appeal = w
            .store
            .appeals_about(&w.wiki.id, &s.contest.action())
            .unwrap()
            .remove(0);
        assert_eq!(reversed.appeal.as_ref(), Some(&appeal.id));
    }
}

#[test]
fn an_overturned_restriction_that_was_renewed_since_ends_the_one_in_force_and_one_that_has_ended_is_moot()
 {
    for kind in KINDS {
        // Renewed after the appeal: the person is restricted under the renewal, and that is what is reversed.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        w.renew(&s.actor, &s.appellant, &s.contest, 5);
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("the renewed restriction is the one reversed");
        assert!(!w.restricted(&s.appellant));

        // Ended by the clock: nothing to reverse, said by name, and the appeal can still be upheld.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        set_now(T0 + 4 * DAY);
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
            AppealError::Overturn(OverturnRefusal::RestrictionEnded)
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Upheld,
                "It has run out.",
            )
            .unwrap();
        set_now(T0);

        // Lifted by a person: the same.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let first = inklkunn_store::RestrictionEventId::parse(&s.contest.id).unwrap();
        w.store
            .record_restriction_event(&inklkunn_store::RestrictionEvent {
                id: inklkunn_store::RestrictionEventId::new(),
                wiki: w.wiki.id.clone(),
                at: T0,
                event: MeasureEvent::Lifted,
                account: s.appellant.id.clone(),
                by: inklkunn_store::Actor {
                    account: admin.id.clone(),
                    rung: Role::Admin,
                },
                policy_block: None,
                reason: None,
                expires_at: None,
                of_event: Some(first),
                appeal: None,
            })
            .unwrap();
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
            AppealError::Overturn(OverturnRefusal::RestrictionEnded)
        );
    }
}

#[test]
fn an_overturned_protection_ends_it_and_one_that_has_ended_is_moot() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Protection);
        assert!(w.protected());
        overturned(&w, &s.appellant, &s.contest, &admin).expect("overturned");
        assert!(!w.protected(), "the protection ended");
        let events = w.store.protection_events(&w.wiki.id, 10).unwrap();
        let reversed = events
            .iter()
            .find(|e| e.event == MeasureEvent::Reversed)
            .expect("a reversed event");
        assert_eq!(reversed.by.account, admin.id);
        assert_eq!(reversed.reason.as_deref(), Some(WHY));
        let appeal = w
            .store
            .appeals_about(&w.wiki.id, &s.contest.action())
            .unwrap()
            .remove(0);
        assert_eq!(reversed.appeal.as_ref(), Some(&appeal.id));
        assert_eq!(reversed.target, "Home");

        // Ended by the clock.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Protection);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        set_now(T0 + 4 * DAY);
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
            AppealError::Overturn(OverturnRefusal::ProtectionEnded)
        );
        set_now(T0);
    }
}

#[test]
fn an_overturned_grant_decision_restores_the_role_and_a_later_change_makes_it_moot() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Grant);
        assert_eq!(w.grant_of(&s.appellant), Some(Role::Reviewer));
        overturned(&w, &s.appellant, &s.contest, &admin).expect("overturned");
        assert_eq!(w.grant_of(&s.appellant), Some(Role::Maintainer), "restored");
        let decisions = w
            .store
            .grant_decisions(&w.wiki.id, Some(&s.appellant.id), 10)
            .unwrap();
        let restoring: &GrantDecisionRecord = decisions.first().expect("the newest first");
        assert_eq!(
            (restoring.before, restoring.after),
            (Some(Role::Reviewer), Some(Role::Maintainer))
        );
        assert_eq!(restoring.by.as_ref().map(|a| &a.account), Some(&admin.id));
        assert_eq!(restoring.reason.as_deref(), Some(WHY));
        assert!(restoring.appeal.is_some());

        // A revocation is restored as well.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let target = w.person("Rae", Some(Role::Reviewer));
        let contest = w.regrade(Some(&admin), &target, Some(Role::Reviewer), None);
        let other_admin = w.person("Eve", Some(Role::Admin));
        overturned(&w, &target, &contest, &other_admin).expect("overturned");
        assert_eq!(w.grant_of(&target), Some(Role::Reviewer));

        // A later change is the person's to keep: the overturn is refused by name.
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Grant);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        w.regrade(
            Some(&admin),
            &s.appellant,
            Some(Role::Reviewer),
            Some(Role::Contributor),
        );
        // The decision is no longer the person's standing: nothing to contest, and the decider is told so.
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
            AppealError::Overturn(OverturnRefusal::RoleChanged)
        );
        assert_eq!(w.grant_of(&s.appellant), Some(Role::Contributor));
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
    }
}

#[test]
fn a_decision_that_raised_a_role_or_gave_a_first_one_is_no_ones_grievance() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let target = w.person("Rae", Some(Role::Contributor));
        let raised = w.regrade(
            Some(&admin),
            &target,
            Some(Role::Contributor),
            Some(Role::Reviewer),
        );
        let newcomer = w.person("New", None);
        let first = w.regrade(Some(&admin), &newcomer, None, Some(Role::Contributor));
        for (who, contest) in [(&target, &raised), (&newcomer, &first)] {
            assert_eq!(
                w.appeals.offer(&who.principal, &w.wiki, contest),
                crate::appeal::Offer::Nothing
            );
            assert_eq!(
                w.appeals
                    .file(&who.principal, &w.wiki, contest, WORDS)
                    .unwrap_err(),
                AppealError::NotAffected
            );
        }
    }
}

#[test]
fn a_maintainer_who_decides_may_restore_only_what_the_rules_let_a_maintainer_grant() {
    let policy = "rungs:\n  decide_appeal: maintainer\n  grant_up_to: reviewer\n";
    for kind in KINDS {
        let w = World::under(kind, policy);
        let admin = w.person("Ada", Some(Role::Admin));
        let decider = w.person("Mia", Some(Role::Maintainer));
        let target = w.person("Pat", Some(Role::Maintainer));
        let contest = w.regrade(
            Some(&admin),
            &target,
            Some(Role::Maintainer),
            Some(Role::Reviewer),
        );
        let filed = w
            .appeals
            .file(&target.principal, &w.wiki, &contest, WORDS)
            .unwrap();
        // Restoring maintainer is above a maintainer's reach: an admin decides this one.
        let refused = w
            .appeals
            .decide(
                &decider.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .unwrap_err();
        let AppealError::Overturn(OverturnRefusal::AboveReach { up_to }) = refused else {
            panic!("above reach: {refused:?}");
        };
        assert_eq!(up_to.highest(), Some(Role::Reviewer));
        assert_eq!(
            w.grant_of(&target),
            Some(Role::Reviewer),
            "nothing was written"
        );
        assert_eq!(w.appeal(&filed.id).state, AppealState::Open);
        // The maintainer may uphold; an admin may overturn.
        let third = w.person("Eve", Some(Role::Admin));
        w.appeals
            .decide(
                &third.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                WHY,
            )
            .expect("an admin restores it");
        assert_eq!(w.grant_of(&target), Some(Role::Maintainer));

        // Within reach: a maintainer restores a reviewer who was made a contributor.
        let w = World::under(kind, policy);
        let admin = w.person("Ada", Some(Role::Admin));
        let decider = w.person("Mia", Some(Role::Maintainer));
        let target = w.person("Pat", Some(Role::Reviewer));
        let contest = w.regrade(
            Some(&admin),
            &target,
            Some(Role::Reviewer),
            Some(Role::Contributor),
        );
        overturned(&w, &target, &contest, &decider).expect("within reach");
        assert_eq!(w.grant_of(&target), Some(Role::Reviewer));
    }
}

#[test]
fn a_hide_that_was_unhidden_by_someone_else_meanwhile_is_moot() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Tombstone);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let comment = s.comment.clone().unwrap();
        let hide = TombstoneEventId::parse(&s.contest.id).unwrap();
        w.store
            .record_tombstone_event(&TombstoneEvent {
                id: TombstoneEventId::new(),
                wiki: w.wiki.id.clone(),
                at: T0,
                act: TombstoneAct::Unhide,
                item: HiddenItem {
                    kind: HiddenItemKind::ProposalComment,
                    id: comment.to_string(),
                    reference: None,
                },
                parts: HiddenParts {
                    content: true,
                    summary: false,
                    author: false,
                },
                category: None,
                note: None,
                affected: Some(s.appellant.id.clone()),
                by: inklkunn_store::Actor {
                    account: admin.id.clone(),
                    rung: Role::Admin,
                },
                policy_block: None,
                of_event: Some(hide),
                appeal: None,
                declined_transition: None,
            })
            .unwrap();
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
            AppealError::Overturn(OverturnRefusal::HidingEnded)
        );
    }
}
