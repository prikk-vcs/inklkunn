//! The windows of the proposal reads (W-9; RFC 007 D-9) on both stores: the proposals **sent for review**, newest sent first, a
//! page of them after a place; the **events** of those proposals (an opening at the time it was sent, a decision at the time it
//! ended), newest first and cut in the store; and a page of a proposal's comments after a comment. Each is held to the same
//! answer on the in-memory store (the reference) and on SQLite, which does the work in its statement.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::{
    AccountId, AccountStore, CommentId, CommentRecord, MemoryStore, ProposalEvent,
    ProposalEventKind, ProposalId, ProposalOp, ProposalRecord, ProposalState, ProposalStore,
    SqliteStore, WikiStore,
};
use support::{account, mv, open_draft, step, submit, wiki};

fn comment(p: &ProposalId, who: &AccountId, at: i64) -> CommentRecord {
    CommentRecord {
        id: CommentId::new(),
        proposal: p.clone(),
        revision: 1,
        author: who.clone(),
        parent: None,
        body: "A word.".to_owned(),
        anchor: None,
        created_at: at,
    }
}

/// A proposal sent at `at`.
fn sent(
    store: &(impl ProposalStore + WikiStore + AccountStore),
    wiki: &inklkunn_store::WikiId,
    who: &AccountId,
    at: i64,
) -> ProposalRecord {
    let draft = open_draft(store, wiki, who, at - 1);
    submit(store, &draft, at)
}

/// Close a sent proposal at `at` in `to`.
fn close(
    store: &impl ProposalStore,
    p: &ProposalRecord,
    op: ProposalOp,
    to: ProposalState,
    who: &AccountId,
    at: i64,
) -> ProposalRecord {
    mv(store, &p.id, op, ProposalState::HeldForReview, to, who, at)
}

/// What the windows are asked of: ids named by the time they were sent, so that the expected answers are plain.
struct World {
    wiki: inklkunn_store::WikiId,
    /// Sent at 100, **accepted at 1000**: an old proposal with a new decision.
    old: ProposalId,
    /// Sent at 200, 200 and 200 (the same second): the id breaks the tie.
    same: [ProposalId; 3],
    /// Sent at 300, declined at 900.
    declined: ProposalId,
    /// Sent at 400 and declined at 400: both events at one moment.
    quick: ProposalId,
    /// Sent at 500, still held.
    held: ProposalId,
    /// Never sent: a draft, and one withdrawn before it was sent.
    drafts: [ProposalId; 2],
    /// A proposal of another wiki.
    elsewhere: ProposalId,
}

fn world(store: &(impl ProposalStore + WikiStore + AccountStore)) -> World {
    let (w, other) = (wiki(store, "one"), wiki(store, "two"));
    let (author, maintainer) = (account(store), account(store));
    let old = sent(store, &w.id, &author, 100);
    let same = [
        sent(store, &w.id, &author, 200),
        sent(store, &w.id, &author, 200),
        sent(store, &w.id, &author, 200),
    ];
    let declined = sent(store, &w.id, &author, 300);
    let quick = sent(store, &w.id, &author, 400);
    let held = sent(store, &w.id, &author, 500);
    let draft = open_draft(store, &w.id, &author, 600);
    let withdrawn_unsent = open_draft(store, &w.id, &author, 650);
    store
        .change_proposal(&inklkunn_store::ProposalChange {
            transition: step(
                &withdrawn_unsent.id,
                ProposalOp::Withdraw,
                Some(ProposalState::Draft),
                ProposalState::Withdrawn,
                &author,
                Some("Not needed."),
                700,
            ),
            text: None,
            revision: None,
            superseded_by: None,
        })
        .expect("withdrawn before it was sent");
    let elsewhere = sent(store, &other.id, &author, 450);
    close(
        store,
        &declined,
        ProposalOp::Decline,
        ProposalState::Declined,
        &maintainer,
        900,
    );
    close(
        store,
        &quick,
        ProposalOp::Decline,
        ProposalState::Declined,
        &maintainer,
        400,
    );
    close(
        store,
        &old,
        ProposalOp::Accept,
        ProposalState::Accepted,
        &maintainer,
        1000,
    );
    World {
        wiki: w.id,
        old: old.id,
        same: same.map(|p| p.id),
        declined: declined.id,
        quick: quick.id,
        held: held.id,
        drafts: [draft.id, withdrawn_unsent.id],
        elsewhere: elsewhere.id,
    }
}

fn sent_ids(
    store: &impl ProposalStore,
    w: &World,
    state: Option<ProposalState>,
    after: Option<(i64, &ProposalId)>,
    limit: usize,
) -> Vec<ProposalId> {
    store
        .proposals_sent(&w.wiki, state, after, limit)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

fn sent_window(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = world(store);
    let mut same = w.same.clone();
    // The id, compared as its text, breaks a tie: the greater first.
    same.sort_by(|a, b| b.as_str().cmp(a.as_str()));
    let [s0, s1, s2] = same;
    let whole = [
        w.held.clone(),
        w.quick.clone(),
        w.declined.clone(),
        s0,
        s1,
        s2,
        w.old.clone(),
    ];
    assert_eq!(sent_ids(store, &w, None, None, 100), whole);
    for draft in w.drafts.iter().chain([&w.elsewhere]) {
        assert!(!whole.contains(draft));
    }

    // A window is the first `limit` of the whole; nothing at zero.
    assert_eq!(sent_ids(store, &w, None, None, 3), whole[..3]);
    assert!(sent_ids(store, &w, None, None, 0).is_empty());

    // Walked by the place the page before ended: each once, in order, whatever a page's size.
    for size in 1..=4_usize {
        let mut walked: Vec<ProposalId> = Vec::new();
        let mut after: Option<(i64, ProposalId)> = None;
        for _ in 0..50 {
            let page = store
                .proposals_sent(
                    &w.wiki,
                    None,
                    after.as_ref().map(|(at, id)| (*at, id)),
                    size,
                )
                .unwrap();
            let Some(last) = page.last() else { break };
            after = Some((last.submitted_at.unwrap(), last.id.clone()));
            walked.extend(page.into_iter().map(|p| p.id));
        }
        assert_eq!(walked, whole, "pages of {size}");
    }

    // A state narrows it; a draft is no state the window has.
    assert_eq!(
        sent_ids(store, &w, Some(ProposalState::Declined), None, 100),
        [w.quick.clone(), w.declined.clone()]
    );
    assert_eq!(
        sent_ids(store, &w, Some(ProposalState::HeldForReview), None, 100)
            .first()
            .cloned(),
        Some(w.held.clone())
    );
    assert!(sent_ids(store, &w, Some(ProposalState::Draft), None, 100).is_empty());
    assert!(sent_ids(store, &w, Some(ProposalState::Withdrawn), None, 100).is_empty());
    // The place is the time and the id, not the state: a state that moved leaves it where it was.
    assert_eq!(
        sent_ids(store, &w, Some(ProposalState::Accepted), None, 100),
        [w.old.clone()]
    );
}

fn event_ids(events: &[ProposalEvent]) -> Vec<(ProposalEventKind, i64, ProposalId)> {
    events
        .iter()
        .map(|e| (e.kind, e.at, e.proposal.id.clone()))
        .collect()
}

fn event_window(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    use ProposalEventKind::{Decided, Opened};
    let w = world(store);
    let mut same = w.same.clone();
    same.sort_by(|a, b| b.as_str().cmp(a.as_str()));
    let [s0, s1, s2] = same;
    let whole = vec![
        (Decided, 1000, w.old.clone()),
        (Decided, 900, w.declined.clone()),
        (Opened, 500, w.held.clone()),
        // Both events of one proposal at one moment: the opening first.
        (Opened, 400, w.quick.clone()),
        (Decided, 400, w.quick.clone()),
        (Opened, 300, w.declined.clone()),
        (Opened, 200, s0),
        (Opened, 200, s1),
        (Opened, 200, s2),
        (Opened, 100, w.old.clone()),
    ];
    assert_eq!(
        event_ids(&store.proposal_events(&w.wiki, 100).unwrap()),
        whole
    );
    // The window is the newest events by their own time: the decision of the oldest proposal is the newest event.
    assert_eq!(
        event_ids(&store.proposal_events(&w.wiki, 2).unwrap()),
        whole[..2]
    );
    assert_eq!(
        event_ids(&store.proposal_events(&w.wiki, 4).unwrap()),
        whole[..4],
        "the window cuts between the two events of a proposal as it cuts anywhere"
    );
    assert!(store.proposal_events(&w.wiki, 0).unwrap().is_empty());
    // The proposal comes as it is now.
    let first = store.proposal_events(&w.wiki, 1).unwrap();
    assert_eq!(
        first.first().map(|e| e.proposal.state),
        Some(ProposalState::Accepted)
    );
}

fn comment_window(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "one");
    let who = account(store);
    let p = sent(store, &w.id, &who, 100);
    let q = sent(store, &w.id, &who, 200);
    let mine: Vec<CommentRecord> = (0..5).map(|n| comment(&p.id, &who, 300 + n)).collect();
    for c in &mine {
        store
            .add_comment(c, &[ProposalState::HeldForReview])
            .unwrap();
    }
    let elsewhere = comment(&q.id, &who, 400);
    store
        .add_comment(&elsewhere, &[ProposalState::HeldForReview])
        .unwrap();
    let ids = |page: Option<Vec<CommentRecord>>| -> Vec<CommentId> {
        page.expect("a place of this thread")
            .into_iter()
            .map(|c| c.id)
            .collect()
    };
    let all: Vec<CommentId> = mine.iter().map(|c| c.id.clone()).collect();
    assert_eq!(
        ids(store.comments_after(&p.id, None, None, 100).unwrap()),
        all
    );
    assert_eq!(
        ids(store.comments_after(&p.id, None, None, 2).unwrap()),
        all[..2]
    );
    let after = all.get(1).unwrap();
    assert_eq!(
        ids(store.comments_after(&p.id, None, Some(after), 2).unwrap()),
        all[2..4]
    );
    let last = all.last().unwrap();
    assert!(
        ids(store.comments_after(&p.id, None, Some(last), 2).unwrap()).is_empty(),
        "the end of the thread is an empty window, not a refusal"
    );
    // A place that is a comment of another proposal, or none at all, is no place of this thread.
    assert!(
        store
            .comments_after(&p.id, None, Some(&elsewhere.id), 2)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .comments_after(&p.id, None, Some(&CommentId::new()), 2)
            .unwrap()
            .is_none()
    );
    // What the whole read gives, the windows give in pieces.
    assert_eq!(
        store.comments(&p.id, None).unwrap().len(),
        mine.len(),
        "the whole thread is the five"
    );
}

#[test]
fn the_memory_store_windows_what_was_sent() {
    sent_window(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_windows_what_was_sent() {
    sent_window(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_memory_store_windows_the_events_by_their_own_time() {
    event_window(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_windows_the_events_by_their_own_time() {
    event_window(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_memory_store_windows_a_threads_comments() {
    comment_window(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_windows_a_threads_comments() {
    comment_window(&SqliteStore::open_in_memory().expect("open"));
}
