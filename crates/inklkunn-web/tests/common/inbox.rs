//! Proposals and inbox events for the page tests (handoff 004 T6): the records the inbox and the lists of
//! accepted changes read. The service that makes them is `inklkunn-core`'s and has its own tests; here a page
//! test only needs them to be there.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{
    AccountId, Base, FileChange, InboxEvent, InboxEventId, InboxEventKind, InboxStore,
    ProposalChange, ProposalId, ProposalOp, ProposalRecord, ProposalState, ProposalStore,
    ProposalTransition, RevisionContent, RevisionRecord,
};

use super::{NOW, World};

/// A submitted proposal of one page by `author`, in the world's wiki.
pub fn held_proposal(w: &World, author: &AccountId, summary: &str) -> ProposalRecord {
    let record = ProposalRecord {
        id: ProposalId::new(),
        wiki: w.wiki.id.clone(),
        view: "heads/main".to_owned(),
        state: ProposalState::Draft,
        author: author.clone(),
        summary: summary.to_owned(),
        sources: Vec::new(),
        superseded_by: None,
        created_at: NOW,
        updated_at: NOW,
        submitted_at: None,
        closed_at: None,
    };
    let content = RevisionContent {
        base: Base::new(&format!("{:064x}", 1_u64), &format!("{:064x}", 2_u64)).unwrap(),
        files: vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some("text".to_owned()),
        }],
        renames: Vec::new(),
        minor: false,
        automated: false,
    };
    let step = |op, from, to| ProposalTransition {
        proposal: record.id.clone(),
        op,
        from,
        to,
        actor: author.clone(),
        reason: None,
        at: NOW,
    };
    w.store
        .create_proposal(
            &record,
            &content,
            &step(ProposalOp::Create, None, ProposalState::Draft),
        )
        .unwrap();
    w.store
        .change_proposal(&ProposalChange {
            transition: step(
                ProposalOp::Submit,
                Some(ProposalState::Draft),
                ProposalState::HeldForReview,
            ),
            text: None,
            revision: Some(RevisionRecord {
                proposal: record.id.clone(),
                seq: 1,
                content,
                created_at: NOW,
            }),
            superseded_by: None,
        })
        .unwrap()
}

/// Put an event about `proposal` in `to`'s inbox.
pub fn tell(
    w: &World,
    to: &AccountId,
    kind: InboxEventKind,
    proposal: &ProposalRecord,
    by: Option<&AccountId>,
    at: i64,
) {
    w.store
        .push_event(&InboxEvent {
            id: InboxEventId::new(),
            account: to.clone(),
            wiki: proposal.wiki.clone(),
            proposal: Some(proposal.id.clone()),
            kind,
            actor: by.cloned(),
            created_at: at,
            read_at: None,
        })
        .unwrap();
}
