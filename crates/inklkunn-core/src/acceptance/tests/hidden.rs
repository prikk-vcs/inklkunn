#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Hidden words are never newly signed (RFC 006 D-8; handoff G-T4 correction H-2): **an acceptance refuses while the summary of
//! the proposal's latest revision is hidden**, before the lease and again under it, since the change's message is built from the
//! record's own summary. A hidden author does not refuse (the message names the proposer for attribution; the name is masked
//! wherever the change is shown), and a revision whose summary is not hidden makes the acceptance possible again.

use inklkunn_store::{
    Actor, GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts, ProposalState, ReasonCategory,
    TombstoneAct, TombstoneActId, TombstoneEvent, TombstoneEventId,
};

use super::super::testing::{HOME, Rig};
use super::*;
use crate::acceptance::AcceptStep;
use crate::proposal::ProposalEdit;

fn changed(text: &str) -> String {
    text.replace("One.", "Uno.")
}

struct Scene {
    rig: Rig,
    maintainer: Principal,
    author: Principal,
    moderator: AccountId,
}

fn scene() -> Scene {
    let rig = Rig::new("hidden-summary");
    rig.fake.derive_changes();
    let (_, maintainer) = rig.person("Maya", Some(Role::Maintainer));
    let (_, author) = rig.person("Noor", Some(Role::Contributor));
    let (moderator, _) = rig.person("Ada", Some(Role::Admin));
    Scene {
        rig,
        maintainer,
        author,
        moderator,
    }
}

const SUMMARY: &str = "ZEBRASUMMARY say it in Spanish";

impl Scene {
    fn propose(&self, summary: &str) -> ProposalRecord {
        self.rig.propose(
            &self.author,
            summary,
            &[("pages/Home.md", Some(&changed(HOME)))],
        )
    }

    /// A hide of `parts` of revision `seq` of the proposal, as the moderator's record would be (the store's own checks only).
    fn hide(
        &self,
        p: &ProposalRecord,
        seq: u32,
        parts: HiddenParts,
    ) -> impl Fn() + Send + Sync + 'static + use<> {
        let store = self.rig.store.clone();
        let wiki = self.rig.wiki.id.clone();
        let by = self.moderator.clone();
        let item = HiddenItem {
            kind: HiddenItemKind::ProposalRevision,
            id: p.id.to_string(),
            reference: Some(seq.to_string()),
        };
        move || {
            store
                .record_tombstone_event(&TombstoneEvent {
                    id: TombstoneEventId::new(),
                    wiki: wiki.clone(),
                    at: inklkunn_store::record::now_secs(),
                    act_id: TombstoneActId::new(),
                    act: TombstoneAct::Hide,
                    item: item.clone(),
                    parts,
                    category: Some(ReasonCategory::PersonalData),
                    note: None,
                    affected: None,
                    by: Actor {
                        account: by.clone(),
                        rung: Role::Admin,
                    },
                    policy_block: None,
                    of_event: None,
                    appeal: None,
                    declined_transition: None,
                })
                .unwrap();
        }
    }

    fn accept(&self, p: &ProposalRecord, revision: u32) -> Result<Accepted, AcceptFailure> {
        self.rig
            .acceptor()
            .accept(&self.maintainer, &self.rig.wiki, &p.id, revision)
    }
}

const SUMMARY_ONLY: HiddenParts = HiddenParts {
    content: false,
    summary: true,
    author: false,
};
const AUTHOR_ONLY: HiddenParts = HiddenParts {
    content: false,
    summary: false,
    author: true,
};

#[test]
fn an_acceptance_is_refused_while_the_latest_summary_is_hidden_and_nothing_is_signed_or_changed() {
    let s = scene();
    let p = s.propose(SUMMARY);
    s.hide(&p, 1, SUMMARY_ONLY)();
    // Refused before the lease: the lease is never asked for.
    let leased = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = leased.clone();
    let result = s
        .rig
        .acceptor()
        .with_observer(Arc::new(move |step| {
            if step == AcceptStep::LeaseTaken {
                seen.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }))
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(
        matches!(result, Err(AcceptFailure::SummaryHidden)),
        "{result:?}"
    );
    assert!(
        !leased.load(std::sync::atomic::Ordering::SeqCst),
        "the lease was not taken"
    );
    assert!(s.rig.double.calls().is_empty(), "nothing is signed");
    assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);
    assert!(!AcceptFailure::SummaryHidden.to_string().contains("ZEBRA"));
    assert_eq!(
        crate::acceptance::words::SUMMARY_HIDDEN,
        "summary-hidden",
        "the word the API's reason says"
    );
}

#[test]
fn a_summary_hidden_between_the_early_check_and_the_lease_is_refused_under_the_lease() {
    let s = scene();
    let p = s.propose(SUMMARY);
    let hide = s.hide(&p, 1, SUMMARY_ONLY);
    let result = s
        .rig
        .acceptor()
        .with_observer(Arc::new(move |step| {
            if step == AcceptStep::LeaseTaken {
                hide();
            }
        }))
        .accept(&s.maintainer, &s.rig.wiki, &p.id, 1);
    assert!(
        matches!(result, Err(AcceptFailure::SummaryHidden)),
        "{result:?}"
    );
    assert!(s.rig.double.calls().is_empty(), "nothing is signed");
    assert_eq!(s.rig.state(&p.id), ProposalState::HeldForReview);
}

#[test]
fn a_revision_whose_summary_is_not_hidden_makes_the_acceptance_possible_again() {
    let s = scene();
    let p = s.propose(SUMMARY);
    s.hide(&p, 1, SUMMARY_ONLY)();
    assert!(matches!(s.accept(&p, 1), Err(AcceptFailure::SummaryHidden)));
    // The proposer revises it: the latest revision's summary is a new one, which nothing hides.
    let revised = s
        .rig
        .proposals
        .revise(
            &s.author,
            &s.rig.wiki,
            &p.id,
            ProposalEdit {
                summary: "Say it in Spanish, plainly".to_owned(),
                files: vec![inklkunn_store::FileChange {
                    path: "pages/Home.md".to_owned(),
                    text: Some(changed(HOME)),
                }],
                ..ProposalEdit::default()
            },
        )
        .expect("a revision");
    assert_eq!(revised.state, ProposalState::HeldForReview);
    let result = s.accept(&p, 2);
    assert!(matches!(result, Ok(Accepted::Signed(_))), "{result:?}");
    assert_eq!(s.rig.double.calls().len(), 1);
}

#[test]
fn a_hidden_author_does_not_refuse_and_the_message_names_the_proposer_for_attribution() {
    let s = scene();
    let p = s.propose("Say it in Spanish");
    s.hide(&p, 1, AUTHOR_ONLY)();
    let result = s.accept(&p, 1);
    assert!(matches!(result, Ok(Accepted::Signed(_))), "{result:?}");
    let calls = s.rig.double.calls();
    assert_eq!(calls.len(), 1);
    let message = format!("{:?}", calls.first().expect("a call").change);
    assert!(message.contains("Noor"), "{message}");
}
