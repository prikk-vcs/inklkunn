//! The events the acts of the proposal service make: who is told, who is not, and that an act refused for want
//! of a reason tells nobody.

use inklkunn_store::InboxEventKind as Kind;

use super::world::on_both;
use crate::proposal::ProposalError;

#[test]
fn a_decline_tells_the_proposer_and_a_decline_without_a_reason_tells_nobody() {
    on_both(|w| {
        let p = w.held(&w.author, "Fix a typo");
        // The good-faith template needs a reason: without one the decline is refused and nothing is told.
        for empty in ["", "   ", "\n"] {
            assert!(matches!(
                w.service
                    .decline(&w.maintainer.principal, &w.wiki, &p.id, empty),
                Err(ProposalError::Invalid(_))
            ));
        }
        assert!(w.kinds(&w.author).is_empty());

        w.service
            .decline(
                &w.maintainer.principal,
                &w.wiki,
                &p.id,
                "Not for this page; try the guide.",
            )
            .expect("declined");
        assert_eq!(w.kinds(&w.author), vec![Kind::Declined]);
        // The decliner is not told of their own act, and nobody else is either.
        for other in [&w.maintainer, &w.reviewer, &w.observer, &w.outsider] {
            assert!(w.kinds(other).is_empty());
        }
        // The event is ids and a kind: the proposal, the wiki and who did it.
        let event = w
            .inbox
            .entries(&w.author.principal, false)
            .expect("entries")
            .remove(0);
        assert_eq!(event.kind, Kind::Declined);
        assert!(event.unread);
    });
}

#[test]
fn a_request_for_a_revision_and_a_review_tell_the_proposer() {
    on_both(|w| {
        let p = w.held(&w.author, "Add a section");
        w.service
            .decide(
                &w.reviewer.principal,
                &w.wiki,
                &p.id,
                1,
                inklkunn_store::DecisionKind::RequestChanges,
                "Needs a source.",
            )
            .expect("decided");
        assert_eq!(w.kinds(&w.author), vec![Kind::Reviewed]);
        w.service
            .ask_for_revision(
                &w.maintainer.principal,
                &w.wiki,
                &p.id,
                "Please add the source.",
            )
            .expect("sent back");
        assert_eq!(
            w.kinds(&w.author),
            vec![Kind::NeedsRevision, Kind::Reviewed]
        );
        // A reviewer who is the proposer is not told of their own decision.
        assert!(w.kinds(&w.reviewer).is_empty());
    });
}

#[test]
fn a_withdrawal_without_a_reason_tells_nobody_and_with_one_tells_those_who_held_it() {
    on_both(|w| {
        let p = w.held(&w.author, "Rewrite the intro");
        w.comment(&w.reviewer, &p, "Looks reasonable.");
        assert!(matches!(
            w.service.withdraw(&w.author.principal, &w.wiki, &p.id, " "),
            Err(ProposalError::Invalid(_))
        ));
        for who in [&w.maintainer, &w.reviewer] {
            assert!(!w.kinds(who).contains(&Kind::Withdrawn));
        }

        w.service
            .withdraw(
                &w.author.principal,
                &w.wiki,
                &p.id,
                "Superseded by my own plan.",
            )
            .expect("withdrawn");
        // The acceptors who held it and the reviewer who took part; not the proposer, not an observer.
        assert!(w.kinds(&w.maintainer).contains(&Kind::Withdrawn));
        assert!(w.kinds(&w.reviewer).contains(&Kind::Withdrawn));
        assert!(!w.kinds(&w.author).contains(&Kind::Withdrawn));
        assert!(!w.kinds(&w.observer).contains(&Kind::Withdrawn));
    });
}

#[test]
fn a_draft_that_is_withdrawn_was_held_by_nobody() {
    on_both(|w| {
        let draft = w
            .service
            .create(
                &w.author.principal,
                &w.wiki,
                None,
                crate::proposal::ProposalEdit {
                    summary: "Not yet".to_owned(),
                    ..Default::default()
                },
            )
            .expect("a draft");
        w.service
            .withdraw(&w.author.principal, &w.wiki, &draft.id, "Changed my mind.")
            .expect("withdrawn");
        for who in [&w.maintainer, &w.reviewer, &w.author] {
            assert!(w.kinds(who).is_empty(), "{:?}", w.kinds(who));
        }
    });
}

#[test]
fn a_superseded_proposal_tells_its_proposer_unless_they_did_it() {
    on_both(|w| {
        let old = w.held(&w.author, "First try");
        let theirs = w.held(&w.reviewer, "A better try");
        // The maintainer may supersede another's proposal only as the proposer of the newer one; a proposer
        // supersedes their own, and is not told of it.
        let newer = w.held(&w.author, "Second try");
        w.service
            .supersede(&w.author.principal, &w.wiki, &old.id, &newer.id)
            .expect("superseded");
        assert!(w.kinds(&w.author).is_empty());
        // The reviewer's own proposal, superseded by the maintainer's, tells the reviewer.
        let by_maintainer = w.held(&w.maintainer, "The maintainer's try");
        w.service
            .supersede(
                &w.maintainer.principal,
                &w.wiki,
                &theirs.id,
                &by_maintainer.id,
            )
            .expect("superseded");
        assert_eq!(w.kinds(&w.reviewer), vec![Kind::Superseded]);
    });
}

#[test]
fn an_act_that_fails_for_another_reason_tells_nobody() {
    on_both(|w| {
        let p = w.held(&w.author, "Fix a typo");
        // An observer may not decline; a contributor may not withdraw another's.
        assert!(
            w.service
                .decline(&w.observer.principal, &w.wiki, &p.id, "no")
                .is_err()
        );
        assert!(
            w.service
                .withdraw(&w.reviewer.principal, &w.wiki, &p.id, "no")
                .is_err()
        );
        // Declining twice: the second finds it declined already.
        w.service
            .decline(&w.maintainer.principal, &w.wiki, &p.id, "No.")
            .expect("declined");
        assert!(
            w.service
                .decline(&w.maintainer.principal, &w.wiki, &p.id, "Again.")
                .is_err()
        );
        assert_eq!(w.kinds(&w.author), vec![Kind::Declined]);
        assert_eq!(w.notifier.failures(), 0);
    });
}
