//! The list of proposals that were sent for review (RFC 007 D-9; handoff 007 I-T5), on both stores: **never a draft, not even the
//! reader's own**, newest sent first, a window that does not move when a proposal changes state, a state to narrow it to, and
//! (the read check of the wiki, first, is the web tests': a restricted wiki is theirs to build).

use inklkunn_store::{ProposalId, ProposalState, Role};

use super::support::{Env, T0, on_both_with, set_now};
use crate::authorize::Principal;
use crate::proposal::QuotaConfig;

fn roomy() -> QuotaConfig {
    QuotaConfig {
        proposals_per_hour_per_wiki: 1000,
        proposals_per_hour_per_principal: 1000,
        open_proposals: 1000,
        ..QuotaConfig::default()
    }
}

fn ids(e: &Env, who: &Principal, state: Option<ProposalState>, limit: usize) -> Vec<ProposalId> {
    e.service
        .list_sent(who, &e.wiki, state, None, limit)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

#[test]
fn no_draft_is_listed_not_even_the_readers_own_and_what_is_sent_is_newest_first() {
    on_both_with(&roomy(), |e| {
        let (writer, other) = (
            e.person(Some(Role::Contributor)),
            e.person(Some(Role::Contributor)),
        );
        set_now(T0 + 10);
        let first = e.held(&writer, "First", "pages/a.md").id;
        set_now(T0 + 20);
        let second = e.held(&other, "Second", "pages/b.md").id;
        set_now(T0 + 30);
        let own_draft = e.draft(&writer, "A draft", "pages/c.md").id;
        let others_draft = e.draft(&other, "Another draft", "pages/d.md").id;
        // A proposal that was taken back before it was ever sent is not one that was sent.
        let never_sent = e.draft(&writer, "Never sent", "pages/e.md");
        e.service
            .withdraw(&writer.principal, &e.wiki, &never_sent.id, "Not needed.")
            .unwrap();
        set_now(T0 + 40);
        let third = e.held(&writer, "Third", "pages/f.md").id;

        for (who, name) in [
            (&Principal::Anonymous, "anonymous"),
            (&writer.principal, "the author of drafts"),
            (&other.principal, "another author of drafts"),
            (&e.observer.principal, "an observer"),
            (&e.maintainer.principal, "a maintainer"),
            (&e.admin.principal, "an admin"),
        ] {
            let listed = ids(e, who, None, 100);
            assert_eq!(
                listed,
                [third.clone(), second.clone(), first.clone()],
                "{name}: newest sent first, and the same for every reader"
            );
            for draft in [&own_draft, &others_draft, &never_sent.id] {
                assert!(!listed.contains(draft), "{name}: a draft is listed");
            }
        }
        // The draft is still its author's own to read: the list leaves it out, it does not unmake it.
        assert!(
            e.service
                .get(&writer.principal, &e.wiki, &own_draft)
                .is_ok()
        );
    });
}

#[test]
fn a_state_narrows_the_list_and_a_draft_is_no_state_a_list_has() {
    on_both_with(&roomy(), |e| {
        let writer = e.person(Some(Role::Contributor));
        set_now(T0 + 10);
        let held = e.held(&writer, "Held", "pages/a.md").id;
        set_now(T0 + 20);
        let declined = e.held(&writer, "Declined", "pages/b.md").id;
        e.service
            .decline(&e.maintainer.principal, &e.wiki, &declined, "No.")
            .unwrap();
        e.draft(&writer, "A draft", "pages/c.md");
        let anyone = &Principal::Anonymous;
        assert_eq!(
            ids(e, anyone, Some(ProposalState::HeldForReview), 100),
            [held.clone()]
        );
        assert_eq!(
            ids(e, anyone, Some(ProposalState::Declined), 100),
            [declined.clone()]
        );
        assert!(ids(e, anyone, Some(ProposalState::Accepted), 100).is_empty());
        // Asking for the drafts is asking for nothing, not for the reader's own.
        assert!(ids(e, &writer.principal, Some(ProposalState::Draft), 100).is_empty());
        assert_eq!(ids(e, anyone, None, 100), [declined, held]);
    });
}

#[test]
fn a_window_names_the_place_by_when_a_proposal_was_sent_so_a_state_change_does_not_move_it() {
    on_both_with(&roomy(), |e| {
        let writer = e.person(Some(Role::Contributor));
        let mut sent = Vec::new();
        for n in 0..5_i64 {
            set_now(T0 + 10 * (n + 1));
            sent.push(e.held(&writer, &format!("Proposal {n}"), &format!("pages/{n}.md")));
        }
        let anyone = &Principal::Anonymous;
        let whole: Vec<ProposalId> = sent.iter().rev().map(|p| p.id.clone()).collect();
        assert_eq!(ids(e, anyone, None, 100), whole);
        // Walk it two at a time; between the pages the oldest-but-one is decided, which moves it in no list that is ordered by
        // what happens to a proposal.
        let mut walked: Vec<ProposalId> = Vec::new();
        let mut after: Option<(i64, ProposalId)> = None;
        loop {
            let page = e
                .service
                .list_sent(anyone, &e.wiki, None, after.as_ref(), 2)
                .unwrap();
            if page.is_empty() {
                break;
            }
            if walked.is_empty() {
                set_now(T0 + 1000);
                let target = &sent[1].id;
                e.service
                    .decline(&e.maintainer.principal, &e.wiki, target, "No.")
                    .unwrap();
            }
            after = page
                .last()
                .and_then(|p| Some((p.submitted_at?, p.id.clone())));
            walked.extend(page.into_iter().map(|p| p.id));
        }
        assert_eq!(walked, whole, "each proposal once, in order");
    });
}
