//! Who reads what (RFC 006 D-9; threat model C-14g): **an appeal's text is the appellant's and the deciding rung's, and no one
//! else's**, a person's own page lists only their own, and what a person may contest is listed for them and for no one else.

use inklkunn_store::{AppealedKind, Role, Visibility};

use super::scenes::scene;
use super::support::{KINDS, World, now};
use crate::appeal::{AppealError, AppealLimits, AppealMark, ContestWhat, Offer, Outcome};

const WORDS: &str = "A private grievance, naming people.";

#[test]
fn an_appeal_s_text_is_read_by_the_appellant_and_the_deciding_rung_and_by_no_one_else() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let mine = w.appeals.mine(&s.appellant.principal, &w.wiki).unwrap();
        assert_eq!(
            mine.appeals.first().map(|r| r.text.as_str()),
            Some(WORDS),
            "the appellant reads it"
        );
        let queue = w.appeals.appeals(&admin.principal, &w.wiki).unwrap();
        assert_eq!(
            queue.open.first().map(|q| q.row.text.as_str()),
            Some(WORDS),
            "the deciding rung reads it"
        );
        // Everyone else: nothing.
        let third = w.person("Tess", Some(Role::Contributor));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        for other in [&third, &maintainer, &s.actor] {
            let theirs = w.appeals.mine(&other.principal, &w.wiki).unwrap();
            assert!(theirs.appeals.is_empty(), "only their own appeals");
            assert!(
                theirs.contestable.is_empty(),
                "and only what is theirs to contest"
            );
            assert_eq!(
                w.appeals.appeals(&other.principal, &w.wiki).unwrap_err(),
                AppealError::NotAllowed,
                "the queue is not theirs"
            );
        }
        assert_eq!(
            w.appeals
                .appeals(&crate::Principal::Anonymous, &w.wiki)
                .unwrap_err(),
            AppealError::NotAllowed
        );
        // A decided appeal is still the same two people's.
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Upheld,
                "It stands.",
            )
            .unwrap();
        let queue = w.appeals.appeals(&admin.principal, &w.wiki).unwrap();
        assert!(queue.open.is_empty());
        let [decided] = queue.decided.as_slice() else {
            panic!("one decided");
        };
        assert_eq!(decided.text, WORDS);
        assert_eq!(
            decided
                .decision
                .as_ref()
                .map(|d| (d.outcome, d.by.as_str(), d.reason.as_str())),
            Some((Outcome::Upheld, "Dee", "It stands."))
        );
    }
}

#[test]
fn a_stranger_of_a_closed_wiki_is_answered_as_if_the_wiki_were_not_there_and_sees_no_appeal() {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Restricted,
            None,
            AppealLimits::default(),
            now,
        );
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Restriction);
        w.appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        let stranger = w.person("Sam", None);
        assert_eq!(
            w.appeals.appeals(&stranger.principal, &w.wiki).unwrap_err(),
            AppealError::NotFound
        );
        assert_eq!(
            w.appeals.mine(&stranger.principal, &w.wiki).unwrap_err(),
            AppealError::NotFound
        );
        assert_eq!(
            w.appeals.offer(&stranger.principal, &w.wiki, &s.contest),
            Offer::Nothing
        );
        assert!(w.appeals.appeals(&admin.principal, &w.wiki).is_ok());
    }
}

#[test]
fn what_a_person_may_contest_is_listed_for_them_with_the_offer_each_link_is_drawn_from() {
    for kind in KINDS {
        for what in AppealedKind::ALL {
            let w = World::new(kind);
            let s = scene(&w, what);
            let mine = w.appeals.mine(&s.appellant.principal, &w.wiki).unwrap();
            let found = mine
                .contestable
                .iter()
                .find(|c| c.contest == s.contest)
                .unwrap_or_else(|| panic!("{what:?}: listed: {:?}", mine.contestable));
            assert_eq!(found.filed, None);
            match (&found.what, what) {
                (ContestWhat::Decline { proposal }, AppealedKind::Decline) => {
                    assert_eq!(Some(proposal), s.proposal.as_ref());
                }
                (ContestWhat::Tombstone { .. }, AppealedKind::Tombstone)
                | (ContestWhat::Restriction { .. }, AppealedKind::Restriction)
                | (ContestWhat::Protection { .. }, AppealedKind::Protection) => {}
                (ContestWhat::Grant { before, after, .. }, AppealedKind::Grant) => {
                    assert_eq!(
                        (*before, *after),
                        (Some(Role::Maintainer), Some(Role::Reviewer))
                    );
                }
                other => panic!("{what:?}: {other:?}"),
            }
            // The form says what is contested, to the person it affects only.
            assert!(
                w.appeals
                    .contesting(&s.appellant.principal, &w.wiki, &s.contest)
                    .is_ok()
            );
            let stranger = w.person("Sam", Some(Role::Contributor));
            assert_eq!(
                w.appeals
                    .contesting(&stranger.principal, &w.wiki, &s.contest)
                    .unwrap_err(),
                AppealError::NotAffected
            );
            // Once filed, the same row says so and offers no second link.
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap();
            let mine = w.appeals.mine(&s.appellant.principal, &w.wiki).unwrap();
            let found = mine
                .contestable
                .iter()
                .find(|c| c.contest == s.contest)
                .expect("still listed");
            assert_eq!(found.filed.as_ref(), Some(&filed.id));
        }
    }
}

#[test]
fn a_proposal_s_page_offers_the_decline_and_the_protection_to_its_proposer_and_to_no_one_else() {
    for kind in KINDS {
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Decline);
        let proposal = s.proposal.clone().unwrap();
        let offered = w
            .appeals
            .for_proposal(&s.appellant.principal, &w.wiki, &proposal);
        assert_eq!(
            offered
                .iter()
                .map(|c| c.contest.clone())
                .collect::<Vec<_>>(),
            vec![s.contest.clone()]
        );
        for other in [
            w.person("Sam", Some(Role::Contributor)),
            w.person("Mia", Some(Role::Maintainer)),
            w.person("Dee", Some(Role::Admin)),
        ] {
            assert!(
                w.appeals
                    .for_proposal(&other.principal, &w.wiki, &proposal)
                    .is_empty()
            );
        }
        assert!(
            w.appeals
                .for_proposal(&crate::Principal::Anonymous, &w.wiki, &proposal)
                .is_empty()
        );

        // A protection that holds an open proposal back.
        let w = World::new(kind);
        let s = scene(&w, AppealedKind::Protection);
        let proposal = s.proposal.clone().unwrap();
        let offered = w
            .appeals
            .for_proposal(&s.appellant.principal, &w.wiki, &proposal);
        assert_eq!(
            offered
                .iter()
                .map(|c| c.contest.clone())
                .collect::<Vec<_>>(),
            vec![s.contest.clone()]
        );
    }
}

#[test]
fn the_log_mark_of_a_hide_says_that_it_was_appealed_and_how_it_ended_and_nothing_else() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let s = scene(&w, AppealedKind::Tombstone);
        let hide = inklkunn_store::TombstoneEventId::parse(&s.contest.id).unwrap();
        assert_eq!(w.appeals.mark_of_hide(&w.wiki, &hide).unwrap(), None);
        let filed = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        assert_eq!(
            w.appeals.mark_of_hide(&w.wiki, &hide).unwrap(),
            Some(AppealMark::Appealed)
        );
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &filed.id,
                Outcome::Overturned,
                "Wrongly hidden.",
            )
            .unwrap();
        assert_eq!(
            w.appeals.mark_of_hide(&w.wiki, &hide).unwrap(),
            Some(AppealMark::Overturned)
        );
    }
}
