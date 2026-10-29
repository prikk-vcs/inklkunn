#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::access::{Role, RoleGrant, RoleStore};
use crate::record::{DisplayName, Slug};
use crate::{NewWiki, WikiStore};

fn foreign_keys(store: &SqliteStore) -> i64 {
    store
        .with(|conn| {
            conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .map_err(backend)
        })
        .unwrap()
}

#[test]
fn the_connection_enforces_foreign_keys() {
    // The pre-checks in the store hide a missing pragma from every behavioural test; this one asks the
    // connection itself, because the schema's `REFERENCES` clauses are only as good as this setting.
    let store = SqliteStore::open_in_memory().unwrap();
    assert_eq!(foreign_keys(&store), 1);
}

/// Mutant M19: the store's own `PRAGMA foreign_keys=ON` removed. The bundled SQLite is compiled with
/// `SQLITE_DEFAULT_FOREIGN_KEYS=1`, so a connection the store opens itself has the keys on whether or not the
/// store asks, and no test that starts from `open` can tell: the mutant is equivalent there. What the store
/// owns is the *request*, so this hands it a connection whose keys are off and asks whether it turns them on
/// (the day the build default changes, or a connection comes from elsewhere, the keys are still on).
#[test]
fn the_store_turns_foreign_keys_on_itself_whatever_the_connection_came_with() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let off: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(off, 0, "the starting point: keys off");

    let store = SqliteStore::init(conn, false).unwrap();
    assert_eq!(foreign_keys(&store), 1, "the store switched them on");

    // And it shows: a grant to an account that does not exist is refused by the database itself.
    let wiki = store
        .create(NewWiki::new(
            Slug::new("keys").unwrap(),
            DisplayName::new("Keys").unwrap(),
        ))
        .unwrap();
    let grant = RoleGrant {
        account: AccountId::new(),
        wiki: wiki.id,
        role: Role::Observer,
        granted_by: None,
        granted_at: 1,
    };
    assert_eq!(store.grant(grant).err(), Some(StoreError::AccountNotFound));
}
