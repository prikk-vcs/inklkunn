//! `entry` through the real authorizer and the proposal service (RFC 006 D-2, D-5; acceptance criterion 6): for each value and
//! each visibility the rung an account holds without a grant, and so exactly the actions it may do. **In a `restricted` wiki
//! it gives nothing** (visibility and membership there stay the operator's, D-3). The account's own account of why it holds
//! the rung (`holding`) says so in the same cases.

use inklkunn_store::{Role, Visibility};

use super::support::{KINDS, World};
use crate::authorize::{Action, HeldBy};
use crate::governance::{Entry, PlatformLimits};
use crate::proposal::ProposalError;

const VISIBILITIES: [Visibility; 3] = [
    Visibility::Public,
    Visibility::Internal,
    Visibility::Restricted,
];

/// What a rung may do, in 0.3.0's words: the ladder written out, not taken from the code.
fn ladder(rung: Role) -> Vec<Action> {
    use Action::*;
    match rung {
        Role::Observer => vec![Read],
        Role::Questioner => vec![Read, Comment],
        Role::Contributor => vec![Read, Comment, Propose],
        Role::Reviewer => vec![Read, Comment, Propose, Review, CreateView],
        Role::Maintainer => vec![Read, Comment, Propose, Review, CreateView, Accept, Moderate],
        Role::Admin => Action::ALL.to_vec(),
    }
}

#[test]
fn entry_gives_every_signed_in_account_its_rung_in_a_public_or_internal_wiki_and_nothing_in_a_restricted_one()
 {
    for kind in KINDS {
        for entry in Entry::ALL {
            for visibility in VISIBILITIES {
                for grant in [None, Some(Role::Observer), Some(Role::Reviewer)] {
                    let w = World::with(
                        kind,
                        visibility,
                        Some(&format!("entry: {}\n", entry.word())),
                        PlatformLimits {
                            // The operator's own ceiling is not what is tested here.
                            entry_max: Entry::Contributor,
                            ..PlatformLimits::default()
                        },
                    );
                    let person = w.person("Pat", grant);
                    let given = match visibility {
                        Visibility::Restricted => None,
                        Visibility::Public | Visibility::Internal => entry.rung(),
                    };
                    let rung = grant.max(given);
                    let allowed: Vec<Action> = Action::ALL
                        .into_iter()
                        .filter(|a| w.may(&person, *a))
                        .collect();
                    let expected: Vec<Action> = match rung {
                        Some(r) => ladder(r),
                        // No rung: what the visibility lets an account read, and nothing else.
                        None if visibility == Visibility::Restricted => Vec::new(),
                        None => vec![Action::Read],
                    };
                    assert_eq!(
                        allowed,
                        expected,
                        "{kind:?} entry {} in a {visibility:?} wiki, grant {grant:?}",
                        entry.word()
                    );
                    // And the account's own account of it.
                    let held = w.authorizer.holding(&person.principal, &w.wiki.id).unwrap();
                    assert_eq!(held.rung, rung);
                    assert_eq!((held.grant, held.entry), (grant, given));
                    let why = match (grant, given) {
                        (None, None) => HeldBy::Nothing,
                        (Some(g), e) if Some(g) >= e => HeldBy::Grant,
                        _ => HeldBy::Entry,
                    };
                    assert_eq!(held.held_by(), why);
                }
            }
        }
    }
}

#[test]
fn a_person_with_no_grant_proposes_where_entry_says_contributor_and_is_refused_where_it_gives_nothing()
 {
    for kind in KINDS {
        for (visibility, may) in [
            (Visibility::Public, true),
            (Visibility::Internal, true),
            (Visibility::Restricted, false),
        ] {
            let w = World::with(
                kind,
                visibility,
                Some("entry: contributor\n"),
                PlatformLimits::default(),
            );
            let newcomer = w.person("Nia", None);
            let made = w.proposals.create(
                &newcomer.principal,
                &w.wiki,
                None,
                crate::proposal::ProposalEdit {
                    summary: "A first edit".to_owned(),
                    files: vec![inklkunn_store::FileChange {
                        path: "pages/First.md".to_owned(),
                        text: Some("text".to_owned()),
                    }],
                    ..crate::proposal::ProposalEdit::default()
                },
            );
            match (may, made) {
                (true, Ok(_)) => {}
                (false, Err(ProposalError::NotAllowed)) => {}
                (_, other) => panic!("{kind:?} {visibility:?}: {other:?}"),
            }
        }
    }
}
