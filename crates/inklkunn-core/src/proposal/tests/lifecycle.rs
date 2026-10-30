//! The life of a proposal, step by step, on both stores: what each act records, in what state it leaves the
//! proposal, and what it refuses.

use inklkunn_store::{DecisionKind, ProposalFilter, ProposalOp, ProposalState};

use super::support::{T0, edit, on_both, set_now};
use crate::proposal::{Invalid, NewComment, ProposalError};

#[test]
fn a_proposal_is_drafted_submitted_reviewed_revised_and_sent_back() {
    on_both(|e| {
        let w = &e.wiki;
        let (author, reviewer, maintainer) = (&e.author, &e.reviewer, &e.maintainer);
        let s = &e.service;

        // A draft: the author's, unsent.
        let p = s
            .create(
                &author.principal,
                w,
                None,
                edit("Fix a typo", "pages/home.md", "v1"),
            )
            .unwrap();
        assert_eq!(p.state, ProposalState::Draft);
        assert_eq!(p.view, "heads/main");
        assert_eq!(p.author, author.id);
        assert_eq!(p.submitted_at, None);
        assert_eq!(p.created_at, T0);

        // Submitted: held for review, with revision 1 from the working copy.
        set_now(T0 + 10);
        let held = s.submit(&author.principal, w, &p.id).unwrap();
        assert_eq!(held.state, ProposalState::HeldForReview);
        assert_eq!(held.submitted_at, Some(T0 + 10));
        let revisions = s.revisions(&author.principal, w, &p.id).unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions.first().map(|r| r.seq), Some(1));
        assert!(
            s.draft(&author.principal, w, &p.id).is_err(),
            "the working copy ended with the submission"
        );

        // Review: a comment (anchored), a decision on revision 1.
        set_now(T0 + 20);
        let c = s
            .comment(
                &reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: "Is this the right spelling?".into(),
                    parent: None,
                    anchor: Some(inklkunn_store::Anchor {
                        page: "pages/home.md".into(),
                        block_id: None,
                        quote: Some(inklkunn_store::Quote {
                            exact: "v1".into(),
                            prefix: String::new(),
                            suffix: String::new(),
                        }),
                        ordinal: 0,
                    }),
                },
            )
            .unwrap();
        assert_eq!(c.revision, 1);
        s.comment(
            &author.principal,
            w,
            &p.id,
            NewComment {
                body: "Yes.".into(),
                parent: Some(c.id.clone()),
                anchor: None,
            },
        )
        .unwrap();
        let d1 = s
            .decide(
                &reviewer.principal,
                w,
                &p.id,
                1,
                DecisionKind::RequestChanges,
                "Add a source",
            )
            .unwrap();
        assert_eq!(d1.revision, 1);

        // A revision: held for review again; the decision on revision 1 stays, bound to revision 1.
        set_now(T0 + 30);
        let mut second = edit("Fix two typos", "pages/home.md", "v2");
        second.sources = vec!["https://example.org/source".into()];
        let revised = s.revise(&author.principal, w, &p.id, second).unwrap();
        assert_eq!(revised.state, ProposalState::HeldForReview);
        assert_eq!(revised.summary, "Fix two typos");
        let decisions = s.decisions(&author.principal, w, &p.id).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions.first().map(|d| d.revision), Some(1));
        let d2 = s
            .decide(&reviewer.principal, w, &p.id, 2, DecisionKind::Approve, "")
            .unwrap();
        assert_eq!(
            d2.revision, 2,
            "the same reviewer decides again on the new revision"
        );
        assert_eq!(s.comments(&author.principal, w, &p.id).unwrap().len(), 2);

        // Sent back by an acceptor with a reason; the proposer revises; the acceptor declines.
        set_now(T0 + 40);
        let back = s
            .ask_for_revision(
                &maintainer.principal,
                w,
                &p.id,
                "Please rebase on the new intro.",
            )
            .unwrap();
        assert_eq!(back.state, ProposalState::NeedsRevision);
        let again = s
            .revise(
                &author.principal,
                w,
                &p.id,
                edit("Fix two typos", "pages/home.md", "v3"),
            )
            .unwrap();
        assert_eq!(again.state, ProposalState::HeldForReview);
        set_now(T0 + 50);
        let declined = s
            .decline(
                &maintainer.principal,
                w,
                &p.id,
                "The page is being rewritten; please wait.",
            )
            .unwrap();
        assert_eq!(declined.state, ProposalState::Declined);
        assert_eq!(declined.closed_at, Some(T0 + 50));

        // The log tells it all, with the reasons.
        let log = s.transitions(&author.principal, w, &p.id).unwrap();
        let shape: Vec<_> = log.iter().map(|t| (t.op, t.from, t.to)).collect();
        use ProposalOp::{AskForRevision, Create, Decline, Revise, Submit};
        use ProposalState::{Declined, Draft, HeldForReview, NeedsRevision};
        assert_eq!(
            shape,
            vec![
                (Create, None, Draft),
                (Submit, Some(Draft), HeldForReview),
                (Revise, Some(HeldForReview), HeldForReview),
                (AskForRevision, Some(HeldForReview), NeedsRevision),
                (Revise, Some(NeedsRevision), HeldForReview),
                (Decline, Some(HeldForReview), Declined),
            ]
        );
        assert_eq!(
            log.get(3).and_then(|t| t.reason.as_deref()),
            Some("Please rebase on the new intro.")
        );
        assert_eq!(
            log.get(5).map(|t| (&t.actor, t.reason.is_some())),
            Some((&maintainer.id, true))
        );
        assert_eq!(s.revisions(&author.principal, w, &p.id).unwrap().len(), 3);
    });
}

#[test]
fn the_base_is_captured_from_the_views_tip_and_never_moves_silently() {
    on_both(|e| {
        let (w, s, a) = (&e.wiki, &e.service, &e.author);
        e.bases.move_to(5);
        let p = s
            .create(&a.principal, w, None, edit("One", "pages/a.md", "x"))
            .unwrap();
        let first = s.draft(&a.principal, w, &p.id).unwrap().base;
        assert_eq!(
            first,
            e.bases.current(),
            "the tip at the time the form opened"
        );

        // The view moves; saving the draft keeps the base the text was written against.
        e.bases.move_to(9);
        s.save_draft(&a.principal, w, &p.id, edit("One", "pages/a.md", "y"))
            .unwrap();
        assert_eq!(s.draft(&a.principal, w, &p.id).unwrap().base, first);
        // A caller that captured the base itself (the form, when it opened) says so.
        let captured = s.capture_base(w, None).unwrap();
        assert_eq!(captured, e.bases.current());
        let mut with_base = edit("One", "pages/a.md", "z");
        with_base.base = Some(captured.clone());
        s.save_draft(&a.principal, w, &p.id, with_base).unwrap();
        assert_eq!(s.draft(&a.principal, w, &p.id).unwrap().base, captured);

        // A revision keeps the previous revision's base unless it names a new one.
        s.submit(&a.principal, w, &p.id).unwrap();
        e.bases.move_to(20);
        s.revise(&a.principal, w, &p.id, edit("One", "pages/a.md", "w"))
            .unwrap();
        let revisions = s.revisions(&a.principal, w, &p.id).unwrap();
        assert_eq!(revisions.get(1).map(|r| &r.content.base), Some(&captured));
        let mut rebased = edit("One", "pages/a.md", "v");
        rebased.base = Some(super::support::base(30));
        s.revise(&a.principal, w, &p.id, rebased).unwrap();
        let revisions = s.revisions(&a.principal, w, &p.id).unwrap();
        assert_eq!(
            revisions.get(2).map(|r| &r.content.base),
            Some(&super::support::base(30))
        );

        // A base that is not a pair of ids is refused.
        let mut junk = edit("One", "pages/a.md", "u");
        junk.base = Some(inklkunn_store::Base {
            state_id: "nope".into(),
            block_id: "nope".into(),
        });
        assert_eq!(
            s.revise(&a.principal, w, &p.id, junk),
            Err(ProposalError::Invalid(Invalid::BadBase))
        );
    });
}

#[test]
fn a_view_that_is_not_there_or_a_repository_that_cannot_answer_creates_nothing() {
    on_both(|e| {
        let (w, s, a) = (&e.wiki, &e.service, &e.author);
        e.bases
            .fail_with(Some(crate::proposal::BaseError::NoSuchView(
                "heads/nope".into(),
            )));
        assert_eq!(
            s.create(
                &a.principal,
                w,
                Some("heads/nope"),
                edit("One", "pages/a.md", "x")
            ),
            Err(ProposalError::NoSuchView("heads/nope".into()))
        );
        e.bases
            .fail_with(Some(crate::proposal::BaseError::Unavailable("busy".into())));
        assert_eq!(
            s.create(&a.principal, w, None, edit("One", "pages/a.md", "x")),
            Err(ProposalError::BaseUnavailable("busy".into()))
        );
        e.bases.fail_with(None);
        assert!(
            s.list(&a.principal, w, &ProposalFilter::default())
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn decline_withdraw_and_a_request_for_revision_need_a_reason_and_change_nothing_without_one() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        for blank in ["", "   ", "\n\t"] {
            let p = e.held(&e.author, "One", "pages/a.md");
            for result in [
                s.decline(&e.maintainer.principal, w, &p.id, blank),
                s.ask_for_revision(&e.maintainer.principal, w, &p.id, blank),
                s.withdraw(&e.author.principal, w, &p.id, blank),
            ] {
                assert_eq!(result, Err(ProposalError::Invalid(Invalid::ReasonMissing)));
            }
            assert_eq!(
                s.get(&e.author.principal, w, &p.id).unwrap().state,
                ProposalState::HeldForReview
            );
            assert_eq!(
                s.transitions(&e.author.principal, w, &p.id).unwrap().len(),
                2
            );
        }
        let p = e.held(&e.author, "Two", "pages/b.md");
        let long = "x".repeat(crate::proposal::MAX_REASON_BYTES + 1);
        assert_eq!(
            s.decline(&e.maintainer.principal, w, &p.id, &long),
            Err(ProposalError::Invalid(Invalid::ReasonTooLong))
        );
        // The reason is kept as written, trimmed.
        let done = s
            .withdraw(&e.author.principal, w, &p.id, "  Not needed any more.  ")
            .unwrap();
        assert_eq!(done.state, ProposalState::Withdrawn);
        let log = s.transitions(&e.author.principal, w, &p.id).unwrap();
        assert_eq!(
            log.last().and_then(|t| t.reason.as_deref()),
            Some("Not needed any more.")
        );
    });
}

#[test]
fn a_decision_is_one_per_reviewer_per_revision_and_bound_to_the_latest() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        s.decide(
            &e.reviewer.principal,
            w,
            &p.id,
            1,
            DecisionKind::Approve,
            "ok",
        )
        .unwrap();
        assert_eq!(
            s.decide(
                &e.reviewer.principal,
                w,
                &p.id,
                1,
                DecisionKind::RequestChanges,
                "no"
            ),
            Err(ProposalError::AlreadyDecided),
            "a reviewer does not change a decision on a revision"
        );
        // Another reviewer decides on the same revision.
        let other = e.person(Some(inklkunn_store::Role::Reviewer));
        s.decide(&other.principal, w, &p.id, 1, DecisionKind::Approve, "")
            .unwrap();
        // A decision on a revision that does not exist yet, or no longer the latest, is refused.
        assert_eq!(
            s.decide(
                &e.maintainer.principal,
                w,
                &p.id,
                2,
                DecisionKind::Approve,
                ""
            ),
            Err(ProposalError::StaleRevision { latest: 1 })
        );
        s.revise(
            &e.author.principal,
            w,
            &p.id,
            edit("One", "pages/a.md", "again"),
        )
        .unwrap();
        assert_eq!(
            s.decide(
                &e.maintainer.principal,
                w,
                &p.id,
                1,
                DecisionKind::Approve,
                ""
            ),
            Err(ProposalError::StaleRevision { latest: 2 })
        );
        s.decide(
            &e.reviewer.principal,
            w,
            &p.id,
            2,
            DecisionKind::Approve,
            "",
        )
        .unwrap();
        let all = s.decisions(&e.author.principal, w, &p.id).unwrap();
        assert_eq!(
            all.iter().map(|d| d.revision).collect::<Vec<_>>(),
            vec![1, 1, 2],
            "earlier decisions stay recorded, each on its own revision"
        );
        // Needing a revision is not a state to decide in.
        s.ask_for_revision(&e.maintainer.principal, w, &p.id, "Rebase, please.")
            .unwrap();
        assert_eq!(
            s.decide(&other.principal, w, &p.id, 2, DecisionKind::Approve, ""),
            Err(ProposalError::NotInThisState {
                op: ProposalOp::Decide,
                state: ProposalState::NeedsRevision
            })
        );
    });
}

#[test]
fn comments_are_for_proposals_in_review_and_reply_within_the_proposal() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        let other = e.held(&e.author, "Two", "pages/b.md");
        let on_other = s
            .comment(
                &e.reviewer.principal,
                w,
                &other.id,
                NewComment {
                    body: "hm".into(),
                    ..NewComment::default()
                },
            )
            .unwrap();
        // A reply must be to a comment of the same proposal.
        assert_eq!(
            s.comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: "reply".into(),
                    parent: Some(on_other.id),
                    anchor: None
                }
            ),
            Err(ProposalError::NotFound)
        );
        // Something must be said, and not too much.
        assert_eq!(
            s.comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: " \n".into(),
                    ..NewComment::default()
                }
            ),
            Err(ProposalError::Invalid(Invalid::CommentEmpty))
        );
        let long = "x".repeat(crate::proposal::MAX_COMMENT_BYTES + 1);
        assert_eq!(
            s.comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: long,
                    ..NewComment::default()
                }
            ),
            Err(ProposalError::Invalid(Invalid::CommentTooLong))
        );
        // An anchor names a block or quotes text.
        let empty_anchor = inklkunn_store::Anchor {
            page: "pages/a.md".into(),
            block_id: None,
            quote: None,
            ordinal: 0,
        };
        assert_eq!(
            s.comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: "x".into(),
                    parent: None,
                    anchor: Some(empty_anchor)
                }
            ),
            Err(ProposalError::Invalid(Invalid::BadAnchor))
        );
        let block_anchor = inklkunn_store::Anchor {
            page: "pages/a.md".into(),
            block_id: Some("b3".into()),
            quote: None,
            ordinal: 4,
        };
        let c = s
            .comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: "x".into(),
                    parent: None,
                    anchor: Some(block_anchor.clone()),
                },
            )
            .unwrap();
        assert_eq!(c.anchor, Some(block_anchor));
        // Once the proposal has ended nobody comments.
        s.decline(&e.maintainer.principal, w, &p.id, "Not now.")
            .unwrap();
        assert_eq!(
            s.comment(
                &e.reviewer.principal,
                w,
                &p.id,
                NewComment {
                    body: "late".into(),
                    ..NewComment::default()
                }
            ),
            Err(ProposalError::NotInThisState {
                op: ProposalOp::Comment,
                state: ProposalState::Declined
            })
        );
    });
}

#[test]
fn a_proposal_can_be_superseded_by_a_later_one_in_the_ways_the_rfc_names() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let old = e.held(&e.author, "Old", "pages/a.md");
        let new = e.held(&e.author, "New", "pages/a.md");
        // Itself, no.
        assert_eq!(
            s.supersede(&e.author.principal, w, &old.id, &old.id),
            Err(ProposalError::Invalid(Invalid::SupersedeSelf))
        );
        // A contributor does not supersede someone else's proposal with their own.
        let other = e.person(Some(inklkunn_store::Role::Contributor));
        let theirs = e.held(&other, "Theirs", "pages/c.md");
        assert_eq!(
            s.supersede(&other.principal, w, &old.id, &theirs.id),
            Err(ProposalError::NotAllowed)
        );
        // Nor name another person's proposal as the superseder of their own.
        assert_eq!(
            s.supersede(&e.author.principal, w, &old.id, &theirs.id),
            Err(ProposalError::NotAllowed)
        );
        // A superseder must be in review (or accepted), not a draft.
        let draft = e.draft(&e.author, "Draft", "pages/d.md");
        assert_eq!(
            s.supersede(&e.author.principal, w, &old.id, &draft.id),
            Err(ProposalError::SupersederNotEligible {
                state: ProposalState::Draft
            })
        );
        let done = s
            .supersede(&e.author.principal, w, &old.id, &new.id)
            .unwrap();
        assert_eq!(done.state, ProposalState::Superseded);
        assert_eq!(done.superseded_by, Some(new.id.clone()));
        assert!(done.closed_at.is_some());
        // An ended proposal is not superseded again.
        assert_eq!(
            s.supersede(&e.author.principal, w, &old.id, &new.id),
            Err(ProposalError::NotInThisState {
                op: ProposalOp::Supersede,
                state: ProposalState::Superseded
            })
        );
        // An acceptor may name a proposal as superseded by one of their own.
        let mine = e.held(&e.maintainer, "Mine", "pages/e.md");
        let done = s
            .supersede(&e.maintainer.principal, w, &theirs.id, &mine.id)
            .unwrap();
        assert_eq!(done.superseded_by, Some(mine.id));
    });
}

#[test]
fn an_accepted_proposal_is_recorded_only_from_held_for_review() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        set_now(T0 + 100);
        let done = s.record_accepted(w, &p.id, &e.maintainer.id).unwrap();
        assert_eq!(done.state, ProposalState::Accepted);
        assert_eq!(done.closed_at, Some(T0 + 100));
        let log = s.transitions(&e.maintainer.principal, w, &p.id).unwrap();
        assert_eq!(
            log.last().map(|t| (t.op, t.actor.clone())),
            Some((ProposalOp::Accept, e.maintainer.id.clone()))
        );
        assert_eq!(
            s.record_accepted(w, &p.id, &e.maintainer.id),
            Err(ProposalError::NotInThisState {
                op: ProposalOp::Accept,
                state: ProposalState::Accepted
            })
        );
        let needs = e.held(&e.author, "Two", "pages/b.md");
        s.ask_for_revision(&e.maintainer.principal, w, &needs.id, "Rebase, please.")
            .unwrap();
        assert_eq!(
            s.record_accepted(w, &needs.id, &e.maintainer.id),
            Err(ProposalError::NotInThisState {
                op: ProposalOp::Accept,
                state: ProposalState::NeedsRevision
            })
        );
        let draft = e.draft(&e.author, "Three", "pages/c.md");
        assert_eq!(
            s.record_accepted(w, &draft.id, &e.maintainer.id),
            Err(ProposalError::NotFound),
            "a draft is not the acceptor's to see, let alone accept"
        );
    });
}
