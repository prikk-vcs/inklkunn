//! The proposal records on both stores (handoff 004 T1; RFC 004 D-1, D-3): one invariant suite run on the
//! in-memory and the SQLite store, drafts that only their author can see, the write-once records refused on
//! both stores and, for SQLite, refused through SQL as well, reopen persistence, and the migration from a
//! version 8 database.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    AccountStore, Anchor, CommentId, CommentRecord, DecisionId, DecisionKind, DecisionRecord,
    MemoryStore, ProposalChange, ProposalFilter, ProposalId, ProposalOp, ProposalState,
    ProposalStore, Quote, SqliteStore, StoreError, WikiStore,
};
use support::{account, content, mv, nth, open_draft, revision, step, submit, temp, wiki};

type Result<T> = std::result::Result<T, StoreError>;

fn decision(p: &ProposalId, rev: u32, who: &inklkunn_store::AccountId, at: i64) -> DecisionRecord {
    DecisionRecord {
        id: DecisionId::new(),
        proposal: p.clone(),
        revision: rev,
        reviewer: who.clone(),
        kind: DecisionKind::Approve,
        body: "Looks right".to_owned(),
        created_at: at,
    }
}

fn comment(p: &ProposalId, rev: u32, who: &inklkunn_store::AccountId, at: i64) -> CommentRecord {
    CommentRecord {
        id: CommentId::new(),
        proposal: p.clone(),
        revision: rev,
        author: who.clone(),
        parent: None,
        body: "Why this word?".to_owned(),
        anchor: None,
        created_at: at,
    }
}

const REVIEWABLE: [ProposalState; 2] = [ProposalState::HeldForReview, ProposalState::NeedsRevision];

fn the_suite(store: &(impl ProposalStore + WikiStore + AccountStore)) {
    let w = wiki(store, "one");
    let other_wiki = wiki(store, "two");
    let (author, stranger, reviewer) = (account(store), account(store), account(store));

    // --- A draft is its author's alone; to everyone else it is absent.
    let p = open_draft(store, &w.id, &author, 100);
    assert_eq!(
        store.proposal(&p.id, Some(&author)).unwrap().as_ref(),
        Some(&p)
    );
    assert_eq!(store.proposal(&p.id, Some(&stranger)).unwrap(), None);
    assert_eq!(store.proposal(&p.id, None).unwrap(), None);
    let all = ProposalFilter::default();
    assert_eq!(
        store.proposals(&w.id, &all, Some(&author)).unwrap().len(),
        1
    );
    assert!(
        store
            .proposals(&w.id, &all, Some(&stranger))
            .unwrap()
            .is_empty()
    );
    assert!(store.proposals(&w.id, &all, None).unwrap().is_empty());
    assert!(store.draft(&p.id, Some(&stranger)).unwrap().is_none());
    assert!(store.draft(&p.id, None).unwrap().is_none());
    assert_eq!(
        store.draft(&p.id, Some(&author)).unwrap(),
        Some(content(1, "draft text"))
    );
    assert_eq!(store.transitions(&p.id, Some(&author)).unwrap().len(), 1);
    assert!(
        store
            .transitions(&p.id, Some(&stranger))
            .unwrap()
            .is_empty()
    );

    // --- Creating refuses what is not there, and a repeated id.
    let again = store.create_proposal(
        &p,
        &content(1, "x"),
        &step(
            &p.id,
            ProposalOp::Create,
            None,
            ProposalState::Draft,
            &author,
            None,
            100,
        ),
    );
    assert_eq!(again, Err(StoreError::IdTaken(p.id.to_string())));
    let mut orphan = support::draft_record(&w.id, &inklkunn_store::AccountId::new(), 100);
    let made = |r: &inklkunn_store::ProposalRecord| {
        store.create_proposal(
            r,
            &content(1, "x"),
            &step(
                &r.id,
                ProposalOp::Create,
                None,
                ProposalState::Draft,
                &author,
                None,
                100,
            ),
        )
    };
    assert_eq!(made(&orphan), Err(StoreError::AccountNotFound));
    orphan = support::draft_record(&inklkunn_store::WikiId::new(), &author, 100);
    assert_eq!(made(&orphan), Err(StoreError::NotFound));

    // --- Saving the draft replaces its summary, sources and working copy, and only the author sees them.
    store
        .save_draft(&p.id, "Fix two typos", &[], &content(2, "better text"), 150)
        .unwrap();
    let saved = store.proposal(&p.id, Some(&author)).unwrap().unwrap();
    assert_eq!(saved.summary, "Fix two typos");
    assert!(saved.sources.is_empty());
    assert_eq!(saved.updated_at, 150);
    assert_eq!(
        store.draft(&p.id, Some(&author)).unwrap(),
        Some(content(2, "better text"))
    );
    assert_eq!(
        store.save_draft(&ProposalId::new(), "s", &[], &content(1, "x"), 1),
        Err(StoreError::NotFound)
    );

    // --- A draft can neither be commented on nor decided.
    assert_eq!(
        store.add_comment(&comment(&p.id, 0, &author, 160), &REVIEWABLE),
        Err(StoreError::StateChanged)
    );
    assert_eq!(
        store.add_decision(&decision(&p.id, 1, &reviewer, 160)),
        Err(StoreError::StateChanged)
    );

    // --- Submitting writes revision 1, takes the working copy away and shows the proposal to everyone.
    let held = submit(store, &saved, 200);
    assert_eq!(held.state, ProposalState::HeldForReview);
    assert_eq!(held.submitted_at, Some(200));
    assert_eq!(held.closed_at, None);
    assert_eq!(
        store.proposal(&p.id, Some(&stranger)).unwrap(),
        Some(held.clone())
    );
    assert_eq!(store.proposal(&p.id, None).unwrap(), Some(held.clone()));
    assert_eq!(store.draft(&p.id, Some(&author)).unwrap(), None);
    assert_eq!(
        store.save_draft(&p.id, "late", &[], &content(1, "x"), 210),
        Err(StoreError::StateChanged)
    );
    let log = store.transitions(&p.id, Some(&stranger)).unwrap();
    assert_eq!(
        log.iter().map(|t| (t.op, t.to)).collect::<Vec<_>>(),
        vec![
            (ProposalOp::Create, ProposalState::Draft),
            (ProposalOp::Submit, ProposalState::HeldForReview)
        ]
    );
    assert_eq!(store.revisions(&p.id, None).unwrap().len(), 1);

    // --- A change names the state it was made from; a stale one changes nothing.
    let stale = store.change_proposal(&ProposalChange {
        transition: step(
            &p.id,
            ProposalOp::Decline,
            Some(ProposalState::Draft),
            ProposalState::Declined,
            &reviewer,
            Some("no"),
            210,
        ),
        text: None,
        revision: None,
        superseded_by: None,
    });
    assert_eq!(stale, Err(StoreError::StateChanged));
    assert_eq!(store.proposal(&p.id, None).unwrap(), Some(held.clone()));
    assert_eq!(store.transitions(&p.id, None).unwrap().len(), 2);
    let unknown = store.change_proposal(&ProposalChange {
        transition: step(
            &ProposalId::new(),
            ProposalOp::Decline,
            Some(ProposalState::HeldForReview),
            ProposalState::Declined,
            &reviewer,
            Some("no"),
            210,
        ),
        text: None,
        revision: None,
        superseded_by: None,
    });
    assert_eq!(unknown, Err(StoreError::NotFound));

    // --- Revisions are numbered one after another; a wrong number changes nothing at all.
    for bad in [1, 3] {
        let r: Result<_> = store.change_proposal(&ProposalChange {
            transition: step(
                &p.id,
                ProposalOp::Revise,
                Some(ProposalState::HeldForReview),
                ProposalState::HeldForReview,
                &author,
                None,
                220,
            ),
            text: Some(("Changed".to_owned(), Vec::new())),
            revision: Some(revision(&p.id, bad, "second", 220)),
            superseded_by: None,
        });
        assert_eq!(r, Err(StoreError::StateChanged), "revision {bad}");
    }
    let unchanged = store.proposal(&p.id, None).unwrap().unwrap();
    assert_eq!(
        unchanged.summary, "Fix two typos",
        "the text did not change with a refused revision"
    );
    assert_eq!(store.revisions(&p.id, None).unwrap().len(), 1);
    assert_eq!(store.transitions(&p.id, None).unwrap().len(), 2);

    // --- A comment is on the latest revision, in one of the allowed states, replying within the proposal.
    let c1 = comment(&p.id, 1, &stranger, 230);
    store.add_comment(&c1, &REVIEWABLE).unwrap();
    assert_eq!(
        store.add_comment(&c1, &REVIEWABLE),
        Err(StoreError::AlreadyRecorded)
    );
    let mut reply = comment(&p.id, 1, &author, 231);
    reply.parent = Some(c1.id.clone());
    reply.anchor = Some(Anchor {
        page: "pages/home.md".to_owned(),
        block_id: Some("b7".to_owned()),
        quote: Some(Quote {
            exact: "word".to_owned(),
            prefix: "this ".to_owned(),
            suffix: " here".to_owned(),
        }),
        ordinal: 2,
    });
    store.add_comment(&reply, &REVIEWABLE).unwrap();
    let mut lost = comment(&p.id, 1, &author, 232);
    lost.parent = Some(CommentId::new());
    assert_eq!(
        store.add_comment(&lost, &REVIEWABLE),
        Err(StoreError::NotFound)
    );
    assert_eq!(
        store.add_comment(
            &comment(&p.id, 1, &author, 232),
            &[ProposalState::NeedsRevision]
        ),
        Err(StoreError::StateChanged),
        "the state is not one of those the comment may be made in"
    );
    assert_eq!(
        store.add_comment(&comment(&p.id, 2, &author, 232), &REVIEWABLE),
        Err(StoreError::StateChanged),
        "there is no revision 2 yet"
    );
    assert_eq!(
        store.add_comment(
            &comment(&p.id, 1, &inklkunn_store::AccountId::new(), 232),
            &REVIEWABLE
        ),
        Err(StoreError::AccountNotFound)
    );
    let listed = store.comments(&p.id, None).unwrap();
    assert_eq!(
        listed,
        vec![c1, reply],
        "comments keep their order and their anchors"
    );
    assert!(store.comments(&ProposalId::new(), None).unwrap().is_empty());

    // --- A decision is bound to a revision; one per reviewer per revision; refused twice, both stores.
    let d1 = decision(&p.id, 1, &reviewer, 240);
    store.add_decision(&d1).unwrap();
    for again in [
        decision(&p.id, 1, &reviewer, 241),
        DecisionRecord {
            id: d1.id.clone(),
            reviewer: stranger.clone(),
            ..d1.clone()
        },
    ] {
        assert_eq!(store.add_decision(&again), Err(StoreError::AlreadyRecorded));
    }
    assert_eq!(store.decisions(&p.id, None).unwrap(), vec![d1.clone()]);
    assert_eq!(
        store.add_decision(&decision(&p.id, 2, &stranger, 242)),
        Err(StoreError::StateChanged),
        "there is no revision 2 to decide on"
    );
    assert_eq!(
        store.add_decision(&decision(&p.id, 1, &inklkunn_store::AccountId::new(), 243)),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(
        store.add_decision(&decision(&ProposalId::new(), 1, &reviewer, 243)),
        Err(StoreError::NotFound)
    );

    // --- A revision supersedes the decision for acceptance, which stays recorded; the same reviewer may
    //     decide on the new revision.
    let revised = store
        .change_proposal(&ProposalChange {
            transition: step(
                &p.id,
                ProposalOp::Revise,
                Some(ProposalState::HeldForReview),
                ProposalState::HeldForReview,
                &author,
                None,
                250,
            ),
            text: Some((
                "Fix three typos".to_owned(),
                vec!["https://example.org/b".to_owned()],
            )),
            revision: Some(revision(&p.id, 2, "second text", 250)),
            superseded_by: None,
        })
        .unwrap();
    assert_eq!(revised.summary, "Fix three typos");
    assert_eq!(revised.sources, vec!["https://example.org/b".to_owned()]);
    assert_eq!(store.revisions(&p.id, None).unwrap().len(), 2);
    assert_eq!(
        store.add_decision(&decision(&p.id, 1, &stranger, 251)),
        Err(StoreError::StateChanged),
        "a decision on a revision that is no longer the latest is refused"
    );
    let d2 = DecisionRecord {
        kind: DecisionKind::RequestChanges,
        ..decision(&p.id, 2, &reviewer, 252)
    };
    store.add_decision(&d2).unwrap();
    assert_eq!(store.decisions(&p.id, None).unwrap(), vec![d1, d2.clone()]);

    // --- Metrics read decisions of submitted proposals, per wiki.
    assert_eq!(store.wiki_decisions(&w.id).unwrap().len(), 2);
    assert!(store.wiki_decisions(&other_wiki.id).unwrap().is_empty());

    // --- Needs a revision accepts comments but not decisions; a closed proposal accepts neither.
    mv(
        store,
        &p.id,
        ProposalOp::AskForRevision,
        ProposalState::HeldForReview,
        ProposalState::NeedsRevision,
        &reviewer,
        260,
    );
    assert_eq!(
        store.add_decision(&decision(&p.id, 2, &stranger, 261)),
        Err(StoreError::StateChanged)
    );
    store
        .add_comment(&comment(&p.id, 2, &author, 262), &REVIEWABLE)
        .unwrap();
    let declined = mv(
        store,
        &p.id,
        ProposalOp::Decline,
        ProposalState::NeedsRevision,
        ProposalState::Declined,
        &reviewer,
        270,
    );
    assert_eq!(declined.closed_at, Some(270));
    assert_eq!(
        store.add_comment(
            &comment(&p.id, 2, &author, 271),
            &[ProposalState::HeldForReview]
        ),
        Err(StoreError::StateChanged)
    );
    let log = store.transitions(&p.id, None).unwrap();
    assert_eq!(log.last().unwrap().reason.as_deref(), Some("a reason"));
    assert_eq!(
        log.iter().map(|t| t.at).collect::<Vec<_>>(),
        vec![100, 200, 250, 260, 270],
        "every step is in the log, in order"
    );

    // --- Superseding names a proposal that exists, in the same step.
    let q = submit(store, &open_draft(store, &w.id, &author, 300), 301);
    let r = submit(store, &open_draft(store, &w.id, &author, 302), 303);
    let missing = store.change_proposal(&ProposalChange {
        transition: step(
            &q.id,
            ProposalOp::Supersede,
            Some(ProposalState::HeldForReview),
            ProposalState::Superseded,
            &author,
            None,
            310,
        ),
        text: None,
        revision: None,
        superseded_by: Some(ProposalId::new()),
    });
    assert_eq!(missing, Err(StoreError::NotFound));
    assert_eq!(
        store.proposal(&q.id, None).unwrap().unwrap().state,
        ProposalState::HeldForReview
    );
    let done = store
        .change_proposal(&ProposalChange {
            transition: step(
                &q.id,
                ProposalOp::Supersede,
                Some(ProposalState::HeldForReview),
                ProposalState::Superseded,
                &author,
                None,
                311,
            ),
            text: None,
            revision: None,
            superseded_by: Some(r.id.clone()),
        })
        .unwrap();
    assert_eq!(done.state, ProposalState::Superseded);
    assert_eq!(done.superseded_by, Some(r.id.clone()));
    assert_eq!(done.closed_at, Some(311));

    // --- A draft that was withdrawn was never submitted: still its author's alone.
    let abandoned = open_draft(store, &w.id, &stranger, 320);
    mv(
        store,
        &abandoned.id,
        ProposalOp::Withdraw,
        ProposalState::Draft,
        ProposalState::Withdrawn,
        &stranger,
        321,
    );
    assert_eq!(store.proposal(&abandoned.id, Some(&author)).unwrap(), None);
    assert_eq!(store.proposal(&abandoned.id, None).unwrap(), None);
    assert!(
        store
            .proposal(&abandoned.id, Some(&stranger))
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .draft(&abandoned.id, Some(&stranger))
            .unwrap()
            .is_some(),
        "the author keeps the working copy of what they withdrew"
    );

    // --- Listing: by state, by author, in the order they were opened, for the viewer's eyes.
    let ids =
        |v: Vec<inklkunn_store::ProposalRecord>| v.into_iter().map(|p| p.id).collect::<Vec<_>>();
    assert_eq!(
        ids(store.proposals(&w.id, &all, Some(&author)).unwrap()),
        vec![p.id.clone(), q.id.clone(), r.id.clone()]
    );
    let only_held = ProposalFilter {
        states: Some(vec![ProposalState::HeldForReview]),
        author: None,
    };
    assert_eq!(
        ids(store.proposals(&w.id, &only_held, None).unwrap()),
        vec![r.id.clone()]
    );
    let strangers = ProposalFilter {
        states: None,
        author: Some(stranger.clone()),
    };
    assert!(
        store
            .proposals(&w.id, &strangers, Some(&author))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .proposals(&w.id, &strangers, Some(&stranger))
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .proposals(&other_wiki.id, &all, Some(&author))
            .unwrap()
            .is_empty()
    );

    // --- The counts the quotas read: open means draft, held or needing a revision, drafts included.
    assert_eq!(
        store.open_count(&w.id, &author).unwrap(),
        1,
        "only r is open"
    );
    open_draft(store, &w.id, &author, 330);
    assert_eq!(
        store.open_count(&w.id, &author).unwrap(),
        2,
        "a draft counts"
    );
    assert_eq!(store.open_count(&other_wiki.id, &author).unwrap(), 0);
    assert_eq!(
        store.opened_since(&w.id, Some(&author), 302).unwrap(),
        vec![302, 330]
    );
    assert_eq!(
        store.opened_since(&w.id, Some(&author), 303).unwrap(),
        vec![330]
    );
    assert_eq!(
        store.opened_since(&w.id, None, 300).unwrap(),
        vec![300, 302, 320, 330]
    );
    assert_eq!(
        store.opened_since(&w.id, Some(&stranger), 0).unwrap(),
        vec![320]
    );
}

#[test]
fn the_memory_store_holds_the_proposal_invariants() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_holds_the_proposal_invariants() {
    the_suite(&SqliteStore::open_in_memory().expect("open"));
}

/// Both stores answer the same, step by step (the SQLite store is held to the memory store as reference).
#[test]
fn a_refused_write_changes_nothing_on_either_store() {
    fn run(store: &(impl ProposalStore + WikiStore + AccountStore)) {
        let w = wiki(store, "refusals");
        let (a, r) = (account(store), account(store));
        let p = submit(store, &open_draft(store, &w.id, &a, 1), 2);
        store.add_decision(&decision(&p.id, 1, &r, 3)).unwrap();
        let before = (
            store.proposal(&p.id, None).unwrap(),
            store.revisions(&p.id, None).unwrap(),
            store.decisions(&p.id, None).unwrap(),
            store.transitions(&p.id, None).unwrap(),
        );
        // A second decision by the same reviewer on the same revision.
        let again = DecisionRecord {
            kind: DecisionKind::RequestChanges,
            body: "changed my mind".into(),
            ..decision(&p.id, 1, &r, 4)
        };
        assert_eq!(store.add_decision(&again), Err(StoreError::AlreadyRecorded));
        // A revision with a number already taken.
        let dup = store.change_proposal(&ProposalChange {
            transition: step(
                &p.id,
                ProposalOp::Revise,
                Some(ProposalState::HeldForReview),
                ProposalState::HeldForReview,
                &a,
                None,
                5,
            ),
            text: Some(("rewritten".to_owned(), Vec::new())),
            revision: Some(revision(&p.id, 1, "overwritten", 5)),
            superseded_by: None,
        });
        assert_eq!(dup, Err(StoreError::StateChanged));
        let after = (
            store.proposal(&p.id, None).unwrap(),
            store.revisions(&p.id, None).unwrap(),
            store.decisions(&p.id, None).unwrap(),
            store.transitions(&p.id, None).unwrap(),
        );
        assert_eq!(before, after);
    }
    run(&MemoryStore::new());
    run(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_sqlite_proposal_records_persist_across_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let (w, a, r, p) = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persist");
        let (a, r) = (account(&store), account(&store));
        let held = submit(&store, &open_draft(&store, &w.id, &a, 10), 11);
        store
            .add_comment(&comment(&held.id, 1, &r, 12), &REVIEWABLE)
            .unwrap();
        store.add_decision(&decision(&held.id, 1, &r, 13)).unwrap();
        let draft = open_draft(&store, &w.id, &a, 14);
        store
            .save_draft(&draft.id, "kept", &[], &content(3, "kept text"), 15)
            .unwrap();
        (w.id, a, r, (held, draft))
    };
    let store = SqliteStore::open(&db).expect("reopen");
    let (held, draft) = p;
    assert_eq!(
        store.proposal(&held.id, None).unwrap().unwrap().state,
        ProposalState::HeldForReview
    );
    assert_eq!(store.revisions(&held.id, None).unwrap().len(), 1);
    assert_eq!(store.comments(&held.id, None).unwrap().len(), 1);
    assert_eq!(
        nth(&store.decisions(&held.id, None).unwrap(), 0).reviewer,
        r
    );
    assert_eq!(store.transitions(&held.id, None).unwrap().len(), 2);
    // The draft is still the author's, with what was saved; and still nobody else's.
    assert_eq!(
        store.draft(&draft.id, Some(&a)).unwrap(),
        Some(content(3, "kept text"))
    );
    assert_eq!(store.proposal(&draft.id, Some(&r)).unwrap(), None);
    assert_eq!(
        store
            .proposal(&draft.id, Some(&a))
            .unwrap()
            .unwrap()
            .summary,
        "kept"
    );
    assert_eq!(
        store
            .proposals(&w, &ProposalFilter::default(), None)
            .unwrap()
            .len(),
        1
    );
}

/// The database refuses the statements that would change what is written once, from a connection that never
/// went through this crate: an `UPDATE`, a `DELETE`, and a replacing `INSERT` on **each** unique key (the
/// replacing insert removes the row it collides with without firing a delete trigger).
#[test]
fn the_write_once_proposal_tables_cannot_be_edited_or_erased_through_sql() {
    let tmp = temp("append-only");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "sql");
    let (a, r) = (account(&store), account(&store));
    let p = submit(&store, &open_draft(&store, &w.id, &a, 1), 2);
    let c = comment(&p.id, 1, &r, 3);
    store.add_comment(&c, &REVIEWABLE).unwrap();
    let d = decision(&p.id, 1, &r, 4);
    store.add_decision(&d).unwrap();

    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let new_id = DecisionId::new();
    let new_comment = CommentId::new();
    let statements: Vec<String> = vec![
        "UPDATE proposal_revisions SET content = '{}'".into(),
        "DELETE FROM proposal_revisions".into(),
        format!("INSERT OR REPLACE INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES ('{}', 1, '{{}}', 9)", p.id),
        "UPDATE proposal_transitions SET reason = 'edited'".into(),
        "DELETE FROM proposal_transitions".into(),
        "REPLACE INTO proposal_transitions (seq, proposal_id, op, from_state, to_state, actor, reason, at) VALUES (1, 'x', 'create', NULL, 'draft', 'y', NULL, 1)".into(),
        "UPDATE proposal_comments SET body = 'edited'".into(),
        "DELETE FROM proposal_comments".into(),
        format!("INSERT OR REPLACE INTO proposal_comments (id, seq, proposal_id, revision, author, body, created_at) VALUES ('{}', 99, '{}', 1, '{}', 'rewritten', 9)", c.id, p.id, r),
        "UPDATE proposal_decisions SET kind = 'request-changes'".into(),
        "DELETE FROM proposal_decisions".into(),
        // Replacing on the id.
        format!("INSERT OR REPLACE INTO proposal_decisions (id, seq, proposal_id, revision, reviewer, kind, body, created_at) VALUES ('{}', 99, '{}', 1, '{}', 'request-changes', 'rewritten', 9)", d.id, p.id, a),
        // Replacing on the other unique key: a different id, the same reviewer and revision.
        format!("INSERT OR REPLACE INTO proposal_decisions (id, seq, proposal_id, revision, reviewer, kind, body, created_at) VALUES ('{new_id}', 99, '{}', 1, '{}', 'request-changes', 'rewritten', 9)", p.id, r),
        format!("INSERT INTO proposal_decisions (id, seq, proposal_id, revision, reviewer, kind, body, created_at) VALUES ('{new_id}', 99, '{}', 1, '{}', 'approve', 'again', 9)", p.id, r),
        format!("INSERT INTO proposal_comments (id, seq, proposal_id, revision, author, body, created_at) VALUES ('{}', 99, '{}', 1, '{}', 'twice', 9)", c.id, p.id, r),
    ];
    for sql in &statements {
        let e = conn.execute(sql, []).expect_err(sql).to_string();
        assert!(e.contains("append-only"), "{sql}: {e}");
    }
    let _ = new_comment;
    // Nothing moved.
    assert_eq!(store.decisions(&p.id, None).unwrap(), vec![d]);
    assert_eq!(store.comments(&p.id, None).unwrap(), vec![c]);
    assert_eq!(store.revisions(&p.id, None).unwrap().len(), 1);
    assert_eq!(store.transitions(&p.id, None).unwrap().len(), 2);
    // The proposal row itself is the one thing that changes, and it still can.
    conn.execute("UPDATE proposals SET updated_at = 77", [])
        .expect("the row is mutable");
}

/// A database exactly as build 8 left it, with a wiki, an account and a role grant in it.
fn make_version_8(db: &std::path::Path, wiki: &str, account: &str) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 8) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '8')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, 'Somebody', 1)",
        [account],
    )
    .expect("an account");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'Old', 'heads/main', 'CC-BY-SA-4.0', \
         '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [wiki],
    )
    .expect("a wiki");
    raw.execute(
        "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
         VALUES (?1, ?2, 'contributor', NULL, 5, 1)",
        [wiki, account],
    )
    .expect("a grant");
}

fn table_names(db: &std::path::Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).expect("raw open");
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger', 'index') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_fts%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

#[test]
fn a_version_8_database_is_migrated_to_10_keeping_its_records_and_only_once() {
    let tmp = temp("v8");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000d1";
    let account_id = "019a0000-0000-7000-8000-0000000000d2";
    make_version_8(&db, wiki_id, account_id);
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "8");
    assert!(
        !table_names(&db).iter().any(|n| n == "proposals"),
        "the M8 database has no proposal tables"
    );
    {
        let store = SqliteStore::open(&db).expect("a version 8 database opens");
        assert_eq!(version(), "10");
        assert_eq!(SCHEMA_VERSION, 10);
        // What was there is there.
        let wikis = store.list().unwrap();
        assert_eq!(wikis.len(), 1);
        let old = wikis.first().unwrap();
        assert_eq!(old.slug.as_str(), "old");
        let acct = inklkunn_store::AccountId::parse(account_id).unwrap();
        assert_eq!(
            inklkunn_store::RoleStore::role(&store, &acct, &old.id).unwrap(),
            Some(inklkunn_store::Role::Contributor)
        );
        // The new tables work on the old wiki and the old account.
        let held = submit(&store, &open_draft(&store, &old.id, &acct, 5), 6);
        assert_eq!(held.state, ProposalState::HeldForReview);
        store
            .add_comment(&comment(&held.id, 1, &acct, 7), &REVIEWABLE)
            .unwrap();
        store
            .add_decision(&decision(&held.id, 1, &acct, 8))
            .unwrap();
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(version(), "10");
    let old = store.list().unwrap().remove(0);
    assert_eq!(
        store
            .proposals(&old.id, &ProposalFilter::default(), None)
            .unwrap()
            .len(),
        1
    );
    // The migrated schema is the fresh schema: the same tables, indexes and triggers.
    let fresh = tmp.0.join("fresh.sqlite");
    drop(SqliteStore::open(&fresh).expect("fresh"));
    assert_eq!(table_names(&db), table_names(&fresh));
    // And the database refuses to be opened by a build that knows less, which this build checks by not
    // accepting a higher version than its own.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE schema_meta SET value = '11' WHERE key = 'schema_version'",
        [],
    )
    .unwrap();
    assert!(matches!(
        SqliteStore::open(&db),
        Err(StoreError::SchemaVersion { .. })
    ));
}

#[test]
fn the_proposal_tables_refer_to_wikis_and_accounts_that_exist() {
    let tmp = temp("foreign-keys");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let w = wiki(&store, "refs");
    let a = account(&store);
    let p = open_draft(&store, &w.id, &a, 1);
    // The store refuses by name ...
    let phantom = inklkunn_store::AccountId::new();
    let bad = support::draft_record(&w.id, &phantom, 1);
    let r = store.create_proposal(
        &bad,
        &content(1, "x"),
        &step(
            &bad.id,
            ProposalOp::Create,
            None,
            ProposalState::Draft,
            &a,
            None,
            1,
        ),
    );
    assert_eq!(r, Err(StoreError::AccountNotFound));
    // ... and the foreign keys are the second wall for a connection that bypasses it.
    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for sql in [
        format!(
            "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, created_at, updated_at) VALUES ('{}', 90, '{}', 'heads/main', 'draft', '{phantom}', 's', '[]', 1, 1)",
            ProposalId::new(),
            w.id
        ),
        format!(
            "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, created_at, updated_at) VALUES ('{}', 90, '{}', 'heads/main', 'draft', '{a}', 's', '[]', 1, 1)",
            ProposalId::new(),
            inklkunn_store::WikiId::new()
        ),
        format!(
            "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, created_at, updated_at) VALUES ('{}', 90, '{}', 'heads/main', 'ended', '{a}', 's', '[]', 1, 1)",
            ProposalId::new(),
            w.id
        ),
        format!(
            "INSERT INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES ('{}', 1, '{{}}', 1)",
            ProposalId::new()
        ),
        format!(
            "INSERT INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES ('{}', 0, '{{}}', 1)",
            p.id
        ),
    ] {
        assert!(conn.execute(&sql, []).is_err(), "{sql}");
    }
}
