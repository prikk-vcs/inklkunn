#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn the_connection_enforces_foreign_keys() {
    // The pre-checks in the store hide a missing pragma from every behavioural test; this one asks the
    // connection itself, because the schema's `REFERENCES` clauses are only as good as this setting.
    let store = SqliteStore::open_in_memory().unwrap();
    let on: i64 = store
        .with(|conn| {
            conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .map_err(backend)
        })
        .unwrap();
    assert_eq!(on, 1);
}
