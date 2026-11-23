#![allow(clippy::expect_used, clippy::unwrap_used)]
//! A proposal that replaces a knowledge file the wiki cannot read is the `moderate` rung's, and **the acceptance asks the rung
//! of the proposal's author again, under the lease** (handoff G-T6 correction S-2, decision 17). What came between the sending
//! and the acceptance (a restriction, a demotion) is not what the proposal was judged for. Each kind of file the wiki cannot read
//! is tried: not UTF-8, over what a knowledge file may be, not in the format. In-memory, with the author's standing changed the
//! moment the lease is taken.

use inklkunn_store::{
    GovernanceStore, MeasureEvent, ProposalState, RestrictionEvent, RestrictionEventId, RoleGrant,
    RoleStore,
};

use super::super::testing::Rig;
use super::*;
use crate::acceptance::AcceptStep;

const BROKEN: &str = "knowledge/claims/01930000-0000-7000-8000-0000000000aa.yaml";
const GOOD: &str = "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions: []\nrelations: []\nsources: []\n";

/// The ways a file is unreadable.
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// Not UTF-8.
    NotUtf8,
    /// Over what a knowledge file may be.
    TooLong,
    /// Text of the right size that the format does not read.
    NotParsing,
}

const KINDS: [Kind; 3] = [Kind::NotUtf8, Kind::TooLong, Kind::NotParsing];

impl Kind {
    fn bytes(self) -> Vec<u8> {
        match self {
            Kind::NotUtf8 => vec![0xff, 0xfe, 0x00, b'k'],
            Kind::TooLong => vec![b'a'; crate::knowledge::MAX_FILE_BYTES + 10],
            Kind::NotParsing => b"kind: observation\nsecret: words\n".to_vec(),
        }
    }
}

struct Scene {
    rig: Rig,
    author: (AccountId, Principal),
    acceptor: (AccountId, Principal),
    moderator: AccountId,
}

/// A wiki that holds a file it cannot read, a maintainer who may moderate and has proposed to replace it, and another who accepts.
fn scene(kind: Kind) -> Scene {
    let rig = Rig::new("author-moderate");
    rig.fake.derive_changes();
    rig.fake.set_bytes(BROKEN, &kind.bytes());
    rig.reads.forget_wiki(&rig.wiki);
    let author = rig.person("Noor", Some(Role::Maintainer));
    let acceptor = rig.person("Maya", Some(Role::Maintainer));
    let (moderator, _) = rig.person("Ada", Some(Role::Admin));
    Scene {
        rig,
        author,
        acceptor,
        moderator,
    }
}

impl Scene {
    fn propose(&self) -> ProposalRecord {
        self.rig.propose(
            &self.author.1,
            "Replace the claim the wiki cannot read",
            &[(BROKEN, Some(GOOD))],
        )
    }

    fn accept_with(
        &self,
        p: &ProposalRecord,
        between: impl Fn() + Send + Sync + 'static,
    ) -> Result<Accepted, AcceptFailure> {
        self.rig
            .acceptor()
            .with_observer(Arc::new(move |step| {
                if step == AcceptStep::LeaseTaken {
                    between();
                }
            }))
            .accept(&self.acceptor.1, &self.rig.wiki, &p.id, 1)
    }

    /// The author is demoted to a contributor.
    fn demote(&self) -> impl Fn() + Send + Sync + 'static + use<> {
        let store = self.rig.store.clone();
        let (who, wiki) = (self.author.0.clone(), self.rig.wiki.id.clone());
        move || {
            store
                .grant(RoleGrant {
                    account: who.clone(),
                    wiki: wiki.clone(),
                    role: Role::Contributor,
                    granted_by: None,
                    granted_at: 2,
                })
                .unwrap();
        }
    }

    /// The author is restricted for a day.
    fn restrict(&self) -> impl Fn() + Send + Sync + 'static + use<> {
        let store = self.rig.store.clone();
        let wiki = self.rig.wiki.id.clone();
        let who = self.author.0.clone();
        let by = self.moderator.clone();
        move || {
            let now = inklkunn_store::record::now_secs();
            store
                .record_restriction_event(&RestrictionEvent {
                    id: RestrictionEventId::new(),
                    wiki: wiki.clone(),
                    at: now,
                    event: MeasureEvent::Set,
                    account: who.clone(),
                    by: inklkunn_store::Actor {
                        account: by.clone(),
                        rung: Role::Admin,
                    },
                    policy_block: None,
                    reason: Some("Edit-warring.".to_owned()),
                    expires_at: Some(now + 86_400),
                    of_event: None,
                    appeal: None,
                })
                .unwrap();
        }
    }

    fn nothing_was_signed(&self, p: &ProposalRecord) {
        assert!(self.rig.double.calls().is_empty(), "nothing is signed");
        assert_eq!(self.rig.state(&p.id), ProposalState::HeldForReview);
    }
}

#[test]
fn an_author_who_still_may_moderate_has_the_file_replaced_whatever_made_it_unreadable() {
    for kind in KINDS {
        let s = scene(kind);
        let p = s.propose();
        let result = s.accept_with(&p, || {});
        assert!(
            matches!(result, Ok(Accepted::Signed(_))),
            "{kind:?}: {result:?}"
        );
    }
}

#[test]
fn an_author_demoted_after_sending_is_refused_at_acceptance_and_nothing_is_signed() {
    for kind in KINDS {
        let s = scene(kind);
        let p = s.propose();
        let result = s.accept_with(&p, s.demote());
        assert!(
            matches!(
                result,
                Err(AcceptFailure::AuthorNotAllowed { restricted: false })
            ),
            "{kind:?}: {result:?}"
        );
        s.nothing_was_signed(&p);
    }
}

#[test]
fn an_author_restricted_after_sending_is_refused_at_acceptance_with_the_restriction_as_the_cause() {
    for kind in KINDS {
        let s = scene(kind);
        let p = s.propose();
        let result = s.accept_with(&p, s.restrict());
        assert!(
            matches!(
                result,
                Err(AcceptFailure::AuthorNotAllowed { restricted: true })
            ),
            "{kind:?}: {result:?}"
        );
        s.nothing_was_signed(&p);
    }
}

#[test]
fn a_restriction_on_an_author_who_was_demoted_anyway_is_not_the_cause() {
    for kind in KINDS {
        let s = scene(kind);
        let p = s.propose();
        let (demote, restrict) = (s.demote(), s.restrict());
        let result = s.accept_with(&p, move || {
            demote();
            restrict();
        });
        assert!(
            matches!(
                result,
                Err(AcceptFailure::AuthorNotAllowed { restricted: false })
            ),
            "{kind:?}: {result:?}"
        );
    }
}

#[test]
fn an_author_who_accepts_their_own_proposal_after_a_demotion_is_told_not_allowed() {
    // The rules let a reviewer accept, so the author who was demoted to one still may; the rung that moderates they no longer hold,
    // and it is their own answer: *not allowed* (a restriction, when it was the cause, would be theirs to read).
    for kind in KINDS {
        let rig = Rig::new("author-moderate-self");
        rig.fake.derive_changes();
        rig.fake.set_files(&[(
            "governance/policy.yaml".to_owned(),
            "rungs:\n  accept: reviewer\n".to_owned(),
        )]);
        rig.fake.set_bytes(BROKEN, &kind.bytes());
        rig.reads.forget_wiki(&rig.wiki);
        let (author_id, author) = rig.person("Noor", Some(Role::Maintainer));
        let p = rig.propose(
            &author,
            "Replace the claim the wiki cannot read",
            &[(BROKEN, Some(GOOD))],
        );
        let store = rig.store.clone();
        let (who, wiki) = (author_id.clone(), rig.wiki.id.clone());
        let result = rig
            .acceptor()
            .with_observer(Arc::new(move |step| {
                if step == AcceptStep::LeaseTaken {
                    store
                        .grant(RoleGrant {
                            account: who.clone(),
                            wiki: wiki.clone(),
                            role: Role::Reviewer,
                            granted_by: None,
                            granted_at: 2,
                        })
                        .unwrap();
                }
            }))
            .accept(&author, &rig.wiki, &p.id, 1);
        assert!(
            matches!(result, Err(AcceptFailure::NotAllowed)),
            "{kind:?}: {result:?}"
        );
        assert!(rig.double.calls().is_empty());
    }
}

#[test]
fn a_wiki_that_asks_the_moderate_rung_of_admins_refuses_a_maintainer_author_at_acceptance() {
    // The rules at the state the change is signed on decide, not the ones the proposal was sent under.
    // (A file the wiki cannot read as text cannot be merged onto a tip that moved after the proposal was sent, and the change of rules
    // moves it: the file here is one that is text, and does not parse.)
    let s = scene(Kind::NotParsing);
    let p = s.propose();
    let rig = &s.rig;
    let result = {
        let fake = rig.fake.clone();
        let reads = rig.reads.clone();
        let wiki = rig.wiki.clone();
        s.accept_with(&p, move || {
            fake.set_files(&[(
                "governance/policy.yaml".to_owned(),
                "rungs:\n  moderate: admin\n".to_owned(),
            )]);
            reads.forget_wiki(&wiki);
        })
    };
    assert!(
        matches!(
            result,
            Err(AcceptFailure::AuthorNotAllowed { restricted: false })
        ),
        "{result:?}"
    );
    s.nothing_was_signed(&p);
}
