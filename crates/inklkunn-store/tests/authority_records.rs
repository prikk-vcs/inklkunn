//! What a decision records of the authority it was made under (RFC 006 D-5; acceptance criterion 12; G-T1 part C): a
//! reviewer's decision names the rung the reviewer held and the block of the rules; an acceptance names the block and whether
//! it used the hatch. Both stores keep them the same way, the SQLite store refuses a block that is not one, and a row written
//! by 0.3.0 (before there was anything to record) is read back as one that recorded none.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::MIGRATIONS;
use inklkunn_store::{
    AcceptanceBegin, AcceptanceId, AcceptanceRecord, AcceptanceStore, AccountId, AccountStore,
    DecisionId, DecisionKind, DecisionRecord, MemoryStore, ProposalId, ProposalStore, Role,
    SqliteStore, StoreError, WikiStore,
};
use support::{account, open_draft, submit, temp, wiki};

const BLOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn acceptance(
    p: &inklkunn_store::ProposalRecord,
    by: &AccountId,
    block: Option<&str>,
    hatch: bool,
) -> AcceptanceRecord {
    AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: p.id.clone(),
        revision: 1,
        wiki: p.wiki.clone(),
        view: p.view.clone(),
        acceptor: by.clone(),
        acceptor_role: Role::Admin,
        policy_version: "governance-1".to_owned(),
        policy_block: block.map(str::to_owned),
        hatch,
        proposer: p.author.clone(),
        digest: "d".repeat(64),
        bytes: 10,
        created_at: 20,
    }
}

fn decision(
    p: &inklkunn_store::ProposalRecord,
    by: &AccountId,
    rung: Option<Role>,
    block: Option<&str>,
) -> DecisionRecord {
    DecisionRecord {
        id: DecisionId::new(),
        proposal: p.id.clone(),
        revision: 1,
        reviewer: by.clone(),
        kind: DecisionKind::Approve,
        body: String::new(),
        reviewer_rung: rung,
        policy_block: block.map(str::to_owned),
        created_at: 15,
    }
}

fn the_suite(store: &(impl ProposalStore + AcceptanceStore + WikiStore + AccountStore)) {
    let w = wiki(store, "authority");
    let (author, reviewer, admin) = (account(store), account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &author, 10), 11);

    // A decision names the rung and the block it was made under, or says it recorded none.
    let named = decision(&p, &reviewer, Some(Role::Reviewer), Some(BLOCK));
    store.add_decision(&named).unwrap();
    let defaults = decision(&p, &admin, Some(Role::Admin), None);
    store.add_decision(&defaults).unwrap();
    let back = store.decisions(&p.id, Some(&author)).unwrap();
    assert_eq!(back, vec![named, defaults.clone()]);
    assert_eq!(back.get(1).map(|d| d.policy_block.clone()), Some(None));

    // An acceptance names the block it was decided under and whether it used the hatch.
    let used = acceptance(&p, &admin, Some(BLOCK), true);
    assert_eq!(
        store.begin_acceptance(&used).unwrap(),
        AcceptanceBegin::Started
    );
    let found = store.acceptances_of(&p.id, 1).unwrap();
    assert_eq!(found.first().map(|e| &e.acceptance), Some(&used));
    let pending = store.pending_acceptances(&w.id).unwrap();
    assert_eq!(pending, vec![used]);
}

#[test]
fn the_memory_store_keeps_the_rung_the_block_and_the_hatch() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_the_same_and_across_a_reopen() {
    let tmp = temp("authority");
    let db = tmp.0.join("store.sqlite");
    let (pid, wiki_id, author) = {
        let store = SqliteStore::open(&db).unwrap();
        the_suite(&store);
        let w = store.list().unwrap().pop().unwrap();
        let p = store
            .proposals(&w.id, &Default::default(), None)
            .unwrap()
            .pop()
            .unwrap();
        (p.id.clone(), w.id, p.author)
    };
    let store = SqliteStore::open(&db).unwrap();
    let decisions = store.decisions(&pid, Some(&author)).unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(
        decisions
            .first()
            .map(|d| (d.reviewer_rung, d.policy_block.as_deref())),
        Some((Some(Role::Reviewer), Some(BLOCK)))
    );
    let pending = store.pending_acceptances(&wiki_id).unwrap();
    assert_eq!(
        pending
            .first()
            .map(|a| (a.policy_block.as_deref(), a.hatch)),
        Some((Some(BLOCK), true))
    );
}

#[test]
fn both_stores_refuse_a_block_that_is_not_one_on_a_decision_and_on_an_acceptance() {
    refuses(&SqliteStore::open_in_memory().unwrap());
    refuses(&MemoryStore::new());
}

fn refuses(store: &(impl ProposalStore + AcceptanceStore + WikiStore + AccountStore)) {
    let w = wiki(store, "refuses");
    let (author, reviewer) = (account(store), account(store));
    let p = submit(store, &open_draft(store, &w.id, &author, 10), 11);
    for bad in ["", "NOT HEX", "ABCDEF", "0123 4567"] {
        let e = store
            .add_decision(&decision(&p, &reviewer, Some(Role::Reviewer), Some(bad)))
            .unwrap_err();
        assert!(matches!(e, StoreError::Backend(_)), "{bad:?}: {e:?}");
        let e = store
            .begin_acceptance(&acceptance(&p, &reviewer, Some(bad), false))
            .unwrap_err();
        assert!(matches!(e, StoreError::Backend(_)), "{bad:?}: {e:?}");
    }
    // Nothing of the refused records is there.
    assert!(store.decisions(&p.id, Some(&author)).unwrap().is_empty());
    assert!(store.acceptances_of(&p.id, 1).unwrap().is_empty());
}

#[test]
fn a_decision_and_an_acceptance_of_0_3_0_are_read_back_as_ones_that_recorded_nothing() {
    // A database at version 18, with an accepted proposal, its decision and its acceptance written as 0.3.0 wrote them.
    let tmp = temp("legacy");
    let db = tmp.0.join("store.sqlite");
    let id = || AccountId::new().to_string();
    let (wiki_id, author, reviewer, proposal) = (id(), id(), id(), id());
    let raw = rusqlite::Connection::open(&db).unwrap();
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 18) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '18')",
        [],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'old', 'heads/main', 'CC-BY-SA-4.0', '[\"en\"]', \
         'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [&wiki_id],
    )
    .unwrap();
    for (who, name) in [(&author, "Author"), (&reviewer, "Reviewer")] {
        raw.execute(
            "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, ?2, 1700000000)",
            [who.as_str(), name],
        )
        .unwrap();
    }
    raw.execute(
        "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, created_at, updated_at, \
         submitted_at) VALUES (?1, 1, ?2, 'heads/main', 'accepted', ?3, 'A change', '[]', 1, 1, 1)",
        [&proposal, &wiki_id, &author],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES (?1, 1, '{}', 1)",
        [&proposal],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO proposal_decisions (id, seq, proposal_id, revision, reviewer, kind, body, created_at) \
         VALUES (?1, 1, ?2, 1, ?3, 'approve', '', 2)",
        [&id(), &proposal, &reviewer],
    )
    .unwrap();
    raw.execute(
        "INSERT INTO acceptances (id, seq, proposal_id, revision, wiki_id, view, acceptor, acceptor_role, \
         policy_version, proposer, digest, bytes, created_at) \
         VALUES (?1, 1, ?2, 1, ?3, 'heads/main', ?4, 'maintainer', 'bootstrap-1', ?5, ?6, 5, 3)",
        [
            &id(),
            &proposal,
            &wiki_id,
            &reviewer,
            &author,
            &"d".repeat(64),
        ],
    )
    .unwrap();
    drop(raw);
    // The migration adds the columns and writes none of them.
    let store = SqliteStore::open(&db).expect("a version 18 database opens");
    let pid = ProposalId::parse(&proposal).unwrap();
    let decisions = store
        .decisions(&pid, Some(&AccountId::parse(&author).unwrap()))
        .unwrap();
    let only = decisions.first().expect("the decision of 0.3.0");
    assert_eq!(
        (only.reviewer_rung, only.policy_block.clone()),
        (None, None)
    );
    let acceptances = store.acceptances_of(&pid, 1).unwrap();
    let only = &acceptances
        .first()
        .expect("the acceptance of 0.3.0")
        .acceptance;
    assert_eq!(
        (
            only.policy_block.clone(),
            only.hatch,
            only.policy_version.as_str()
        ),
        (None, false, "bootstrap-1")
    );
}
