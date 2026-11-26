//! The lists cost what they show (W-9; RFC 007 D-9): a page of the proposals that were sent, the newest events of the wiki and a
//! page of a proposal's comments are **read from the store as a window**, never the wiki's or the thread's whole and then cut
//! (the cost is counted by a store that sees every read, not timed), and the events are chosen by the time of the event, so a
//! decision made now on an old proposal is among the newest.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use inklkunn_store::{
    AccountId, CommentId, CommentRecord, DecisionRecord, MemoryAudit, ProposalChange,
    ProposalEvent, ProposalEventKind, ProposalFilter, ProposalId, ProposalRecord, ProposalState,
    ProposalStore, ProposalTransition, RevisionContent, RevisionFlags, RevisionRecord, Role,
    StoreError, WikiId,
};

use super::support::{Env, Everything, Kind, T0, now, set_now};
use crate::authorize::{Authorizer, Principal, StoreAuthz};
use crate::proposal::{ProposalService, QuotaConfig};

type Stored<T> = Result<T, StoreError>;

fn roomy() -> QuotaConfig {
    QuotaConfig {
        proposals_per_hour_per_wiki: 100_000,
        proposals_per_hour_per_principal: 100_000,
        open_proposals: 100_000,
        ..QuotaConfig::default()
    }
}

/// What the store was asked to read: the rows of the windows it gave, and the times it was asked for **the whole** (every
/// proposal of a wiki, every comment of a thread).
#[derive(Default)]
struct Counts {
    window_rows: AtomicUsize,
    whole_reads: AtomicUsize,
}

impl Counts {
    fn rows(&self) -> usize {
        self.window_rows.load(Ordering::SeqCst)
    }

    fn wholes(&self) -> usize {
        self.whole_reads.load(Ordering::SeqCst)
    }

    fn reset(&self) {
        self.window_rows.store(0, Ordering::SeqCst);
        self.whole_reads.store(0, Ordering::SeqCst);
    }
}

/// The proposal records of a store, counting what the lists read. Every read the lists make goes through it.
struct Spy {
    inner: Arc<dyn Everything>,
    counts: Arc<Counts>,
}

impl ProposalStore for Spy {
    fn create_proposal(
        &self,
        p: &ProposalRecord,
        d: &RevisionContent,
        t: &ProposalTransition,
    ) -> Stored<()> {
        self.inner.create_proposal(p, d, t)
    }
    fn create_rollback_proposal(
        &self,
        p: &ProposalRecord,
        d: &RevisionContent,
        t: &ProposalTransition,
        undoes: &str,
    ) -> Stored<()> {
        self.inner.create_rollback_proposal(p, d, t, undoes)
    }
    fn rollback_of(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Option<String>> {
        self.inner.rollback_of(id, v)
    }
    fn revision_flags(&self, ids: &[ProposalId]) -> Stored<Vec<(ProposalId, RevisionFlags)>> {
        self.inner.revision_flags(ids)
    }
    fn proposal(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Option<ProposalRecord>> {
        self.inner.proposal(id, v)
    }
    fn proposals(
        &self,
        w: &WikiId,
        f: &ProposalFilter,
        v: Option<&AccountId>,
    ) -> Stored<Vec<ProposalRecord>> {
        self.counts.whole_reads.fetch_add(1, Ordering::SeqCst);
        self.inner.proposals(w, f, v)
    }
    fn proposals_sent(
        &self,
        w: &WikiId,
        state: Option<ProposalState>,
        after: Option<(i64, &ProposalId)>,
        limit: usize,
    ) -> Stored<Vec<ProposalRecord>> {
        let rows = self.inner.proposals_sent(w, state, after, limit)?;
        self.counts
            .window_rows
            .fetch_add(rows.len(), Ordering::SeqCst);
        Ok(rows)
    }
    fn proposal_events(&self, w: &WikiId, limit: usize) -> Stored<Vec<ProposalEvent>> {
        let rows = self.inner.proposal_events(w, limit)?;
        self.counts
            .window_rows
            .fetch_add(rows.len(), Ordering::SeqCst);
        Ok(rows)
    }
    fn comments_after(
        &self,
        id: &ProposalId,
        v: Option<&AccountId>,
        after: Option<&CommentId>,
        limit: usize,
    ) -> Stored<Option<Vec<CommentRecord>>> {
        let rows = self.inner.comments_after(id, v, after, limit)?;
        self.counts
            .window_rows
            .fetch_add(rows.as_ref().map_or(0, Vec::len), Ordering::SeqCst);
        Ok(rows)
    }
    fn draft(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Option<RevisionContent>> {
        self.inner.draft(id, v)
    }
    fn save_draft(
        &self,
        id: &ProposalId,
        summary: &str,
        sources: &[String],
        d: &RevisionContent,
        at: i64,
    ) -> Stored<()> {
        self.inner.save_draft(id, summary, sources, d, at)
    }
    fn change_proposal(&self, c: &ProposalChange) -> Stored<ProposalRecord> {
        self.inner.change_proposal(c)
    }
    fn revisions(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Vec<RevisionRecord>> {
        self.inner.revisions(id, v)
    }
    fn transitions(
        &self,
        id: &ProposalId,
        v: Option<&AccountId>,
    ) -> Stored<Vec<ProposalTransition>> {
        self.inner.transitions(id, v)
    }
    fn add_comment(&self, c: &CommentRecord, states: &[ProposalState]) -> Stored<()> {
        self.inner.add_comment(c, states)
    }
    fn comments(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Vec<CommentRecord>> {
        self.counts.whole_reads.fetch_add(1, Ordering::SeqCst);
        self.inner.comments(id, v)
    }
    fn add_decision(&self, d: &DecisionRecord) -> Stored<()> {
        self.inner.add_decision(d)
    }
    fn decisions(&self, id: &ProposalId, v: Option<&AccountId>) -> Stored<Vec<DecisionRecord>> {
        self.inner.decisions(id, v)
    }
    fn wiki_decisions(&self, w: &WikiId) -> Stored<Vec<DecisionRecord>> {
        self.inner.wiki_decisions(w)
    }
    fn open_count(&self, w: &WikiId, a: &AccountId) -> Stored<u64> {
        self.inner.open_count(w, a)
    }
    fn opened_since(&self, w: &WikiId, a: Option<&AccountId>, since: i64) -> Stored<Vec<i64>> {
        self.inner.opened_since(w, a, since)
    }
    fn submitted_since(&self, w: &WikiId, since: i64) -> Stored<Vec<(AccountId, i64)>> {
        self.inner.submitted_since(w, since)
    }
    fn comments_since(&self, w: &WikiId, a: &AccountId, since: i64) -> Stored<Vec<i64>> {
        self.inner.comments_since(w, a, since)
    }
}

/// The same world read through a service whose proposal records are the spy: the lists under test go through it, the world
/// is built with the real one.
fn spied(e: &Env) -> (ProposalService, Arc<Counts>) {
    let counts = Arc::new(Counts::default());
    let spy = Spy {
        inner: e.store.clone(),
        counts: counts.clone(),
    };
    let authz = StoreAuthz::new(e.store.clone(), e.store.clone());
    let authorizer = Authorizer::new(Arc::new(authz), Arc::new(MemoryAudit::new()));
    let service = ProposalService::new(
        Arc::new(spy),
        e.store.clone(),
        e.store.clone(),
        authorizer,
        e.bases.clone(),
        QuotaConfig::default(),
    )
    .with_clock(now);
    (service, counts)
}

/// `n` proposals sent, one a second, by one author (the clock is the test's).
fn send_many(e: &Env, n: i64) {
    let writer = e.person(Some(Role::Contributor));
    for k in 0..n {
        set_now(T0 + k);
        e.held(&writer, &format!("Proposal {k}"), &format!("pages/p{k}.md"));
    }
}

fn kinds(events: &[ProposalEvent]) -> Vec<(ProposalEventKind, ProposalId)> {
    events
        .iter()
        .map(|e| (e.kind, e.proposal.id.clone()))
        .collect()
}

#[test]
fn a_page_of_the_list_reads_the_page_and_not_the_wiki() {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        send_many(&e, 2_000);
        let writer = e.person(Some(Role::Contributor));
        // Some drafts beside them, which no list reads.
        e.draft(&writer, "A draft", "pages/draft.md");
        let (service, counts) = spied(&e);
        let anyone = &Principal::Anonymous;

        // The first page, as the API asks for it: a page of 20 and one more, to know whether it goes on.
        let page = service.list_sent(anyone, &e.wiki, None, None, 21).unwrap();
        assert_eq!(page.len(), 21, "{kind:?}");
        assert_eq!(counts.rows(), 21, "{kind:?}: only the page's rows are read");
        assert_eq!(
            counts.wholes(),
            0,
            "{kind:?}: the wiki's proposals are not loaded to be cut"
        );

        // A page from the middle, narrowed to a state: the same.
        counts.reset();
        let place = page
            .get(20)
            .and_then(|p| Some((p.submitted_at?, p.id.clone())))
            .unwrap();
        let next = service
            .list_sent(
                anyone,
                &e.wiki,
                Some(ProposalState::HeldForReview),
                Some(&place),
                21,
            )
            .unwrap();
        assert_eq!(next.len(), 21, "{kind:?}");
        assert_eq!(counts.rows(), 21, "{kind:?}");
        assert_eq!(counts.wholes(), 0, "{kind:?}");
    }
}

#[test]
fn the_feed_reads_its_window_of_events_and_not_the_wiki() {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        send_many(&e, 2_000);
        let (service, counts) = spied(&e);
        let events = service
            .list_events(&Principal::Anonymous, &e.wiki, 50)
            .unwrap();
        assert_eq!(events.len(), 50, "{kind:?}");
        assert_eq!(
            counts.rows(),
            50,
            "{kind:?}: only the window's rows are read"
        );
        assert_eq!(counts.wholes(), 0, "{kind:?}: no whole read of the wiki");
    }
}

#[test]
fn a_page_of_a_threads_comments_reads_the_page_and_not_the_thread() {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        let writer = e.person(Some(Role::Contributor));
        let p = e.held(&writer, "Talked about", "pages/a.md");
        let all: Vec<CommentRecord> = (0..300_i64)
            .map(|n| CommentRecord {
                id: CommentId::new(),
                proposal: p.id.clone(),
                revision: 1,
                author: writer.id.clone(),
                parent: None,
                body: format!("Comment {n}"),
                anchor: None,
                created_at: T0 + n,
            })
            .collect();
        for comment in &all {
            e.store
                .add_comment(comment, &[ProposalState::HeldForReview])
                .unwrap();
        }
        let (service, counts) = spied(&e);
        let anyone = &Principal::Anonymous;
        let page = service
            .comments_page(anyone, &e.wiki, &p.id, None, 21)
            .unwrap()
            .unwrap();
        assert_eq!(page.comments.len(), 21, "{kind:?}");
        assert_eq!(counts.rows(), 21, "{kind:?}: only the page's comments");
        assert_eq!(
            counts.wholes(),
            0,
            "{kind:?}: the thread is not loaded to be cut"
        );

        // Walked 100 at a time, the thread comes back whole and in order; a place of no comment of it is none.
        let mut walked: Vec<CommentId> = Vec::new();
        let mut after: Option<CommentId> = None;
        for _ in 0..50 {
            let page = service
                .comments_page(anyone, &e.wiki, &p.id, after.as_ref(), 100)
                .unwrap()
                .unwrap();
            let Some(last) = page.comments.last() else {
                break;
            };
            after = Some(last.id.clone());
            walked.extend(page.comments.into_iter().map(|c| c.id));
        }
        assert_eq!(
            walked,
            all.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            "{kind:?}"
        );
        assert!(
            service
                .comments_page(anyone, &e.wiki, &p.id, Some(&CommentId::new()), 10)
                .unwrap()
                .is_none(),
            "{kind:?}"
        );
        assert_eq!(counts.wholes(), 0, "{kind:?}");
    }
}

/// The ids of `proposals` in the list's order: newest sent first, the id breaking a tie.
fn in_list_order(mut sent: Vec<(i64, ProposalId)>) -> Vec<ProposalId> {
    sent.sort_by(|a, b| (b.0, b.1.as_str()).cmp(&(a.0, a.1.as_str())));
    sent.into_iter().map(|(_, id)| id).collect()
}

#[test]
fn pages_over_a_cursor_reproduce_the_whole_order_with_proposals_sent_in_one_second_and_a_state_changing_between_pages()
 {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        let writer = e.person(Some(Role::Contributor));
        let mut sent: Vec<(i64, ProposalId)> = Vec::new();
        // Nine proposals in three seconds, three in each: the id alone orders the three of a second.
        for n in 0..9_i64 {
            set_now(T0 + 100 * (n / 3));
            let p = e.held(&writer, &format!("Proposal {n}"), &format!("pages/{n}.md"));
            sent.push((p.submitted_at.unwrap(), p.id));
        }
        let whole = in_list_order(sent.clone());
        let anyone = &Principal::Anonymous;
        for size in [1_usize, 2, 4, 5] {
            let mut walked: Vec<ProposalId> = Vec::new();
            let mut after: Option<(i64, ProposalId)> = None;
            for _ in 0..50 {
                let page = e
                    .service
                    .list_sent(anyone, &e.wiki, None, after.as_ref(), size)
                    .unwrap();
                let Some(last) = page.last() else { break };
                after = last.submitted_at.map(|at| (at, last.id.clone()));
                walked.extend(page.into_iter().map(|p| p.id));
            }
            assert_eq!(walked, whole, "{kind:?}: pages of {size}");
        }

        // A proposal decided between two pages does not move the place, in a list of every state and in the list of its old
        // state alone (it is then simply no longer there).
        let first = e.service.list_sent(anyone, &e.wiki, None, None, 4).unwrap();
        let place = first
            .last()
            .and_then(|p| Some((p.submitted_at?, p.id.clone())))
            .unwrap();
        let decided = whole.get(4).unwrap().clone();
        set_now(T0 + 5_000);
        e.service
            .decline(&e.maintainer.principal, &e.wiki, &decided, "No.")
            .unwrap();
        let rest: Vec<ProposalId> = e
            .service
            .list_sent(anyone, &e.wiki, None, Some(&place), 100)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(rest, whole[4..], "{kind:?}: the place does not move");
        let held_rest: Vec<ProposalId> = e
            .service
            .list_sent(
                anyone,
                &e.wiki,
                Some(ProposalState::HeldForReview),
                Some(&place),
                100,
            )
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        let expected: Vec<ProposalId> = whole[4..]
            .iter()
            .filter(|id| **id != decided)
            .cloned()
            .collect();
        assert_eq!(held_rest, expected, "{kind:?}: a state and a place");
        assert_eq!(
            e.service
                .list_sent(
                    anyone,
                    &e.wiki,
                    Some(ProposalState::Declined),
                    Some(&place),
                    100
                )
                .unwrap()
                .into_iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            [decided],
            "{kind:?}"
        );
    }
}

#[test]
fn a_recent_decision_on_an_old_proposal_is_among_the_newest_events_and_an_old_opening_falls_out_of_the_window()
 {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        let writer = e.person(Some(Role::Contributor));
        set_now(T0 + 10);
        let old = e.held(&writer, "Old", "pages/old.md").id;
        set_now(T0 + 20);
        let middle = e.held(&writer, "Middle", "pages/middle.md").id;
        set_now(T0 + 30);
        let newer = e.held(&writer, "Newer", "pages/newer.md").id;
        let anyone = &Principal::Anonymous;

        // Three openings, newest first, and nothing else has happened.
        let before = e.service.list_events(anyone, &e.wiki, 100).unwrap();
        assert_eq!(
            kinds(&before),
            [
                (ProposalEventKind::Opened, newer.clone()),
                (ProposalEventKind::Opened, middle.clone()),
                (ProposalEventKind::Opened, old.clone()),
            ],
            "{kind:?}"
        );

        // The oldest proposal is declined now: its decision is the newest event, above the opening of a proposal sent after it
        // and before the decision. A feed that took the newest-sent proposals first would not show it in a small window.
        set_now(T0 + 40);
        e.service
            .decline(&e.maintainer.principal, &e.wiki, &old, "No.")
            .unwrap();
        let after = e.service.list_events(anyone, &e.wiki, 100).unwrap();
        assert_eq!(
            kinds(&after),
            [
                (ProposalEventKind::Decided, old.clone()),
                (ProposalEventKind::Opened, newer.clone()),
                (ProposalEventKind::Opened, middle.clone()),
                (ProposalEventKind::Opened, old.clone()),
            ],
            "{kind:?}"
        );
        assert_eq!(after.first().map(|ev| ev.at), Some(T0 + 40), "{kind:?}");
        assert_eq!(
            after.first().map(|ev| ev.proposal.state),
            Some(ProposalState::Declined),
            "{kind:?}: the proposal as it is now"
        );

        // A window of two holds the decision and the newest opening; the older openings fall out of it.
        assert_eq!(
            kinds(&e.service.list_events(anyone, &e.wiki, 2).unwrap()),
            [
                (ProposalEventKind::Decided, old.clone()),
                (ProposalEventKind::Opened, newer.clone()),
            ],
            "{kind:?}"
        );

        // A proposal sent later pushes the oldest opening out of a window of four.
        set_now(T0 + 50);
        let latest = e.held(&writer, "Latest", "pages/latest.md").id;
        assert_eq!(
            kinds(&e.service.list_events(anyone, &e.wiki, 4).unwrap()),
            [
                (ProposalEventKind::Opened, latest),
                (ProposalEventKind::Decided, old.clone()),
                (ProposalEventKind::Opened, newer),
                (ProposalEventKind::Opened, middle),
            ],
            "{kind:?}: the oldest opening is out"
        );
    }
}

#[test]
fn no_draft_has_an_event_not_even_the_readers_own_and_a_proposal_withdrawn_before_it_was_sent_has_none()
 {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::with(kind, roomy());
        let writer = e.person(Some(Role::Contributor));
        set_now(T0 + 10);
        let sent = e.held(&writer, "Sent", "pages/a.md").id;
        set_now(T0 + 20);
        let own_draft = e.draft(&writer, "A draft", "pages/b.md").id;
        let never_sent = e.draft(&writer, "Never sent", "pages/c.md");
        set_now(T0 + 30);
        e.service
            .withdraw(&writer.principal, &e.wiki, &never_sent.id, "Not needed.")
            .unwrap();
        for (who, name) in [
            (&Principal::Anonymous, "anonymous"),
            (&writer.principal, "the author of the drafts"),
            (&e.maintainer.principal, "a maintainer"),
        ] {
            let events = e.service.list_events(who, &e.wiki, 100).unwrap();
            assert_eq!(
                kinds(&events),
                [(ProposalEventKind::Opened, sent.clone())],
                "{kind:?}: {name}"
            );
            assert!(
                !events
                    .iter()
                    .any(|ev| ev.proposal.id == own_draft || ev.proposal.id == never_sent.id),
                "{kind:?}: {name}: a draft has an event"
            );
        }
        // A proposal that was sent and then withdrawn has both.
        set_now(T0 + 40);
        e.service
            .withdraw(&writer.principal, &e.wiki, &sent, "Taken back.")
            .unwrap();
        assert_eq!(
            kinds(
                &e.service
                    .list_events(&Principal::Anonymous, &e.wiki, 100)
                    .unwrap()
            ),
            [
                (ProposalEventKind::Decided, sent.clone()),
                (ProposalEventKind::Opened, sent),
            ],
            "{kind:?}"
        );
    }
}
