//! The review record of RFC 005 part 1 D-1 (d) on both stores (handoff 005 K3 b; store schema 16): per accepted change
//! and per claim it touches, *kept* or *dropped*, later *re-confirmed by* the curation proposal that judged it again;
//! the claims a *request changes* decision's revision touched; and the note, made with a curation proposal, of the change
//! it re-confirms. Written with the act and once, read only from acceptances that sealed. The suite is written against
//! the traits; what only a database can refuse (a raw connection, the migration from 15) is below it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::review::MAX_CLAIMS;
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore,
    AccountId, AccountStore, ClaimHow, ClaimOutcome, ClaimReview, DecisionId, DecisionKind,
    DecisionRecord, MemoryStore, ProposalId, ProposalOp, ProposalRecord, ProposalState,
    ProposalStore, Role, SqliteStore, StoreError, WikiId, WikiStore,
};
use support::{account, content, draft_record, open_draft, step, submit, temp, wiki};

const C1: &str = "019a0000-0000-7000-8000-000000000c01";
const C2: &str = "019a0000-0000-7000-8000-000000000c02";
const C3: &str = "019a0000-0000-7000-8000-000000000c03";

fn out(claim: &str, how: ClaimHow) -> ClaimOutcome {
    ClaimOutcome {
        claim: claim.to_owned(),
        how,
    }
}

fn acceptance(p: &ProposalRecord, acceptor: &AccountId, at: i64) -> AcceptanceRecord {
    AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: p.id.clone(),
        revision: 1,
        wiki: p.wiki.clone(),
        view: p.view.clone(),
        acceptor: acceptor.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "default-1".to_owned(),
        proposer: p.author.clone(),
        digest: "d".repeat(64),
        bytes: 10,
        created_at: at,
    }
}

fn seal(store: &impl AcceptanceStore, a: &AcceptanceRecord, at: i64) {
    store
        .finish_acceptance(&AcceptanceResult {
            acceptance: a.id.clone(),
            outcome: AcceptanceOutcome::Sealed {
                block_id: "b".repeat(64),
                patch_id: "c".repeat(64),
            },
            at,
        })
        .unwrap();
}

fn fail(store: &impl AcceptanceStore, a: &AcceptanceRecord, at: i64) {
    store
        .finish_acceptance(&AcceptanceResult {
            acceptance: a.id.clone(),
            outcome: AcceptanceOutcome::Failed {
                word: "wiki-busy".to_owned(),
            },
            at,
        })
        .unwrap();
}

fn pending(
    store: &impl AcceptanceStore,
    p: &ProposalRecord,
    acceptor: &AccountId,
    at: i64,
) -> AcceptanceRecord {
    let a = acceptance(p, acceptor, at);
    store.begin_acceptance(&a).unwrap();
    a
}

fn decision(
    p: &ProposalRecord,
    reviewer: &AccountId,
    kind: DecisionKind,
    at: i64,
) -> DecisionRecord {
    DecisionRecord {
        id: DecisionId::new(),
        proposal: p.id.clone(),
        revision: 1,
        reviewer: reviewer.clone(),
        kind,
        body: String::new(),
        created_at: at,
    }
}

fn review(
    a: &AcceptanceRecord,
    claim: &str,
    how: ClaimHow,
    by: Option<&AcceptanceRecord>,
) -> ClaimReview {
    ClaimReview {
        acceptance: a.id.clone(),
        proposal: a.proposal.clone(),
        claim: claim.to_owned(),
        how,
        reconfirmed_by: by.map(|b| b.id.clone()),
    }
}

/// A reconfirm proposal's records, as the service makes them: a draft, with the note of the change.
fn reconfirm_draft(
    store: &impl ProposalStore,
    wiki: &WikiId,
    author: &AccountId,
    change: &AcceptanceId,
    at: i64,
) -> Result<ProposalRecord, StoreError> {
    let record = draft_record(wiki, author, at);
    let created = step(
        &record.id,
        ProposalOp::Create,
        None,
        ProposalState::Draft,
        author,
        None,
        at,
    );
    store.create_reconfirm_proposal(&record, &content(1, "draft text"), &created, change)?;
    Ok(record)
}

fn the_suite(store: &(impl ProposalStore + AcceptanceStore + WikiStore + AccountStore)) {
    let w = wiki(store, "review");
    let other = wiki(store, "elsewhere");
    let (author, maintainer, reviewer) = (account(store), account(store), account(store));
    let p1 = submit(store, &open_draft(store, &w.id, &author, 10), 11);

    // --- A change's records are written while its acceptance is pending, and read once it has sealed.
    let a1 = pending(store, &p1, &maintainer, 20);
    store
        .record_claim_reviews(
            &a1.id,
            &[out(C1, ClaimHow::Kept), out(C2, ClaimHow::Dropped)],
        )
        .unwrap();
    assert!(store.claim_reviews(&w.id).unwrap().is_empty(), "pending");
    assert!(store.claim_reviews_of(&a1.id).unwrap().is_empty());
    seal(store, &a1, 21);
    let first = vec![
        review(&a1, C1, ClaimHow::Kept, None),
        review(&a1, C2, ClaimHow::Dropped, None),
    ];
    assert_eq!(store.claim_reviews(&w.id).unwrap(), first);
    assert_eq!(store.claim_reviews_of(&a1.id).unwrap(), first);
    assert!(store.claim_reviews(&other.id).unwrap().is_empty());
    assert_eq!(
        store.claim_reviews_of(&AcceptanceId::new()).unwrap(),
        Vec::new()
    );

    // --- Once the acceptance has ended nothing is added to it.
    assert_eq!(
        store.record_claim_reviews(&a1.id, &[out(C3, ClaimHow::Kept)]),
        Err(StoreError::StateChanged)
    );
    assert_eq!(store.claim_reviews(&w.id).unwrap(), first);

    // --- What is refused writes nothing: all of the claims or none.
    let p2 = submit(store, &open_draft(store, &w.id, &author, 30), 31);
    let a2 = pending(store, &p2, &maintainer, 32);
    assert_eq!(
        store.record_claim_reviews(&AcceptanceId::new(), &[out(C1, ClaimHow::Kept)]),
        Err(StoreError::NotFound)
    );
    assert_eq!(
        store.record_claim_reviews(
            &a2.id,
            &[out(C3, ClaimHow::Kept), out(C3, ClaimHow::Dropped)]
        ),
        Err(StoreError::AlreadyRecorded),
        "a claim twice in one record"
    );
    for bad in ["", "not-a-claim", &C1.to_uppercase(), &format!("{C1}0")] {
        assert!(
            matches!(
                store.record_claim_reviews(
                    &a2.id,
                    &[out(C3, ClaimHow::Kept), out(bad, ClaimHow::Kept)]
                ),
                Err(StoreError::Backend(_))
            ),
            "{bad:?} is not a claim's id"
        );
    }
    let many: Vec<ClaimOutcome> = (0..=MAX_CLAIMS)
        .map(|n| out(&format!("019a0000-0000-7000-8000-{n:012x}"), ClaimHow::Kept))
        .collect();
    assert!(matches!(
        store.record_claim_reviews(&a2.id, &many),
        Err(StoreError::Backend(_))
    ));
    store
        .record_claim_reviews(&a2.id, &[out(C1, ClaimHow::Kept)])
        .unwrap();
    assert_eq!(
        store.record_claim_reviews(
            &a2.id,
            &[out(C3, ClaimHow::Kept), out(C1, ClaimHow::Dropped)]
        ),
        Err(StoreError::AlreadyRecorded),
        "a claim already recorded for the acceptance"
    );
    // A change that touched nothing writes nothing, and that is no error.
    store.record_claim_reviews(&a2.id, &[]).unwrap();

    // --- An acceptance that failed is never read, and its retry writes its own.
    fail(store, &a2, 33);
    assert_eq!(store.claim_reviews(&w.id).unwrap(), first);
    let a2b = pending(store, &p2, &maintainer, 34);
    store
        .record_claim_reviews(&a2b.id, &[out(C1, ClaimHow::Kept), out(C3, ClaimHow::Kept)])
        .unwrap();
    seal(store, &a2b, 35);
    let mut all = first.clone();
    all.push(review(&a2b, C1, ClaimHow::Kept, None));
    all.push(review(&a2b, C3, ClaimHow::Kept, None));
    assert_eq!(
        store.claim_reviews(&w.id).unwrap(),
        all,
        "oldest change first"
    );

    // --- A curation acceptance re-confirms a kept claim of a sealed change of the same wiki, while it is pending.
    let pc = submit(store, &open_draft(store, &w.id, &reviewer, 40), 41);
    let ac = pending(store, &pc, &maintainer, 42);
    assert_eq!(
        store.record_reconfirmations(&ac.id, &a1.id, &[C2.to_owned()]),
        Err(StoreError::NotFound),
        "a dropped claim has nothing to judge again"
    );
    assert_eq!(
        store.record_reconfirmations(&ac.id, &a1.id, &[C3.to_owned()]),
        Err(StoreError::NotFound),
        "a claim the change did not touch"
    );
    assert_eq!(
        store.record_reconfirmations(&ac.id, &a2.id, &[C1.to_owned()]),
        Err(StoreError::NotFound),
        "a change that did not seal"
    );
    assert_eq!(
        store.record_reconfirmations(&AcceptanceId::new(), &a1.id, &[C1.to_owned()]),
        Err(StoreError::NotFound)
    );
    assert!(matches!(
        store.record_reconfirmations(&ac.id, &a1.id, &["x".to_owned()]),
        Err(StoreError::Backend(_))
    ));
    assert!(matches!(
        store.record_reconfirmations(&a1.id, &a1.id, &[C1.to_owned()]),
        Err(StoreError::Backend(_))
    ));
    assert_eq!(
        store.record_reconfirmations(&a1.id, &a2b.id, &[C1.to_owned()]),
        Err(StoreError::StateChanged),
        "an acceptance that has ended re-confirms nothing"
    );
    // A change of another wiki is not this one's to re-confirm.
    let po = submit(store, &open_draft(store, &other.id, &author, 43), 44);
    let ao = pending(store, &po, &maintainer, 45);
    store
        .record_claim_reviews(&ao.id, &[out(C1, ClaimHow::Kept)])
        .unwrap();
    seal(store, &ao, 46);
    assert_eq!(
        store.record_reconfirmations(&ac.id, &ao.id, &[C1.to_owned()]),
        Err(StoreError::NotFound)
    );
    assert_eq!(
        store.claim_reviews(&w.id).unwrap(),
        all,
        "nothing was written"
    );

    store
        .record_reconfirmations(&ac.id, &a1.id, &[C1.to_owned()])
        .unwrap();
    assert_eq!(
        store.record_reconfirmations(&ac.id, &a1.id, &[C1.to_owned()]),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(
        store.claim_reviews_of(&a1.id).unwrap(),
        first,
        "not before the curation acceptance seals"
    );
    seal(store, &ac, 47);
    let again = vec![
        review(&a1, C1, ClaimHow::Kept, Some(&ac)),
        review(&a1, C2, ClaimHow::Dropped, None),
    ];
    assert_eq!(store.claim_reviews_of(&a1.id).unwrap(), again);
    // The same claim of another change is not re-confirmed by it, and a second re-confirmation does not replace the first.
    assert_eq!(
        store.claim_reviews_of(&a2b.id).unwrap(),
        vec![
            review(&a2b, C1, ClaimHow::Kept, None),
            review(&a2b, C3, ClaimHow::Kept, None)
        ]
    );
    let pd = submit(store, &open_draft(store, &w.id, &reviewer, 50), 51);
    let ad = pending(store, &pd, &maintainer, 52);
    store
        .record_reconfirmations(&ad.id, &a1.id, &[C1.to_owned()])
        .unwrap();
    seal(store, &ad, 53);
    assert_eq!(
        store.claim_reviews_of(&a1.id).unwrap(),
        again,
        "the first stays"
    );
    // One that failed re-confirmed nothing.
    let pe = submit(store, &open_draft(store, &w.id, &reviewer, 54), 55);
    let ae = pending(store, &pe, &maintainer, 56);
    store
        .record_reconfirmations(&ae.id, &a2b.id, &[C3.to_owned()])
        .unwrap();
    fail(store, &ae, 57);
    assert_eq!(
        store
            .claim_reviews_of(&a2b.id)
            .unwrap()
            .last()
            .unwrap()
            .reconfirmed_by,
        None
    );

    // --- A request for changes records the claims its revision touches, with the decision, or nothing.
    let held = submit(store, &open_draft(store, &w.id, &author, 60), 61);
    let bad = decision(&held, &reviewer, DecisionKind::Approve, 62);
    assert!(matches!(
        store.add_decision_touching(&bad, &[C1.to_owned()]),
        Err(StoreError::Backend(_))
    ));
    let asks = decision(&held, &reviewer, DecisionKind::RequestChanges, 62);
    assert!(matches!(
        store.add_decision_touching(&asks, &[C1.to_owned(), "x".to_owned()]),
        Err(StoreError::Backend(_))
    ));
    assert_eq!(
        store.add_decision_touching(&asks, &[C1.to_owned(), C1.to_owned()]),
        Err(StoreError::AlreadyRecorded)
    );
    assert!(
        store.decisions(&held.id, Some(&author)).unwrap().is_empty(),
        "no decision without its claims"
    );
    assert!(store.touching_decisions(&w.id).unwrap().is_empty());
    store
        .add_decision_touching(&asks, &[C1.to_owned(), C2.to_owned()])
        .unwrap();
    let plain = decision(&held, &maintainer, DecisionKind::RequestChanges, 63);
    store.add_decision(&plain).unwrap();
    let approves = decision(&held, &author, DecisionKind::Approve, 64);
    store.add_decision_touching(&approves, &[]).unwrap();
    assert_eq!(
        store.touching_decisions(&w.id).unwrap(),
        vec![asks.id.clone()],
        "only the request that recorded a claim"
    );
    assert!(store.touching_decisions(&other.id).unwrap().is_empty());
    assert_eq!(
        store.add_decision_touching(&asks, &[C1.to_owned()]),
        Err(StoreError::AlreadyRecorded)
    );

    // --- A curation proposal made by the reconfirm step names the change it re-confirms, from its creation or never.
    let origin = reconfirm_draft(store, &w.id, &reviewer, &a1.id, 70).unwrap();
    assert_eq!(
        store.reconfirm_of(&origin.id, Some(&reviewer)).unwrap(),
        Some(a1.id.clone())
    );
    assert_eq!(
        store.reconfirm_of(&origin.id, None).unwrap(),
        None,
        "a draft is its author's"
    );
    assert_eq!(
        store.reconfirmations_of(&a1.id, Some(&reviewer)).unwrap(),
        vec![origin.id.clone()]
    );
    assert!(store.reconfirmations_of(&a1.id, None).unwrap().is_empty());
    let sent = submit(store, &origin, 71);
    assert_eq!(
        store.reconfirm_of(&sent.id, None).unwrap(),
        Some(a1.id.clone())
    );
    assert_eq!(
        store.reconfirmations_of(&a1.id, None).unwrap(),
        vec![origin.id.clone()]
    );
    assert_eq!(
        store.reconfirm_of(&p1.id, None).unwrap(),
        None,
        "an ordinary proposal"
    );
    assert_eq!(store.reconfirm_of(&ProposalId::new(), None).unwrap(), None);
    assert!(store.reconfirmations_of(&a2b.id, None).unwrap().is_empty());
    // A change that did not seal, one of another wiki, or none at all, is no change to re-confirm; nothing is made.
    for change in [&a2.id, &ao.id, &AcceptanceId::new()] {
        let before = store
            .proposals(&w.id, &Default::default(), None)
            .unwrap()
            .len();
        assert_eq!(
            reconfirm_draft(store, &w.id, &reviewer, change, 72).map(|_| ()),
            Err(StoreError::NotFound)
        );
        assert_eq!(
            store
                .proposals(&w.id, &Default::default(), None)
                .unwrap()
                .len(),
            before,
            "the proposal is not made without its note"
        );
    }
}

#[test]
fn the_memory_store_keeps_the_review_record() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_the_same_review_record() {
    the_suite(&SqliteStore::open_in_memory().unwrap());
}

// ------------------------------------------------------------------------------------ the database itself

struct Rig {
    _tmp: support::TempDir,
    raw: rusqlite::Connection,
    a1: AcceptanceRecord,
    open: AcceptanceRecord,
    decision: DecisionRecord,
    approve: DecisionRecord,
    proposal: ProposalRecord,
}

/// A database file with a sealed change that kept `C1` and dropped `C2`, an acceptance still pending, a request for
/// changes with a claim and an approval; and a raw connection to it with foreign keys on.
fn rig(tag: &str) -> Rig {
    let tmp = temp(tag);
    let path = tmp.0.join("store.sqlite");
    let (a1, open, d, approve, proposal) = {
        let store = SqliteStore::open(&path).unwrap();
        let w = wiki(&store, "review");
        let (author, maintainer, reviewer) = (account(&store), account(&store), account(&store));
        let p1 = submit(&store, &open_draft(&store, &w.id, &author, 10), 11);
        let a1 = pending(&store, &p1, &maintainer, 20);
        store
            .record_claim_reviews(
                &a1.id,
                &[out(C1, ClaimHow::Kept), out(C2, ClaimHow::Dropped)],
            )
            .unwrap();
        seal(&store, &a1, 21);
        let p2 = submit(&store, &open_draft(&store, &w.id, &author, 30), 31);
        let open = pending(&store, &p2, &maintainer, 32);
        let d = decision(&p2, &reviewer, DecisionKind::RequestChanges, 33);
        store.add_decision_touching(&d, &[C1.to_owned()]).unwrap();
        let approve = decision(&p2, &maintainer, DecisionKind::Approve, 34);
        store.add_decision(&approve).unwrap();
        (a1, open, d, approve, p2)
    };
    let raw = rusqlite::Connection::open(&path).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    Rig {
        _tmp: tmp,
        raw,
        a1,
        open,
        decision: d,
        approve,
        proposal,
    }
}

fn message(r: rusqlite::Result<usize>) -> String {
    r.expect_err("the database refuses it").to_string()
}

#[test]
fn the_database_refuses_what_the_store_would_not_write() {
    let r = rig("raw");
    let (a1, open) = (r.a1.id.as_str(), r.open.id.as_str());
    let claim_row = |acceptance: &str, claim: &str, how: &str| {
        r.raw.execute(
            "INSERT INTO change_claims (acceptance_id, claim, how) VALUES (?1, ?2, ?3)",
            rusqlite::params![acceptance, claim, how],
        )
    };

    // A change's record: a claim's id, kept or dropped, once, an acceptance that is there and has not ended.
    assert!(message(claim_row(open, "not-a-claim", "kept")).contains("CHECK constraint"));
    assert!(message(claim_row(open, C3, "unsure")).contains("CHECK constraint"));
    assert!(
        message(claim_row(&AcceptanceId::new().to_string(), C3, "kept")).contains("FOREIGN KEY")
    );
    assert!(message(claim_row(a1, C3, "kept")).contains("the acceptance has ended"));
    assert!(message(claim_row(a1, C1, "kept")).contains("append-only"));
    claim_row(open, C3, "kept").expect("a pending acceptance takes a record");
    // A re-confirmation: of a kept claim, of a sealed change, by an acceptance that has not ended, not by itself.
    let reconfirm = |change: &str, claim: &str, by: &str| {
        r.raw.execute(
            "INSERT INTO claim_reconfirmations (change_id, claim, by_id) VALUES (?1, ?2, ?3)",
            rusqlite::params![change, claim, by],
        )
    };
    assert!(message(reconfirm(a1, C2, open)).contains("only a kept claim"));
    assert!(
        message(reconfirm(open, C3, a1)).contains("append-only"),
        "a change that did not seal"
    );
    reconfirm(a1, C1, open)
        .expect("a pending acceptance re-confirms a kept claim of a sealed change");
    assert!(message(reconfirm(a1, C1, open)).contains("append-only"));

    // A decision's claims belong to a request for changes.
    let decision_claim = |decision: &str, claim: &str| {
        r.raw.execute(
            "INSERT INTO decision_claims (decision_id, claim) VALUES (?1, ?2)",
            rusqlite::params![decision, claim],
        )
    };
    assert!(
        message(decision_claim(r.approve.id.as_str(), C1)).contains("only a request for changes")
    );
    assert!(message(decision_claim(r.decision.id.as_str(), "x")).contains("CHECK constraint"));
    assert!(message(decision_claim(r.decision.id.as_str(), C1)).contains("append-only"));
    decision_claim(r.decision.id.as_str(), C2).expect("a second claim of the request");

    // A proposal's note of the change it re-confirms is one per proposal, of an acceptance that is there.
    let note = |proposal: &str, change: &str| {
        r.raw.execute(
            "INSERT INTO proposal_reconfirms (proposal_id, change_id) VALUES (?1, ?2)",
            rusqlite::params![proposal, change],
        )
    };
    assert!(
        message(note(
            r.proposal.id.as_str(),
            &AcceptanceId::new().to_string()
        ))
        .contains("FOREIGN KEY")
    );
    note(r.proposal.id.as_str(), a1).expect("a note");
    assert!(message(note(r.proposal.id.as_str(), a1)).contains("append-only"));

    // Once there is a row in each, none is changed or removed.
    for sql in [
        "UPDATE change_claims SET how = 'dropped'",
        "DELETE FROM change_claims",
        "UPDATE claim_reconfirmations SET by_id = by_id",
        "DELETE FROM proposal_reconfirms",
        "UPDATE decision_claims SET claim = claim",
        "DELETE FROM decision_claims",
    ] {
        let refused = r.raw.execute(sql, []);
        let refused = refused.expect_err(sql).to_string();
        assert!(refused.contains("append-only"), "{sql}: {refused}");
    }
}

#[test]
fn a_version_15_database_is_migrated_and_the_review_record_works_on_what_it_held() {
    use inklkunn_store::sqlite::MIGRATIONS;
    let tmp = temp("v15");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f2";
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 15) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '15')",
            [],
        )
        .expect("version");
        raw.execute(
            "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, \
             visibility, policy_bootstrap, prikk_format_version, verification_state, verification_at, \
             author_key_id, maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'old', 'heads/main', \
             'CC-BY-SA-4.0', '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, \
             1700000000, 1)",
            [wiki_id],
        )
        .expect("a wiki");
    }
    let w = WikiId::parse(wiki_id).unwrap();
    {
        let store = SqliteStore::open(&db).expect("a version 15 database opens");
        assert!(
            MIGRATIONS.iter().any(|(v, _)| *v == 16),
            "the view-act tables came with version 16"
        );
        assert!(store.claim_reviews(&w).unwrap().is_empty());
        let (author, maintainer) = (account(&store), account(&store));
        let p = submit(&store, &open_draft(&store, &w, &author, 10), 11);
        let a = pending(&store, &p, &maintainer, 12);
        store
            .record_claim_reviews(&a.id, &[out(C1, ClaimHow::Kept)])
            .unwrap();
        seal(&store, &a, 13);
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    let found = store.claim_reviews(&w).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found.first().map(|r| r.how), Some(ClaimHow::Kept));
}
