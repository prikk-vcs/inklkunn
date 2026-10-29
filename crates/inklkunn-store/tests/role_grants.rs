//! The role grants' foreign keys to accounts (handoff 002 C10, schema version 6): the migration from a
//! version 5 database (data kept, or refused by name and nothing changed), a fresh database, and the
//! connection that enforces them through the public API. (Mutant M19, the store not asking for
//! `foreign_keys=ON`, is killed in `src/sqlite/identity/tests.rs`: the bundled SQLite has the keys on by
//! default, so only a connection handed over with them off can show whether the store asks.)
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, NewWiki, Role, RoleGrant, RoleStore, Slug,
    SqliteStore, StoreError, WikiId, WikiStore,
};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "inklkunn-role-grants-{tag}-{}-{}",
        std::process::id(),
        AccountId::new()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    TempDir(dir)
}

fn external_account() -> Account {
    Account {
        id: AccountId::new(),
        display_name: DisplayName::new("Somebody").expect("a name"),
        local: None,
        created_at: 1,
        disabled: false,
    }
}

/// A database exactly as build 5 left it: versions 1 to 5, the version row at 5, and these rows. Foreign keys
/// are off on the raw connection, as they were for the table that had none.
fn make_version_5(db: &Path, accounts: &[&AccountId], grants: &[(&str, &str, Option<&str>, i64)]) {
    let raw = rusqlite::Connection::open(db).expect("raw open");
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 5) {
        raw.execute_batch(sql)
            .unwrap_or_else(|e| panic!("v{version}: {e}"));
    }
    raw.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '5')",
        [],
    )
    .expect("version");
    for account in accounts {
        raw.execute(
            "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, 'Somebody', 1)",
            [account.as_str()],
        )
        .expect("account");
    }
    for (n, (wiki, account, by, at)) in grants.iter().enumerate() {
        raw.execute(
            "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
             VALUES (?1, ?2, 'admin', ?3, ?4, ?5)",
            rusqlite::params![wiki, account, by, at, i64::try_from(n).unwrap() + 1],
        )
        .expect("grant");
    }
}

fn raw_one<T: rusqlite::types::FromSql>(db: &Path, sql: &str) -> T {
    rusqlite::Connection::open(db)
        .expect("raw open")
        .query_row(sql, [], |r| r.get(0))
        .expect(sql)
}

fn version(db: &Path) -> String {
    raw_one(
        db,
        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
    )
}

fn table_sql(db: &Path) -> String {
    raw_one(
        db,
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'role_grants'",
    )
}

#[test]
fn a_version_5_database_gains_the_role_grant_foreign_keys_and_keeps_every_grant() {
    let tmp = temp("migrate");
    let db = tmp.0.join("store.sqlite");
    let (alice, bob) = (AccountId::new(), AccountId::new());
    let (w1, w2) = (WikiId::new().to_string(), WikiId::new().to_string());
    make_version_5(
        &db,
        &[&alice, &bob],
        &[
            // The operator's grant (no grantor), then one an account made, in two wikis.
            (&w1, alice.as_str(), None, 10),
            (&w1, bob.as_str(), Some(alice.as_str()), 11),
            (&w2, bob.as_str(), None, 12),
        ],
    );
    assert_eq!(version(&db), "5");
    assert!(!table_sql(&db).contains("REFERENCES"));

    let store = SqliteStore::open(&db).expect("a version 5 database opens");
    assert_eq!(version(&db), SCHEMA_VERSION.to_string());
    assert_eq!(SCHEMA_VERSION, 8);
    let w1 = WikiId::parse(&w1).expect("id");
    let grants = store.grants_in(&w1).expect("grants");
    assert_eq!(grants.len(), 2, "both grants of the first wiki survived");
    let [first, second] = grants.as_slice() else {
        panic!("{grants:?}");
    };
    assert_eq!(
        (&first.account, first.granted_by.as_ref(), first.granted_at),
        (&alice, None, 10),
        "the operator's grant keeps its NULL grantor"
    );
    assert_eq!(
        (
            &second.account,
            second.granted_by.as_ref(),
            second.granted_at
        ),
        (&bob, Some(&alice), 11)
    );
    assert_eq!(first.role, Role::Admin);
    assert_eq!(
        store
            .role(&bob, &WikiId::parse(&w2).expect("id"))
            .expect("role"),
        Some(Role::Admin)
    );

    // The table now carries both references and its index, and SQLite's own check finds nothing wrong.
    let sql = table_sql(&db);
    assert_eq!(sql.matches("REFERENCES accounts(id)").count(), 2, "{sql}");
    let index_sql: String = raw_one(
        &db,
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'role_grants_wiki'",
    );
    assert!(
        index_sql.contains("role_grants(wiki_id, seq)"),
        "{index_sql}"
    );
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    let mut check = raw
        .prepare("PRAGMA foreign_key_check(role_grants)")
        .expect("pragma");
    assert!(
        check
            .query([])
            .expect("rows")
            .next()
            .expect("next")
            .is_none()
    );

    // The new grant order continues after the copied ones, and a second open changes nothing.
    drop(store);
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.grants_in(&w1).expect("grants").len(), 2);
}

#[test]
fn a_version_5_database_with_a_grant_to_a_missing_account_is_refused_by_name_and_left_as_it_was() {
    for tag in ["missing-account", "missing-grantor"] {
        let tmp = temp(tag);
        let db = tmp.0.join("store.sqlite");
        let (alice, ghost) = (AccountId::new(), AccountId::new());
        let (w1, w2) = (WikiId::new().to_string(), WikiId::new().to_string());
        // Two grants; the second is the bad one: to an account that does not exist, or by one.
        let second = if tag == "missing-account" {
            (w2.as_str(), ghost.as_str(), None, 11)
        } else {
            (w2.as_str(), alice.as_str(), Some(ghost.as_str()), 11)
        };
        make_version_5(
            &db,
            &[&alice],
            &[(w1.as_str(), alice.as_str(), None, 10), second],
        );

        let err = SqliteStore::open(&db).expect_err("refused");
        let StoreError::MigrationRefused { version: v, why } = &err else {
            panic!("{err:?}");
        };
        assert_eq!(*v, 6);
        assert!(why.contains("1 role grant(s)"), "{why}");
        assert!(
            why.contains(ghost.as_str()) || why.contains(alice.as_str()),
            "{why}"
        );
        assert!(
            err.to_string().contains("schema version 6")
                && err.to_string().contains("nothing was changed"),
            "{err}"
        );
        // Nothing changed: still version 5, the old table without references, both rows there.
        assert_eq!(version(&db), "5", "{tag}");
        assert!(!table_sql(&db).contains("REFERENCES"), "{tag}");
        let rows: i64 = raw_one(&db, "SELECT COUNT(*) FROM role_grants");
        assert_eq!(rows, 2, "{tag}");
        let leftovers: i64 = raw_one(
            &db,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'role_grants_v6'",
        );
        assert_eq!(leftovers, 0, "the half-built table was rolled back");

        // Repair it as the message says (remove the grants), and it migrates.
        rusqlite::Connection::open(&db)
            .expect("raw open")
            .execute("DELETE FROM role_grants WHERE seq = 2", [])
            .expect("repair");
        SqliteStore::open(&db).expect("the repaired database migrates");
        assert_eq!(version(&db), SCHEMA_VERSION.to_string());
    }
}

#[test]
fn a_fresh_database_has_the_foreign_keys_and_the_connection_enforces_them() {
    let tmp = temp("fresh");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    assert_eq!(table_sql(&db).matches("REFERENCES accounts(id)").count(), 2);

    let wiki = store
        .create(NewWiki::new(
            Slug::new("granted").expect("slug"),
            DisplayName::new("Granted").expect("name"),
        ))
        .expect("wiki");
    let (real, grantor) = (external_account(), external_account());
    store.add_account(&real).expect("account");
    store.add_account(&grantor).expect("account");
    let grant = |account: &AccountId, by: Option<&AccountId>| RoleGrant {
        account: account.clone(),
        wiki: wiki.id.clone(),
        role: Role::Contributor,
        granted_by: by.cloned(),
        granted_at: 5,
    };
    // Nothing in the store looks for the account before it inserts, so this refusal is SQLite's own, seen
    // through the public API.
    let nobody = AccountId::new();
    assert_eq!(
        store.grant(grant(&nobody, None)).err(),
        Some(StoreError::AccountNotFound),
        "a grant to an account that does not exist"
    );
    assert_eq!(
        store.grant(grant(&real.id, Some(&nobody))).err(),
        Some(StoreError::AccountNotFound),
        "a grant by an account that does not exist"
    );
    assert_eq!(store.role(&nobody, &wiki.id).expect("role"), None);
    assert!(store.grants_in(&wiki.id).expect("grants").is_empty());

    // The legitimate values keep working: the operator (no grantor) and a real account as grantor.
    store.grant(grant(&real.id, None)).expect("operator grant");
    store
        .grant(grant(&grantor.id, Some(&real.id)))
        .expect("grant by an account");
    assert_eq!(store.grants_in(&wiki.id).expect("grants").len(), 2);

    // And a connection that bypasses the store's code is refused by the database: with the pragma on, as
    // the store sets it.
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("pragma");
    let refused = raw
        .execute(
            "INSERT INTO role_grants (wiki_id, account_id, role, granted_at, seq) \
             VALUES ('w', 'nobody', 'admin', 1, 99)",
            [],
        )
        .expect_err("no such account");
    assert!(refused.to_string().contains("FOREIGN KEY"), "{refused}");
}

#[test]
fn an_account_cannot_be_deleted_from_under_its_grants() {
    // The other direction of the reference: with the keys on, the account row cannot go while a grant
    // names it (nothing deletes accounts today; this is what makes a future deletion decide about grants).
    let tmp = temp("parent");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let wiki = store
        .create(NewWiki::new(
            Slug::new("held").expect("slug"),
            DisplayName::new("Held").expect("name"),
        ))
        .expect("wiki");
    let holder = external_account();
    store.add_account(&holder).expect("account");
    store
        .grant(RoleGrant {
            account: holder.id.clone(),
            wiki: wiki.id.clone(),
            role: Role::Observer,
            granted_by: None,
            granted_at: 1,
        })
        .expect("grant");
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("pragma");
    let refused = raw
        .execute("DELETE FROM accounts WHERE id = ?1", [holder.id.as_str()])
        .expect_err("held by a grant");
    assert!(refused.to_string().contains("FOREIGN KEY"), "{refused}");
}
