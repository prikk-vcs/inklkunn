//! The SQLite schema of governance and moderation (store schema 19; RFC 006 D-6 - D-10, D-14): the database's own refusals
//! from any connection, the migration from 18 (rows intact, the new tables empty, one transaction, the derived table dropped
//! with the rest), and the shape the migrated database shares with a fresh one. The traits are in `governance.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::path::Path;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    AccountId, Actor, AppealId, AppealRecord, AppealState, AppealedAction, AppealedKind,
    DerivedStore, GovernanceSnapshot, GovernanceSnapshotStore, GovernanceStore, GrantDecisionId,
    GrantDecisionRecord, Role, RoleStore, SqliteStore, StoreError, WikiId, WikiStore,
};
use rusqlite::types::Value;
use support::{TempDir, account, temp, wiki};

const NEW_TABLES: [&str; 5] = [
    "grant_decisions",
    "restrictions",
    "protections",
    "tombstones",
    "appeals",
];

fn text(s: &str) -> Value {
    Value::Text(s.to_owned())
}

fn num(n: i64) -> Value {
    Value::Integer(n)
}

const NULL: Value = Value::Null;

/// `base` with the columns of `over` replaced (and added).
fn with(base: &[(&str, Value)], over: &[(&str, Value)]) -> Vec<(String, Value)> {
    let mut merged: Vec<(String, Value)> = base
        .iter()
        .filter(|(k, _)| !over.iter().any(|(o, _)| o == k))
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    merged.extend(over.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    merged
}

fn insert(
    conn: &rusqlite::Connection,
    table: &str,
    row: &[(String, Value)],
) -> rusqlite::Result<usize> {
    insert_with(conn, "INSERT", table, row)
}

fn insert_with(
    conn: &rusqlite::Connection,
    verb: &str,
    table: &str,
    row: &[(String, Value)],
) -> rusqlite::Result<usize> {
    // A column named twice takes its last value.
    let row: Vec<&(String, Value)> = row
        .iter()
        .enumerate()
        .filter(|(i, (k, _))| !row.iter().skip(i + 1).any(|(later, _)| later == k))
        .map(|(_, kv)| kv)
        .collect();
    let columns: Vec<&str> = row.iter().map(|(k, _)| k.as_str()).collect();
    let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
    conn.execute(
        &format!(
            "{verb} INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            marks.join(", ")
        ),
        rusqlite::params_from_iter(row.iter().map(|(_, v)| v)),
    )
}

#[track_caller]
fn why(r: rusqlite::Result<usize>) -> String {
    r.expect_err("the database refuses it").to_string()
}

fn count(conn: &rusqlite::Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

fn version(db: &Path) -> String {
    rusqlite::Connection::open(db)
        .expect("raw open")
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
}

fn schema_objects(db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).expect("raw open");
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger', 'index') \
             AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_fts%' ORDER BY name",
        )
        .expect("prepare");
    stmt.query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("names")
}

/// A database file with a wiki, three accounts and a raw connection to it, foreign keys on.
struct Raw {
    _tmp: TempDir,
    conn: rusqlite::Connection,
    wiki: String,
    other_wiki: String,
    a: String,
    b: String,
    c: String,
}

fn raw(tag: &str) -> Raw {
    let tmp = temp(tag);
    let path = tmp.0.join("store.sqlite");
    let (w, w2, a, b, c) = {
        let store = SqliteStore::open(&path).expect("open");
        (
            wiki(&store, "schema").id.to_string(),
            wiki(&store, "elsewhere").id.to_string(),
            account(&store).to_string(),
            account(&store).to_string(),
            account(&store).to_string(),
        )
    };
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("pragma");
    Raw {
        _tmp: tmp,
        conn,
        wiki: w,
        other_wiki: w2,
        a,
        b,
        c,
    }
}

fn uuid() -> String {
    AccountId::new().to_string()
}

impl Raw {
    fn appeal(&self, over: &[(&str, Value)]) -> Vec<(String, Value)> {
        with(
            &[
                ("id", text(&uuid())),
                ("seq", num(1)),
                ("wiki_id", text(&self.wiki)),
                ("at", num(10)),
                ("action_kind", text("restriction")),
                ("action_id", text(&uuid())),
                ("action_actor", text(&self.a)),
                ("appellant", text(&self.b)),
                ("text", text("Please look again.")),
                ("status", text("open")),
            ],
            over,
        )
    }

    fn grant(&self, over: &[(&str, Value)]) -> Vec<(String, Value)> {
        with(
            &[
                ("id", text(&uuid())),
                ("seq", num(1)),
                ("wiki_id", text(&self.wiki)),
                ("at", num(10)),
                ("actor", text(&self.a)),
                ("actor_rung", text("admin")),
                ("target", text(&self.b)),
                ("rung_before", NULL),
                ("rung_after", text("reviewer")),
            ],
            over,
        )
    }

    fn restriction(&self, over: &[(&str, Value)]) -> Vec<(String, Value)> {
        with(
            &[
                ("id", text(&uuid())),
                ("seq", num(1)),
                ("wiki_id", text(&self.wiki)),
                ("at", num(10)),
                ("event", text("set")),
                ("account", text(&self.b)),
                ("actor", text(&self.a)),
                ("actor_rung", text("maintainer")),
                ("reason", text("A reason")),
                ("expires_at", num(100)),
            ],
            over,
        )
    }

    fn protection(&self, over: &[(&str, Value)]) -> Vec<(String, Value)> {
        with(
            &[
                ("id", text(&uuid())),
                ("seq", num(1)),
                ("wiki_id", text(&self.wiki)),
                ("at", num(10)),
                ("event", text("set")),
                ("kind", text("page")),
                ("target", text("Help/x")),
                ("requirement", text("review")),
                ("actor", text(&self.a)),
                ("actor_rung", text("maintainer")),
                ("reason", text("A reason")),
                ("expires_at", num(100)),
            ],
            over,
        )
    }

    fn tombstone(&self, over: &[(&str, Value)]) -> Vec<(String, Value)> {
        with(
            &[
                ("id", text(&uuid())),
                ("seq", num(1)),
                ("wiki_id", text(&self.wiki)),
                ("at", num(10)),
                ("event", text("hide")),
                ("item_kind", text("proposal-comment")),
                ("item_id", text(&uuid())),
                ("hides_content", num(1)),
                ("hides_summary", num(0)),
                ("hides_author", num(0)),
                ("reason_category", text("spam")),
                ("actor", text(&self.a)),
                ("actor_rung", text("maintainer")),
            ],
            over,
        )
    }
}

#[test]
fn the_database_refuses_a_tombstone_the_store_would_not_write() {
    let r = raw("tombstones");
    let t = |over: &[(&str, Value)]| why(insert(&r.conn, "tombstones", &r.tombstone(over)));
    // The closed sets: a reason category of six, four kinds of item, two events, bounded flags.
    assert!(t(&[("reason_category", text("racism"))]).contains("CHECK constraint"));
    assert!(t(&[("reason_category", text("Spam"))]).contains("CHECK constraint"));
    assert!(t(&[("item_kind", text("page"))]).contains("CHECK constraint"));
    assert!(t(&[("event", text("remove"))]).contains("CHECK constraint"));
    assert!(t(&[("hides_content", num(2))]).contains("CHECK constraint"));
    // Something is hidden; a comment has no summary and no reference; a revision of a proposal has a number.
    assert!(t(&[("hides_content", num(0))]).contains("CHECK constraint"));
    assert!(t(&[("hides_summary", num(1))]).contains("CHECK constraint"));
    assert!(t(&[("item_ref", text("1"))]).contains("CHECK constraint"));
    let revision = [("item_kind", text("proposal-revision"))];
    assert!(t(&revision).contains("CHECK constraint"));
    assert!(t(&[revision[0].clone(), ("item_ref", text("one"))]).contains("CHECK constraint"));
    // The identifier is plain; the bounds hold; a role is one of six.
    assert!(t(&[("item_id", text("NOT/AN/ID"))]).contains("CHECK constraint"));
    assert!(t(&[("item_id", text(&"a".repeat(129)))]).contains("CHECK constraint"));
    assert!(t(&[("note", text(""))]).contains("CHECK constraint"));
    assert!(t(&[("note", text(&"x".repeat(4097)))]).contains("CHECK constraint"));
    assert!(t(&[("actor_rung", text("moderator"))]).contains("CHECK constraint"));
    assert!(t(&[("policy_block", text("XYZ"))]).contains("CHECK constraint"));
    // A hide follows nothing and has a category; an unhide follows a hide and has none; only a revision's hide declines.
    assert!(t(&[("of_event", text(&uuid()))]).contains("an unhide follows a hide"));
    assert!(t(&[("reason_category", NULL)]).contains("CHECK constraint"));
    assert!(t(&[("event", text("unhide"))]).contains("CHECK constraint"));
    assert!(t(&[("declined_transition", num(1))]).contains("CHECK constraint"));
    // Accounts, wikis and the proposal log are there.
    assert!(t(&[("actor", text(&uuid()))]).contains("FOREIGN KEY"));
    assert!(t(&[("affected", text(&uuid()))]).contains("FOREIGN KEY"));
    assert!(t(&[("wiki_id", text(&uuid()))]).contains("FOREIGN KEY"));
    assert!(
        t(&[
            revision[0].clone(),
            ("item_ref", text("1")),
            ("declined_transition", num(77))
        ])
        .contains("FOREIGN KEY")
    );
    // What is right is accepted: every category and every kind.
    for category in [
        "personal-data",
        "harassment",
        "unlawful",
        "copyright",
        "spam",
        "other",
    ] {
        insert(
            &r.conn,
            "tombstones",
            &r.tombstone(&[("reason_category", text(category))]),
        )
        .expect(category);
    }
    insert(
        &r.conn,
        "tombstones",
        &r.tombstone(&[revision[0].clone(), ("item_ref", text("12"))]),
    )
    .expect("a revision");
    insert(
        &r.conn,
        "tombstones",
        &r.tombstone(&[
            ("item_kind", text("page-revision")),
            ("item_id", text(&"ab".repeat(32))),
            ("item_ref", text("Help/x")),
        ]),
    )
    .expect("a page revision");
    // An unhide follows a hide of the same item and parts, once; from any connection.
    let hide_id = uuid();
    let item = uuid();
    insert(
        &r.conn,
        "tombstones",
        &r.tombstone(&[("id", text(&hide_id)), ("item_id", text(&item))]),
    )
    .expect("a hide");
    let unhide = |over: &[(&str, Value)]| {
        let base = [
            ("event", text("unhide")),
            ("reason_category", NULL),
            ("item_id", text(&item)),
            ("of_event", text(&hide_id)),
        ];
        let mut all: Vec<(&str, Value)> = base
            .iter()
            .filter(|(k, _)| !over.iter().any(|(o, _)| o == k))
            .cloned()
            .collect();
        all.extend(over.iter().map(|(k, v)| (*k, v.clone())));
        why(insert(&r.conn, "tombstones", &r.tombstone(&all)))
    };
    assert!(
        unhide(&[("hides_author", num(1))])
            .contains("an unhide follows a hide of the same item and parts")
    );
    assert!(
        unhide(&[("item_id", text(&uuid()))])
            .contains("an unhide follows a hide of the same item and parts")
    );
    assert!(unhide(&[("at", num(5))]).contains("an unhide follows a hide"));
    assert!(unhide(&[("of_event", text(&uuid()))]).contains("an unhide follows a hide"));
    insert(
        &r.conn,
        "tombstones",
        &r.tombstone(&[
            ("event", text("unhide")),
            ("reason_category", NULL),
            ("item_id", text(&item)),
            ("of_event", text(&hide_id)),
        ]),
    )
    .expect("an unhide");
    assert!(
        unhide(&[]).contains("append-only"),
        "a hide has at most one unhide"
    );
}

#[test]
fn the_database_refuses_a_grant_a_restriction_and_a_protection_the_store_would_not_write() {
    let r = raw("measures");
    // Grants: a rung is one of six, the operator has no rung, nothing changes nothing.
    let g = |over: &[(&str, Value)]| why(insert(&r.conn, "grant_decisions", &r.grant(over)));
    assert!(g(&[("rung_after", text("owner"))]).contains("CHECK constraint"));
    assert!(
        g(&[("rung_before", text("reviewer"))]).contains("CHECK constraint"),
        "before and after are the same"
    );
    assert!(
        g(&[("rung_after", NULL)]).contains("CHECK constraint"),
        "no rung to none"
    );
    assert!(g(&[("actor_rung", NULL)]).contains("CHECK constraint"));
    assert!(g(&[("actor", NULL)]).contains("CHECK constraint"));
    assert!(g(&[("reason", text(""))]).contains("CHECK constraint"));
    assert!(g(&[("target", text(&uuid()))]).contains("FOREIGN KEY"));
    assert!(g(&[("appeal_id", text(&uuid()))]).contains("an open appeal of a grant in this wiki"));
    insert(
        &r.conn,
        "grant_decisions",
        &r.grant(&[("actor", NULL), ("actor_rung", NULL)]),
    )
    .expect("the operator grants");
    insert(
        &r.conn,
        "grant_decisions",
        &r.grant(&[("rung_before", text("reviewer")), ("rung_after", NULL)]),
    )
    .expect("a revocation");

    // Restrictions: the events, the chain shape, the end after the beginning, nobody on their own.
    let x = |over: &[(&str, Value)]| why(insert(&r.conn, "restrictions", &r.restriction(over)));
    assert!(x(&[("event", text("expired"))]).contains("CHECK constraint"));
    assert!(
        x(&[("actor", text(&r.b))]).contains("CHECK constraint"),
        "nobody acts on their own restriction"
    );
    assert!(x(&[("expires_at", num(10))]).contains("CHECK constraint"));
    assert!(x(&[("expires_at", NULL)]).contains("CHECK constraint"));
    assert!(x(&[("reason", NULL)]).contains("CHECK constraint"));
    assert!(
        x(&[("of_event", text(&uuid()))]).contains("follows a set or renewed event"),
        "a set follows nothing"
    );
    assert!(
        x(&[("event", text("lifted")), ("expires_at", NULL)]).contains("CHECK constraint"),
        "a lift follows a set"
    );
    let set_id = uuid();
    insert(
        &r.conn,
        "restrictions",
        &r.restriction(&[("id", text(&set_id))]),
    )
    .expect("a set");
    let follow = |over: &[(&str, Value)]| {
        let mut all = vec![
            ("event", text("lifted")),
            ("expires_at", NULL),
            ("of_event", text(&set_id)),
            ("at", num(20)),
        ];
        all.extend(over.iter().map(|(k, v)| (*k, v.clone())));
        why(insert(&r.conn, "restrictions", &r.restriction(&all)))
    };
    assert!(follow(&[("account", text(&r.c))]).contains("the same account in the same wiki"));
    assert!(follow(&[("at", num(5))]).contains("follows a set or renewed event"));
    assert!(
        follow(&[
            ("reason", NULL),
            ("event", text("reversed")),
            ("appeal_id", text(&uuid()))
        ])
        .contains("an open appeal of a restriction in this wiki")
    );
    assert!(
        follow(&[("event", text("reversed"))]).contains("CHECK constraint"),
        "a reversal names an appeal"
    );
    insert(
        &r.conn,
        "restrictions",
        &r.restriction(&[
            ("event", text("lifted")),
            ("expires_at", NULL),
            ("of_event", text(&set_id)),
            ("at", num(20)),
        ]),
    )
    .expect("a lift");
    assert!(
        follow(&[]).contains("append-only"),
        "an event has at most one that follows it"
    );

    // Protections: the kind, the requirement and its rung, the bound on the target.
    let p = |over: &[(&str, Value)]| why(insert(&r.conn, "protections", &r.protection(over)));
    assert!(p(&[("kind", text("wiki"))]).contains("CHECK constraint"));
    assert!(p(&[("requirement", text("approvals"))]).contains("CHECK constraint"));
    assert!(
        p(&[("requirement", text("rung"))]).contains("CHECK constraint"),
        "a rung requirement names its rung"
    );
    assert!(
        p(&[("rung", text("maintainer"))]).contains("CHECK constraint"),
        "a review requirement names no rung"
    );
    assert!(p(&[("requirement", NULL)]).contains("CHECK constraint"));
    assert!(p(&[("target", text(""))]).contains("CHECK constraint"));
    assert!(p(&[("target", text(&"x".repeat(1025)))]).contains("CHECK constraint"));
    assert!(p(&[("expires_at", num(9))]).contains("CHECK constraint"));
    insert(
        &r.conn,
        "protections",
        &r.protection(&[("requirement", text("rung")), ("rung", text("maintainer"))]),
    )
    .expect("a rung protection");
    insert(
        &r.conn,
        "protections",
        &r.protection(&[
            ("kind", text("namespace")),
            ("target", text(&"é".repeat(512))),
        ]),
    )
    .expect("a target at the byte bound");
    assert!(
        p(&[("target", text(&"é".repeat(513)))]).contains("CHECK constraint"),
        "bounded in bytes, not characters"
    );
}

#[test]
fn the_database_refuses_a_second_open_appeal_and_decides_once() {
    let r = raw("appeals");
    let a = |over: &[(&str, Value)]| why(insert(&r.conn, "appeals", &r.appeal(over)));
    assert!(a(&[("action_kind", text("rule"))]).contains("CHECK constraint"));
    assert!(a(&[("status", text("withdrawn"))]).contains("filed open"));
    assert!(a(&[("text", text(""))]).contains("CHECK constraint"));
    assert!(a(&[("text", text(&"x".repeat(16385)))]).contains("CHECK constraint"));
    assert!(a(&[("action_id", text("Not-An-Id"))]).contains("CHECK constraint"));
    assert!(a(&[("action_id", text(&"a".repeat(37)))]).contains("CHECK constraint"));
    assert!(a(&[("appellant", text(&uuid()))]).contains("FOREIGN KEY"));
    // Filed open, and open means undecided.
    assert!(a(&[("decided_at", num(11))]).contains("CHECK constraint"));
    assert!(
        a(&[
            ("status", text("upheld")),
            ("decided_at", num(11)),
            ("decided_by", text(&r.c)),
            ("decision_reason", text("Fine."))
        ])
        .contains("filed open")
    );

    // One open appeal per action per person: the trigger says it (a replacing insert too) ...
    let action = uuid();
    let first = uuid();
    insert(
        &r.conn,
        "appeals",
        &r.appeal(&[("id", text(&first)), ("action_id", text(&action))]),
    )
    .expect("an appeal");
    assert!(a(&[("action_id", text(&action))]).contains("one open appeal per action per person"));
    let replacing = format!(
        "INSERT OR REPLACE INTO appeals (id, seq, wiki_id, at, action_kind, action_id, appellant, text, status) \
         VALUES ('{}', 2, '{}', 11, 'restriction', '{action}', '{}', 'again', 'open')",
        uuid(),
        r.wiki,
        r.b
    );
    assert!(why(r.conn.execute(&replacing, [])).contains("append-only"));
    assert_eq!(
        count(&r.conn, "appeals"),
        1,
        "a replacing insert removed nothing"
    );
    // ... and the partial unique index says it on its own, with the trigger gone.
    r.conn
        .execute_batch("DROP TRIGGER appeals_no_replace;")
        .expect("drop");
    assert!(a(&[("action_id", text(&action))]).contains("UNIQUE constraint failed"));
    // Another person, or another action, is not the same appeal.
    insert(
        &r.conn,
        "appeals",
        &r.appeal(&[("action_id", text(&action)), ("appellant", text(&r.c))]),
    )
    .expect("another person");
    insert(
        &r.conn,
        "appeals",
        &r.appeal(&[("action_kind", text("grant")), ("action_id", text(&action))]),
    )
    .expect("another kind");

    // Decided once: open to upheld, by someone who neither appealed nor acted, and nothing else about it ever changes.
    let decide = |sql: &str| why(r.conn.execute(sql, []));
    assert!(
        decide(&format!(
            "UPDATE appeals SET text = 'changed' WHERE id = '{first}'"
        ))
        .contains("append-only")
    );
    assert!(decide(&format!("UPDATE appeals SET status = 'upheld', text = 'changed', decided_at = 11, decided_by = '{}', decision_reason = 'x' WHERE id = '{first}'", r.c)).contains("append-only"));
    assert!(decide(&format!("UPDATE appeals SET status = 'upheld', decided_at = 11, decided_by = '{}', decision_reason = 'x' WHERE id = '{first}'", r.b)).contains("CHECK constraint"), "not the appellant");
    assert!(decide(&format!("UPDATE appeals SET status = 'upheld', decided_at = 11, decided_by = '{}', decision_reason = 'x' WHERE id = '{first}'", r.a)).contains("CHECK constraint"), "not the one who took the action");
    assert!(decide(&format!("UPDATE appeals SET status = 'upheld', decided_at = 5, decided_by = '{}', decision_reason = 'x' WHERE id = '{first}'", r.c)).contains("CHECK constraint"), "not before it was filed");
    assert!(decide(&format!("UPDATE appeals SET status = 'upheld', decided_at = 11, decided_by = '{}' WHERE id = '{first}'", r.c)).contains("CHECK constraint"), "a decision has its reason");
    assert!(decide(&format!("UPDATE appeals SET status = 'overturned', decided_at = 11, decided_by = '{}', decision_reason = 'x' WHERE id = '{first}'", r.c)).contains("names the record that reversed the action"), "an overturn names its record");
    assert!(decide(&format!("UPDATE appeals SET status = 'overturned', decided_at = 11, decided_by = '{}', decision_reason = 'x', overturn_id = '{}' WHERE id = '{first}'", r.c, uuid())).contains("names the record that reversed the action"));
    r.conn
        .execute(
            &format!("UPDATE appeals SET status = 'upheld', decided_at = 11, decided_by = '{}', decision_reason = 'Looked again.' WHERE id = '{first}'", r.c),
            [],
        )
        .expect("an open appeal is decided");
    assert!(decide(&format!("UPDATE appeals SET status = 'overturned', decided_at = 12, decided_by = '{}', decision_reason = 'x', overturn_id = '{}' WHERE id = '{first}'", r.c, uuid())).contains("append-only"));
    assert!(
        decide(&format!(
            "UPDATE appeals SET decision_reason = 'changed' WHERE id = '{first}'"
        ))
        .contains("append-only")
    );
    assert!(decide(&format!("DELETE FROM appeals WHERE id = '{first}'")).contains("append-only"));
}

#[test]
fn nothing_governance_wrote_is_changed_or_removed_or_replaced() {
    let r = raw("append-only");
    let ids: Vec<(&str, String)> = NEW_TABLES
        .iter()
        .filter(|t| **t != "appeals")
        .map(|t| (*t, uuid()))
        .collect();
    for (table, id) in &ids {
        let row = match *table {
            "grant_decisions" => r.grant(&[("id", text(id))]),
            "restrictions" => r.restriction(&[("id", text(id))]),
            "protections" => r.protection(&[("id", text(id))]),
            _ => r.tombstone(&[("id", text(id))]),
        };
        insert(&r.conn, table, &row).expect(table);
    }
    for (table, id) in &ids {
        for sql in [
            format!("UPDATE {table} SET at = at + 1"),
            format!("UPDATE {table} SET wiki_id = wiki_id"),
            format!("DELETE FROM {table}"),
        ] {
            let refused = why(r.conn.execute(&sql, []));
            assert!(refused.contains("append-only"), "{sql}: {refused}");
        }
        // A replacing insert over an id taken would remove the row without firing the delete trigger.
        let row = match *table {
            "grant_decisions" => r.grant(&[("id", text(id)), ("rung_after", text("observer"))]),
            "restrictions" => r.restriction(&[("id", text(id)), ("reason", text("Replaced"))]),
            "protections" => r.protection(&[("id", text(id)), ("reason", text("Replaced"))]),
            _ => r.tombstone(&[("id", text(id)), ("reason_category", text("other"))]),
        };
        let refused = why(insert_with(&r.conn, "INSERT OR REPLACE", table, &row));
        assert!(refused.contains("append-only"), "{table}: {refused}");
        assert_eq!(count(&r.conn, table), 1, "{table}");
    }
}

#[test]
fn a_reversal_names_an_open_appeal_of_its_own_kind_in_its_own_wiki() {
    let r = raw("by-appeal");
    let appeal_id = uuid();
    insert(
        &r.conn,
        "appeals",
        &r.appeal(&[("id", text(&appeal_id)), ("action_kind", text("tombstone"))]),
    )
    .expect("an appeal of a tombstone");
    // A reversal of another kind of action does not borrow it.
    let set_id = uuid();
    insert(
        &r.conn,
        "restrictions",
        &r.restriction(&[("id", text(&set_id))]),
    )
    .expect("a set");
    let reversed = r.restriction(&[
        ("event", text("reversed")),
        ("expires_at", NULL),
        ("reason", NULL),
        ("of_event", text(&set_id)),
        ("appeal_id", text(&appeal_id)),
        ("at", num(20)),
    ]);
    assert!(
        why(insert(&r.conn, "restrictions", &reversed))
            .contains("an open appeal of a restriction in this wiki")
    );
    let protection_set = uuid();
    insert(
        &r.conn,
        "protections",
        &r.protection(&[("id", text(&protection_set))]),
    )
    .expect("a protection");
    let reversed_protection = r.protection(&[
        ("event", text("reversed")),
        ("requirement", NULL),
        ("expires_at", NULL),
        ("reason", NULL),
        ("of_event", text(&protection_set)),
        ("appeal_id", text(&appeal_id)),
        ("at", num(20)),
    ]);
    assert!(
        why(insert(&r.conn, "protections", &reversed_protection))
            .contains("an open appeal of a protection in this wiki")
    );
    let grant = r.grant(&[("appeal_id", text(&appeal_id))]);
    assert!(
        why(insert(&r.conn, "grant_decisions", &grant))
            .contains("an open appeal of a grant in this wiki")
    );
    // An appeal of the right kind in another wiki is not it either, and an appeal that is decided is not open.
    let elsewhere = {
        let appeal_id = uuid();
        insert(
            &r.conn,
            "appeals",
            &r.appeal(&[("id", text(&appeal_id)), ("action_kind", text("grant"))]),
        )
        .expect("an appeal of a grant");
        why(insert(
            &r.conn,
            "grant_decisions",
            &r.grant(&[
                ("appeal_id", text(&appeal_id)),
                ("wiki_id", text(&r.other_wiki)),
            ]),
        ))
    };
    assert!(
        elsewhere.contains("an open appeal of a grant in this wiki"),
        "{elsewhere}"
    );
    let hide_id = uuid();
    insert(
        &r.conn,
        "tombstones",
        &r.tombstone(&[("id", text(&hide_id))]),
    )
    .expect("a hide");
    r.conn
        .execute(
            &format!("UPDATE appeals SET status = 'upheld', decided_at = 30, decided_by = '{}', decision_reason = 'No.' WHERE id = '{appeal_id}'", r.c),
            [],
        )
        .expect("decided");
    let unhide = r.tombstone(&[
        ("event", text("unhide")),
        ("reason_category", NULL),
        ("item_id", text(&item_of(&r.conn, &hide_id))),
        ("of_event", text(&hide_id)),
        ("appeal_id", text(&appeal_id)),
        ("at", num(20)),
    ]);
    assert!(
        why(insert(&r.conn, "tombstones", &unhide))
            .contains("an open appeal of a tombstone in this wiki")
    );
}

fn item_of(conn: &rusqlite::Connection, tombstone: &str) -> String {
    conn.query_row(
        "SELECT item_id FROM tombstones WHERE id = ?1",
        [tombstone],
        |r| r.get(0),
    )
    .expect("item")
}

#[test]
fn the_derived_governance_table_is_checked_by_the_database_too() {
    let r = raw("derived");
    let d = |over: &[(&str, Value)]| {
        let row = with(
            &[
                ("wiki_id", text(&r.wiki)),
                ("tip", text("tip")),
                ("built_at", num(1)),
            ],
            over,
        );
        why(insert(&r.conn, "governance_derived", &row))
    };
    assert!(
        d(&[("fault_file", text("governance/policy.yaml"))]).contains("CHECK constraint"),
        "a fault names its rule"
    );
    assert!(d(&[("fault_rule", text("unknown-key"))]).contains("CHECK constraint"));
    assert!(
        d(&[("fault_line", num(3))]).contains("CHECK constraint"),
        "a line is a fault's"
    );
    assert!(
        d(&[
            ("fault_file", text("f")),
            ("fault_rule", text("r")),
            ("fault_line", num(0))
        ])
        .contains("CHECK constraint")
    );
    assert!(d(&[("policy_block", text("not hex"))]).contains("CHECK constraint"));
    assert!(d(&[("policy", text(&"x".repeat(1_048_577)))]).contains("CHECK constraint"));
    insert(
        &r.conn,
        "governance_derived",
        &with(
            &[
                ("wiki_id", text(&r.wiki)),
                ("tip", text("tip")),
                ("built_at", num(1)),
            ],
            &[],
        ),
    )
    .expect("a snapshot of nothing");
    assert!(
        d(&[]).contains("UNIQUE constraint failed"),
        "one row per wiki and tip"
    );
}

#[test]
fn the_indexes_that_enforce_and_serve_are_there() {
    let tmp = temp("indexes");
    let db = tmp.0.join("store.sqlite");
    drop(SqliteStore::open(&db).expect("open"));
    let conn = rusqlite::Connection::open(&db).expect("raw open");
    let sql = |name: &str| -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    let open = sql("appeals_one_open");
    assert!(
        open.contains("UNIQUE") && open.contains("WHERE status = 'open'"),
        "{open}"
    );
    assert!(
        open.contains("wiki_id, action_kind, action_id, appellant"),
        "{open}"
    );
    for name in [
        "restrictions_one_follow",
        "protections_one_follow",
        "tombstones_one_unhide",
    ] {
        let follow = sql(name);
        assert!(
            follow.contains("UNIQUE") && follow.contains("WHERE of_event IS NOT NULL"),
            "{follow}"
        );
    }
    // The lookups each surface needs: a wiki's restrictions of an account, protections of a target, the moderation log,
    // the tombstones of an item, the appeals queue, a person's appeals.
    for name in [
        "restrictions_account",
        "protections_target",
        "tombstones_wiki",
        "tombstones_item",
        "appeals_wiki",
        "appeals_appellant",
        "appeals_action",
        "grant_decisions_wiki",
        "grant_decisions_target",
    ] {
        sql(name);
    }
}

// ------------------------------------------------------------------------------------------------ the migration

/// A database at version 18, built from the migrations themselves, with a wiki, accounts, a grant and a derived page.
fn version_18(db: &Path) -> (String, String, String) {
    let (wiki_id, admin, other) = (
        "019a0000-0000-7000-8000-0000000000a1".to_owned(),
        "019a0000-0000-7000-8000-0000000000a2".to_owned(),
        "019a0000-0000-7000-8000-0000000000a3".to_owned(),
    );
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 18) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '18')",
        [],
    )
    .expect("version");
    raw.execute(
        "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
         policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
         maintainer_key_id, created_at, seq) VALUES (?1, 'old', 'old', 'heads/main', 'CC-BY-SA-4.0', '[\"en\"]', \
         'public', 'default', NULL, 'verified', 1700000000, NULL, NULL, 1700000000, 1)",
        [&wiki_id],
    )
    .expect("a wiki");
    for (id, name) in [(&admin, "Admin"), (&other, "Other")] {
        raw.execute(
            "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, ?2, 1700000000)",
            [id.as_str(), name],
        )
        .expect("an account");
    }
    raw.execute(
        "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) VALUES (?1, ?2, 'admin', NULL, 1700000000, 1)",
        [&wiki_id, &admin],
    )
    .expect("a grant");
    raw.execute(
        "INSERT INTO claim_comments (id, wiki_id, claim_id, seq, author, parent, body, created_at) \
         VALUES (?1, ?2, ?3, 1, ?4, NULL, 'A comment from 0.3.0.', 1700000001)",
        [
            &uuid(),
            &wiki_id,
            "019a0000-0000-7000-8000-0000000000c1",
            &admin,
        ],
    )
    .expect("a claim comment");
    raw.execute(
        "INSERT INTO derived_pages (wiki_id, tip, path, lang, presentation, html, meta, links, toc, computed_at) \
         VALUES (?1, 'tip', 'pages/home.md', 'en', 'page', '<p>x</p>', '{}', '[]', '[]', 1)",
        [&wiki_id],
    )
    .expect("a derived page");
    (wiki_id, admin, other)
}

#[test]
fn a_version_18_database_is_migrated_with_its_rows_intact_and_the_new_tables_empty_and_working() {
    let tmp = temp("v18");
    let db = tmp.0.join("store.sqlite");
    let (wiki_id, admin_id, other_id) = version_18(&db);
    assert_eq!(version(&db), "18");
    assert!(
        MIGRATIONS.iter().any(|(v, _)| *v == 19),
        "the governance tables came with version 19"
    );
    assert_eq!(SCHEMA_VERSION, 19);
    let (w, admin, other) = (
        WikiId::parse(&wiki_id).unwrap(),
        AccountId::parse(&admin_id).unwrap(),
        AccountId::parse(&other_id).unwrap(),
    );
    {
        let store = SqliteStore::open(&db).expect("a version 18 database opens");
        assert_eq!(version(&db), "19");
        // What was there is there.
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.role(&admin, &w).unwrap(), Some(Role::Admin));
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        assert_eq!(count(&conn, "claim_comments"), 1);
        assert_eq!(
            count(&conn, "derived_pages"),
            1,
            "the migration drops nothing derived"
        );
        // The new tables exist, empty: nothing is written on upgrade.
        for table in NEW_TABLES.iter().chain(&["governance_derived"]) {
            assert_eq!(count(&conn, table), 0, "{table}");
        }
        // And they work on the old wiki and the old accounts.
        let decision = GrantDecisionRecord {
            id: GrantDecisionId::new(),
            wiki: w.clone(),
            at: 1_700_000_100,
            by: Some(Actor {
                account: admin.clone(),
                rung: Role::Admin,
            }),
            policy_block: None,
            target: other.clone(),
            before: None,
            after: Some(Role::Contributor),
            reason: None,
            appeal: None,
        };
        store
            .record_grant_decision(&decision)
            .expect("a decision on the old wiki");
        assert_eq!(store.role(&other, &w).unwrap(), Some(Role::Contributor));
        store
            .file_appeal(&AppealRecord {
                id: AppealId::new(),
                wiki: w.clone(),
                at: 1_700_000_200,
                action: AppealedAction {
                    kind: AppealedKind::Grant,
                    id: decision.id.to_string(),
                },
                action_actor: Some(admin.clone()),
                appellant: other.clone(),
                text: "An appeal.".to_owned(),
                state: AppealState::Open,
            })
            .expect("an appeal");
        store
            .put_governance_snapshot(&w, "tip", &GovernanceSnapshot::default(), 1)
            .expect("a snapshot");
    }
    // Opening it again changes nothing and loses nothing; a build that knows less is refused, as before.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(version(&db), "19");
    assert_eq!(store.grant_decisions(&w, None, 10).unwrap().len(), 1);
    assert_eq!(store.open_appeal_count(&w, &other).unwrap(), 1);
    // The derived governance row is dropped with the rest of the derived store; the authoritative rows are not.
    assert_eq!(
        store.governance_snapshot(&w, "tip").unwrap(),
        Some(GovernanceSnapshot::default())
    );
    assert_eq!(store.clear(&w).unwrap(), 2, "the page and the snapshot");
    assert_eq!(store.governance_snapshot(&w, "tip").unwrap(), None);
    assert_eq!(store.grant_decisions(&w, None, 10).unwrap().len(), 1);
    drop(store);
    // The migrated schema is the fresh schema: the same tables, indexes and triggers.
    let fresh = tmp.0.join("fresh.sqlite");
    drop(SqliteStore::open(&fresh).expect("fresh"));
    assert_eq!(schema_objects(&db), schema_objects(&fresh));
    // A database written by a newer build is refused before anything is written.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
            [(SCHEMA_VERSION + 1).to_string()],
        )
        .unwrap();
    assert!(matches!(
        SqliteStore::open(&db),
        Err(StoreError::SchemaVersion { .. })
    ));
}

#[test]
fn a_migration_that_fails_part_way_leaves_version_18_and_its_tables_untouched() {
    let tmp = temp("v18-fails");
    let db = tmp.0.join("store.sqlite");
    let (wiki_id, ..) = version_18(&db);
    // A table of the new version's name with another shape: the migration's first table is made, then an index on this one
    // cannot be, and the whole transaction goes.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("CREATE TABLE grant_decisions (something_else TEXT);")
        .unwrap();
    let before = schema_objects(&db);
    let err = SqliteStore::open(&db).expect_err("the migration fails");
    assert!(matches!(err, StoreError::Backend(_)), "{err:?}");
    assert_eq!(version(&db), "18");
    assert_eq!(
        schema_objects(&db),
        before,
        "no table, index or trigger of version 19 is left behind"
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(count(&conn, "role_grants"), 1);
    let wikis: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM wikis WHERE id = ?1",
            [&wiki_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wikis, 1);
}
