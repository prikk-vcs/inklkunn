//! A claim's thread on both stores (RFC 005 part 2 D-11; handoff 005 K-T10; store schema 18): the epistemic discussion
//! of a claim, written once and never changed, keyed by the wiki and the claim's id, a reply in the thread of the comment
//! it answers, read oldest first in pages, counted for the comment quota, and listed whole for the bundle. The suite is
//! written against the trait; what only a database can refuse (a raw connection, the migration from 17) is below it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::claim_thread::MAX_PAGE;
use inklkunn_store::{
    AccountId, AccountStore, ClaimCommentRecord, ClaimThreadStore, CommentId, MemoryStore,
    SqliteStore, StoreError, WikiId, WikiStore,
};
use support::{account, temp, wiki};

const C1: &str = "019a0000-0000-7000-8000-000000000c01";
const C2: &str = "019a0000-0000-7000-8000-000000000c02";

fn comment(
    wiki: &WikiId,
    claim: &str,
    author: &AccountId,
    parent: Option<&CommentId>,
    body: &str,
    at: i64,
) -> ClaimCommentRecord {
    ClaimCommentRecord {
        id: CommentId::new(),
        wiki: wiki.clone(),
        claim: claim.to_owned(),
        author: author.clone(),
        parent: parent.cloned(),
        body: body.to_owned(),
        created_at: at,
    }
}

fn bodies(store: &impl ClaimThreadStore, w: &WikiId, claim: &str) -> Vec<String> {
    store
        .claim_comments(w, claim, None, MAX_PAGE)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.comment.body)
        .collect()
}

fn the_suite(store: &(impl ClaimThreadStore + WikiStore + AccountStore)) {
    let w = wiki(store, "threads");
    let other = wiki(store, "elsewhere");
    let (ann, ben) = (account(store), account(store));

    // --- A thread nobody has written in is an empty page, never an error.
    let empty = store.claim_comments(&w.id, C1, None, 10).unwrap();
    assert!(empty.rows.is_empty() && empty.total == 0 && empty.next.is_none());

    // --- Comments are written once, in order, with their places in the thread.
    let first = comment(&w.id, C1, &ann, None, "Where was this measured?", 100);
    store.add_claim_comment(&first).unwrap();
    let reply = comment(&w.id, C1, &ben, Some(&first.id), "At the north gauge.", 110);
    store.add_claim_comment(&reply).unwrap();
    let third = comment(&w.id, C1, &ann, Some(&reply.id), "Thank you.", 120);
    store.add_claim_comment(&third).unwrap();
    let page = store.claim_comments(&w.id, C1, None, 10).unwrap();
    assert_eq!(page.total, 3);
    assert!(page.next.is_none());
    let got: Vec<(u32, &CommentId, Option<&CommentId>)> = page
        .rows
        .iter()
        .map(|r| (r.position, &r.comment.id, r.comment.parent.as_ref()))
        .collect();
    let parents: Vec<Option<u32>> = page.rows.iter().map(|r| r.parent_position).collect();
    assert_eq!(
        parents,
        [None, Some(1), Some(2)],
        "a reply knows the place of the comment it answers"
    );
    assert_eq!(
        got,
        [
            (1, &first.id, None),
            (2, &reply.id, Some(&first.id)),
            (3, &third.id, Some(&reply.id)),
        ]
    );
    assert_eq!(page.rows.first().unwrap().comment, first);

    // --- A thread is the wiki's and the claim's: nothing leaks to another claim or another wiki.
    store
        .add_claim_comment(&comment(&w.id, C2, &ben, None, "A different claim.", 130))
        .unwrap();
    store
        .add_claim_comment(&comment(
            &other.id,
            C1,
            &ben,
            None,
            "The same id, elsewhere.",
            131,
        ))
        .unwrap();
    assert_eq!(bodies(store, &w.id, C1).len(), 3);
    assert_eq!(bodies(store, &w.id, C2), ["A different claim."]);
    assert_eq!(bodies(store, &other.id, C1), ["The same id, elsewhere."]);
    assert_eq!(
        store
            .claim_comments(&w.id, C2, None, 10)
            .unwrap()
            .rows
            .first()
            .unwrap()
            .position,
        1,
        "each thread counts from one"
    );

    // --- A reply belongs to the thread of the comment it answers.
    for (what, parent) in [
        ("another claim's comment", walked_id(store, &w.id, C2, 1)),
        ("another wiki's comment", walked_id(store, &other.id, C1, 1)),
        ("a comment that is not there", CommentId::new()),
    ] {
        let before = bodies(store, &w.id, C1).len();
        assert_eq!(
            store.add_claim_comment(&comment(&w.id, C1, &ann, Some(&parent), "Misplaced", 140)),
            Err(StoreError::NotFound),
            "{what}"
        );
        assert_eq!(
            bodies(store, &w.id, C1).len(),
            before,
            "{what}: nothing is written"
        );
    }

    // --- What is refused writes nothing.
    assert_eq!(
        store.add_claim_comment(&first),
        Err(StoreError::AlreadyRecorded),
        "a comment is written once"
    );
    for bad in ["", "not-a-claim", &C1.to_uppercase(), &format!("{C1}0")] {
        assert!(
            matches!(
                store.add_claim_comment(&comment(&w.id, bad, &ann, None, "x", 150)),
                Err(StoreError::Backend(_))
            ),
            "{bad:?} is not a claim's id"
        );
    }
    assert!(matches!(
        store.add_claim_comment(&comment(&w.id, C1, &ann, None, "", 150)),
        Err(StoreError::Backend(_))
    ));
    assert_eq!(
        store.add_claim_comment(&comment(&WikiId::new(), C1, &ann, None, "x", 150)),
        Err(StoreError::NotFound)
    );
    assert_eq!(
        store.add_claim_comment(&comment(&w.id, C1, &AccountId::new(), None, "x", 150)),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(bodies(store, &w.id, C1).len(), 3);

    // --- A long thread is read in pages that end where the next begins.
    for n in 0..7 {
        store
            .add_claim_comment(&comment(
                &w.id,
                C2,
                &ann,
                None,
                &format!("c2 #{n}"),
                200 + n,
            ))
            .unwrap();
    }
    let all = bodies(store, &w.id, C2);
    assert_eq!(all.len(), 8);
    let mut walked = Vec::new();
    let mut after: Option<CommentId> = None;
    let mut pages = 0;
    loop {
        let page = store.claim_comments(&w.id, C2, after.as_ref(), 3).unwrap();
        assert_eq!(page.total, 8);
        assert!(page.rows.len() <= 3);
        walked.extend(
            page.rows
                .iter()
                .map(|r| (r.position, r.comment.body.clone())),
        );
        pages += 1;
        match page.next {
            Some(next) => {
                assert_eq!(Some(&next), page.rows.last().map(|r| &r.comment.id));
                after = Some(next);
            }
            None => break,
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(
        walked.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>(),
        all
    );
    assert_eq!(
        walked.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        (1..=8).collect::<Vec<u32>>()
    );
    // A page of exactly the rest has no next; a limit of nothing is one comment; a comment that is not in this thread
    // is not a place to continue from.
    let last_two = store
        .claim_comments(&w.id, C2, Some(&walked_id(store, &w.id, C2, 6)), 5)
        .unwrap();
    assert_eq!(last_two.rows.len(), 2);
    assert!(last_two.next.is_none());
    assert_eq!(
        store.claim_comments(&w.id, C2, None, 0).unwrap().rows.len(),
        1
    );
    assert_eq!(
        store.claim_comments(&w.id, C2, Some(&first.id), 3),
        Err(StoreError::NotFound),
        "a comment of another thread"
    );
    assert_eq!(
        store.claim_comments(&w.id, C2, Some(&CommentId::new()), 3),
        Err(StoreError::NotFound)
    );

    // --- The comment quota counts a person's comments in the wiki from a time on, oldest first.
    assert_eq!(
        store.claim_comments_since(&w.id, &ann, 0).unwrap(),
        vec![100, 120, 200, 201, 202, 203, 204, 205, 206]
    );
    assert_eq!(
        store.claim_comments_since(&w.id, &ann, 120).unwrap().len(),
        8
    );
    assert_eq!(
        store.claim_comments_since(&w.id, &ben, 0).unwrap(),
        vec![110, 130]
    );
    assert_eq!(
        store.claim_comments_since(&other.id, &ben, 0).unwrap(),
        vec![131]
    );
    assert!(
        store
            .claim_comments_since(&other.id, &ann, 0)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .claim_comments_since(&w.id, &ann, 1_000)
            .unwrap()
            .is_empty()
    );

    // --- The whole of a wiki's threads, by claim and in the order written: what the bundle is made from.
    let every = store.claim_comments_in_wiki(&w.id).unwrap();
    assert_eq!(every.len(), 3 + 8);
    let keys: Vec<(&str, u32)> = every
        .iter()
        .map(|r| (r.comment.claim.as_str(), r.position))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
    assert_eq!(keys.first(), Some(&(C1, 1)));
    assert_eq!(keys.last(), Some(&(C2, 8)));
    assert_eq!(store.claim_comments_in_wiki(&other.id).unwrap().len(), 1);
    assert!(
        store
            .claim_comments_in_wiki(&WikiId::new())
            .unwrap()
            .is_empty()
    );
}

/// The id of the comment at `position` in a thread.
fn walked_id(store: &impl ClaimThreadStore, w: &WikiId, claim: &str, position: u32) -> CommentId {
    store
        .claim_comments(w, claim, None, MAX_PAGE)
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.position == position)
        .unwrap()
        .comment
        .id
}

#[test]
fn the_memory_store_keeps_claim_threads() {
    the_suite(&MemoryStore::new());
}

#[test]
fn the_sqlite_store_keeps_the_same_claim_threads() {
    the_suite(&SqliteStore::open_in_memory().unwrap());
}

#[test]
fn a_page_is_never_longer_than_the_bound() {
    for store in [
        &MemoryStore::new() as &dyn Both,
        &SqliteStore::open_in_memory().unwrap(),
    ] {
        store.big_page();
    }
}

/// What the two stores are asked for in one test.
trait Both {
    fn big_page(&self);
}

impl<T: ClaimThreadStore + WikiStore + AccountStore> Both for T {
    fn big_page(&self) {
        let w = wiki(self, "big");
        let a = account(self);
        for n in 0..(MAX_PAGE + 5) {
            self.add_claim_comment(&comment(
                &w.id,
                C1,
                &a,
                None,
                "x",
                i64::try_from(n).unwrap(),
            ))
            .unwrap();
        }
        let page = self.claim_comments(&w.id, C1, None, usize::MAX).unwrap();
        assert_eq!(page.rows.len(), MAX_PAGE);
        assert_eq!(page.total, u64::try_from(MAX_PAGE + 5).unwrap());
        let rest = self
            .claim_comments(&w.id, C1, page.next.as_ref(), usize::MAX)
            .unwrap();
        assert_eq!(rest.rows.len(), 5);
        assert!(rest.next.is_none());
    }
}

// ------------------------------------------------------------------------------------ the database itself

struct Rig {
    _tmp: support::TempDir,
    raw: rusqlite::Connection,
    wiki: WikiId,
    author: AccountId,
    first: CommentId,
}

/// A database file with one comment, and a raw connection to it with foreign keys on.
fn rig(tag: &str) -> Rig {
    let tmp = temp(tag);
    let path = tmp.0.join("store.sqlite");
    let (w, author, first) = {
        let store = SqliteStore::open(&path).unwrap();
        let w = wiki(&store, "threads");
        let author = account(&store);
        let c = comment(&w.id, C1, &author, None, "First.", 10);
        store.add_claim_comment(&c).unwrap();
        (w.id, author, c.id)
    };
    let raw = rusqlite::Connection::open(&path).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    Rig {
        _tmp: tmp,
        raw,
        wiki: w,
        author,
        first,
    }
}

fn message(r: rusqlite::Result<usize>) -> String {
    r.expect_err("the database refuses it").to_string()
}

#[test]
fn the_database_refuses_what_the_store_would_not_write() {
    let r = rig("raw");
    let insert = |id: &str,
                  wiki: &str,
                  claim: &str,
                  seq: i64,
                  author: &str,
                  parent: Option<&str>,
                  body: &str| {
        r.raw.execute(
            "INSERT INTO claim_comments (id, wiki_id, claim_id, seq, author, parent, body, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            rusqlite::params![id, wiki, claim, seq, author, parent, body],
        )
    };
    let (w, a) = (r.wiki.as_str(), r.author.as_str());
    let fresh = || CommentId::new().to_string();
    // The shape: a claim's id, a place from one, a text, a wiki and an author that are there.
    assert!(
        message(insert(&fresh(), w, "not-a-claim", 2, a, None, "x")).contains("CHECK constraint")
    );
    assert!(
        message(insert(&fresh(), w, &C1.to_uppercase(), 2, a, None, "x"))
            .contains("CHECK constraint")
    );
    assert!(message(insert(&fresh(), w, C1, 0, a, None, "x")).contains("CHECK constraint"));
    assert!(message(insert(&fresh(), w, C1, 2, a, None, "")).contains("CHECK constraint"));
    assert!(
        message(insert(
            &fresh(),
            &WikiId::new().to_string(),
            C1,
            2,
            a,
            None,
            "x"
        ))
        .contains("FOREIGN KEY")
    );
    assert!(
        message(insert(
            &fresh(),
            w,
            C1,
            2,
            &AccountId::new().to_string(),
            None,
            "x"
        ))
        .contains("FOREIGN KEY")
    );
    // A place in a thread is taken once, and an id is used once: a replacing insert removes nothing.
    assert!(message(insert(&fresh(), w, C1, 1, a, None, "x")).contains("append-only"));
    assert!(message(insert(r.first.as_str(), w, C2, 1, a, None, "x")).contains("append-only"));
    assert!(
        message(r.raw.execute(
            "INSERT OR REPLACE INTO claim_comments (id, wiki_id, claim_id, seq, author, body, created_at) \
             VALUES (?1, ?2, ?3, 1, ?4, 'replaced', 1)",
            rusqlite::params![fresh(), w, C1, a],
        ))
        .contains("append-only")
    );
    // A reply is in the thread of the comment it answers, from any connection.
    assert!(
        message(insert(&fresh(), w, C2, 1, a, Some(r.first.as_str()), "x"))
            .contains("the thread of the comment it answers")
    );
    insert(&fresh(), w, C1, 2, a, Some(r.first.as_str()), "A reply")
        .expect("a reply in its thread");
    // Once there is a row, none is changed or removed.
    for sql in [
        "UPDATE claim_comments SET body = 'changed'",
        "UPDATE claim_comments SET author = author",
        "DELETE FROM claim_comments",
    ] {
        let refused = r.raw.execute(sql, []).expect_err(sql).to_string();
        assert!(refused.contains("append-only"), "{sql}: {refused}");
    }
    let n: i64 = r
        .raw
        .query_row("SELECT COUNT(*) FROM claim_comments", [], |row| row.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn a_version_17_database_is_migrated_and_threads_work_on_what_it_held() {
    use inklkunn_store::sqlite::MIGRATIONS;
    let tmp = temp("v17");
    let db = tmp.0.join("store.sqlite");
    let wiki_id = "019a0000-0000-7000-8000-0000000000f3";
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 17) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '17')",
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
        let store = SqliteStore::open(&db).expect("a version 17 database opens");
        assert!(
            MIGRATIONS.iter().any(|(v, _)| *v == 18),
            "the claim threads came with version 18"
        );
        assert!(store.claim_comments_in_wiki(&w).unwrap().is_empty());
        let a = account(&store);
        store
            .add_claim_comment(&comment(&w, C1, &a, None, "After the migration.", 5))
            .unwrap();
    }
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(bodies(&store, &w, C1), ["After the migration."]);
}
