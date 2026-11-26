//! What moderation hides is hidden in the windows as everywhere (W-9; RFC 006 D-8): a proposal whose latest revision's summary is
//! hidden comes without its summary in a page of the list and in the events (both of its events), and the others are as they were.

use std::sync::Arc;

use inklkunn_store::{
    Actor, CommentId, CommentRecord, GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts,
    MemoryAudit, MemoryStore, ProposalId, ProposalRecord, ReasonCategory, Role, SqliteStore,
    TombstoneAct, TombstoneActId, TombstoneEvent, TombstoneEventId,
};

use super::support::{Env, Everything, T0, now, set_now};
use crate::authorize::{Authorizer, Principal, StoreAuthz};
use crate::proposal::{ProposalService, QuotaConfig};
use crate::tombstone::Tombstones;

fn roomy() -> QuotaConfig {
    QuotaConfig {
        proposals_per_hour_per_wiki: 1000,
        proposals_per_hour_per_principal: 1000,
        open_proposals: 1000,
        ..QuotaConfig::default()
    }
}

fn summaries(records: Vec<&ProposalRecord>) -> Vec<(ProposalId, String)> {
    records
        .into_iter()
        .map(|p| (p.id.clone(), p.summary.clone()))
        .collect()
}

fn on<S: Everything + GovernanceStore + 'static>(store: &Arc<S>) {
    let e = Env::over(store.clone(), roomy());
    let writer = e.person(Some(Role::Contributor));
    set_now(T0 + 10);
    let hidden = e.held(&writer, "Said something unsayable", "pages/a.md").id;
    set_now(T0 + 20);
    let plain = e.held(&writer, "Said something fine", "pages/b.md").id;
    set_now(T0 + 30);
    e.service
        .decline(&e.maintainer.principal, &e.wiki, &hidden, "No.")
        .unwrap();
    store
        .record_tombstone_event(&TombstoneEvent {
            id: TombstoneEventId::new(),
            act_id: TombstoneActId::new(),
            wiki: e.wiki.id.clone(),
            at: T0 + 40,
            act: TombstoneAct::Hide,
            item: HiddenItem {
                kind: HiddenItemKind::ProposalRevision,
                id: hidden.to_string(),
                reference: Some("1".to_owned()),
            },
            parts: HiddenParts {
                content: false,
                summary: true,
                author: false,
            },
            category: Some(ReasonCategory::Spam),
            note: None,
            affected: None,
            by: Actor {
                account: e.admin.id.clone(),
                rung: Role::Admin,
            },
            policy_block: None,
            of_event: None,
            appeal: None,
            declined_transition: None,
        })
        .unwrap();
    // A thread of two comments on the plain proposal, the first of which is hidden (its body).
    let comments: Vec<CommentRecord> = (0..2_i64)
        .map(|n| CommentRecord {
            id: CommentId::new(),
            proposal: plain.clone(),
            revision: 1,
            author: writer.id.clone(),
            parent: None,
            body: format!("Comment {n}"),
            anchor: None,
            created_at: T0 + 50 + n,
        })
        .collect();
    for comment in &comments {
        store
            .add_comment(comment, &[inklkunn_store::ProposalState::HeldForReview])
            .unwrap();
    }
    let first_comment = comments.first().unwrap().id.clone();
    store
        .record_tombstone_event(&TombstoneEvent {
            id: TombstoneEventId::new(),
            act_id: TombstoneActId::new(),
            wiki: e.wiki.id.clone(),
            at: T0 + 60,
            act: TombstoneAct::Hide,
            item: HiddenItem {
                kind: HiddenItemKind::ProposalComment,
                id: first_comment.to_string(),
                reference: None,
            },
            parts: HiddenParts {
                content: true,
                summary: false,
                author: false,
            },
            category: Some(ReasonCategory::Spam),
            note: None,
            affected: None,
            by: Actor {
                account: e.admin.id.clone(),
                rung: Role::Admin,
            },
            policy_block: None,
            of_event: None,
            appeal: None,
            declined_transition: None,
        })
        .unwrap();
    let service = ProposalService::new(
        store.clone(),
        store.clone(),
        store.clone(),
        Authorizer::new(
            Arc::new(StoreAuthz::new(store.clone(), store.clone())),
            Arc::new(MemoryAudit::new()),
        ),
        e.bases.clone(),
        QuotaConfig::default(),
    )
    .with_tombstones(Arc::new(Tombstones::new(store.clone(), store.clone())))
    .with_clock(now);
    let anyone = &Principal::Anonymous;
    let listed = service.list_sent(anyone, &e.wiki, None, None, 10).unwrap();
    assert_eq!(
        summaries(listed.iter().collect()),
        [
            (plain.clone(), "Said something fine".to_owned()),
            (hidden.clone(), String::new()),
        ]
    );
    // The thread's window: the hidden comment has no body and says why, the other is whole; one window, one read of what is
    // hidden for each comment of it.
    let page = service
        .comments_page(anyone, &e.wiki, &plain, None, 10)
        .unwrap()
        .unwrap();
    assert_eq!(
        page.comments
            .iter()
            .map(|c| c.body.as_str())
            .collect::<Vec<_>>(),
        ["", "Comment 1"]
    );
    assert_eq!(page.marks.len(), 1);
    assert!(page.marks.contains_key(&first_comment));
    let events = service.list_events(anyone, &e.wiki, 10).unwrap();
    // The decision and the opening of the hidden one, and the plain opening: the hidden summary is in none of them.
    assert_eq!(
        summaries(events.iter().map(|ev| &ev.proposal).collect()),
        [
            (hidden.clone(), String::new()),
            (plain, "Said something fine".to_owned()),
            (hidden, String::new()),
        ]
    );
}

#[test]
fn a_hidden_summary_is_hidden_in_the_list_and_in_the_events_on_the_memory_store() {
    on(&Arc::new(MemoryStore::new()));
}

#[test]
fn a_hidden_summary_is_hidden_in_the_list_and_in_the_events_on_sqlite() {
    on(&Arc::new(SqliteStore::open_in_memory().unwrap()));
}
