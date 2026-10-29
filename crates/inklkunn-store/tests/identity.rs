//! Accounts, external identities, sessions and tokens: the same invariants on both stores, then what only
//! SQLite can show (reopen, the schema's own refusals, the migration from version 3).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, ExternalIdentity, LocalCredential, LoginName,
    MemoryStore, Scope, SessionRecord, SessionStore, SqliteStore, StoreError, TokenId, TokenRecord,
    TokenStore,
};

fn login(s: &str) -> LoginName {
    LoginName::parse(s).expect("a valid login")
}

/// A well-formed Argon2id string (the store checks the shape, not the cryptography).
fn phc(tag: &str) -> String {
    format!(
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaC1mb3ItdGVzdHM{}",
        tag.len()
    )
}

fn account(name: &str, with_login: Option<&str>) -> Account {
    Account {
        id: AccountId::new(),
        display_name: DisplayName::new(name).expect("a display name"),
        local: with_login.map(|l| LocalCredential {
            login: login(l),
            password_hash: phc(l),
        }),
        created_at: 100,
        disabled: false,
    }
}

fn session(hash: &str, account: &AccountId, expires_at: i64) -> SessionRecord {
    SessionRecord {
        id_hash: hash.into(),
        account: account.clone(),
        csrf: format!("csrf-{hash}"),
        created_at: 100,
        expires_at,
    }
}

fn token(hash: &str, account: &AccountId, scope: Scope) -> TokenRecord {
    TokenRecord {
        id: TokenId::new(),
        hash: hash.into(),
        account: account.clone(),
        scope,
        label: "build\nbot".into(),
        created_at: 100,
        expires_at: Some(10_000),
        revoked_at: None,
    }
}

fn identity(issuer: &str, subject: &str) -> ExternalIdentity {
    ExternalIdentity::new(issuer, subject).expect("an identity")
}

fn account_invariants(store: &(impl AccountStore + SessionStore + TokenStore)) {
    // Create and find, by id and by (folded) sign-in name.
    let alice = account("Alice", Some("alice"));
    store.add_account(&alice).expect("create");
    assert_eq!(store.account(&alice.id).expect("get"), Some(alice.clone()));
    assert_eq!(
        store.account_by_login(&login("ALICE")).expect("by login"),
        Some(alice.clone())
    );
    assert_eq!(
        store.account_by_login(&login("nobody")).expect("by login"),
        None
    );
    assert_eq!(store.account(&AccountId::new()).expect("get"), None);

    // The id and the sign-in name are unique, and a refusal stores nothing.
    assert_eq!(
        store.add_account(&alice).err(),
        Some(StoreError::IdTaken(alice.id.to_string()))
    );
    let rival = account("Rival", Some("Alice"));
    assert_eq!(
        store.add_account(&rival).err(),
        Some(StoreError::LoginTaken("alice".into()))
    );
    assert_eq!(store.account(&rival.id).expect("get"), None);

    // An account with no password is an account (single sign-on only), and has no sign-in name to find.
    let sso = account("Sso only", None);
    store.add_account(&sso).expect("create without a password");
    assert_eq!(
        store.account(&sso.id).expect("get").expect("there").local,
        None
    );

    // External identities: linked once, idempotent for the same account, refused for another, and the
    // pair (issuer, subject) is the whole identity.
    let who = identity("https://idp.example", "user-42");
    store.link_identity(&sso.id, &who, 200).expect("link");
    store
        .link_identity(&sso.id, &who, 300)
        .expect("linking again is not an error");
    assert_eq!(
        store.link_identity(&alice.id, &who, 300).err(),
        Some(StoreError::IdentityTaken)
    );
    store
        .link_identity(
            &alice.id,
            &identity("https://other.example", "user-42"),
            300,
        )
        .expect("the same subject at another issuer is another identity");
    assert_eq!(
        store
            .link_identity(&AccountId::new(), &identity("https://x", "y"), 1)
            .err(),
        Some(StoreError::AccountNotFound)
    );
    assert_eq!(
        store.account_by_identity(&who).expect("find"),
        Some(sso.clone())
    );
    assert_eq!(
        store
            .account_by_identity(&identity("https://idp.example", "user-43"))
            .expect("find"),
        None
    );

    // Changing the password: replaced, removed, refused when another account has the name, refused for a
    // missing account. Keeping one's own name is not a clash.
    let renewed = LocalCredential {
        login: login("alice"),
        password_hash: phc("renewed"),
    };
    store
        .set_local_credential(&alice.id, Some(&renewed))
        .expect("replace");
    assert_eq!(
        store.account(&alice.id).expect("get").expect("there").local,
        Some(renewed.clone())
    );
    let clash = LocalCredential {
        login: login("alice"),
        password_hash: phc("clash"),
    };
    assert_eq!(
        store.set_local_credential(&sso.id, Some(&clash)).err(),
        Some(StoreError::LoginTaken("alice".into()))
    );
    store.set_local_credential(&alice.id, None).expect("remove");
    assert_eq!(store.account_by_login(&login("alice")).expect("find"), None);
    store
        .set_local_credential(&sso.id, Some(&clash))
        .expect("the name is free now");
    assert_eq!(
        store.set_local_credential(&AccountId::new(), None).err(),
        Some(StoreError::AccountNotFound)
    );

    // Disabling.
    store.set_disabled(&sso.id, true).expect("disable");
    assert!(
        store
            .account(&sso.id)
            .expect("get")
            .expect("there")
            .disabled
    );
    store.set_disabled(&sso.id, false).expect("enable");
    assert!(
        !store
            .account(&sso.id)
            .expect("get")
            .expect("there")
            .disabled
    );
    assert_eq!(
        store.set_disabled(&AccountId::new(), true).err(),
        Some(StoreError::AccountNotFound)
    );

    // A hash that is not an Argon2id string is refused, so no row can hold a "password" nobody can verify; the
    // account is looked for first, then the hash, then the name — in both stores.
    let mut bad = account("Bad hash", Some("badhash"));
    bad.local.as_mut().unwrap().password_hash = "!".into();
    assert_eq!(
        store.add_account(&bad).err(),
        Some(StoreError::InvalidCredential)
    );
    assert_eq!(
        store.account(&bad.id).expect("get"),
        None,
        "nothing was stored"
    );
    let junk = LocalCredential {
        login: login("junk"),
        password_hash: "$bcrypt$12$abc".into(),
    };
    assert_eq!(
        store.set_local_credential(&sso.id, Some(&junk)).err(),
        Some(StoreError::InvalidCredential)
    );
    let taken_name = LocalCredential {
        login: login("alice"),
        password_hash: phc("t"),
    };
    assert_eq!(
        store
            .set_local_credential(&AccountId::new(), Some(&taken_name))
            .err(),
        Some(StoreError::AccountNotFound),
        "a missing account is reported before a taken name"
    );
    assert_eq!(
        store
            .set_local_credential(&AccountId::new(), Some(&junk))
            .err(),
        Some(StoreError::AccountNotFound)
    );

    session_invariants(store, &alice.id, &sso.id);
    token_invariants(store, &alice.id, &sso.id);
}

fn session_invariants(store: &(impl SessionStore + ?Sized), a: &AccountId, b: &AccountId) {
    store
        .add_session(&session("sha256$s1", a, 1_000))
        .expect("create");
    store
        .add_session(&session("sha256$s2", a, 2_000))
        .expect("create");
    store
        .add_session(&session("sha256$s3", b, 3_000))
        .expect("create");
    assert_eq!(
        store.add_session(&session("sha256$s1", b, 9)).err(),
        Some(StoreError::IdTaken("session".into())),
        "a hash is used once"
    );
    assert_eq!(
        store
            .add_session(&session("sha256$s9", &AccountId::new(), 9))
            .err(),
        Some(StoreError::AccountNotFound)
    );
    let got = store.session("sha256$s1", 999).expect("get").expect("live");
    assert_eq!(
        (got.account.clone(), got.csrf.as_str()),
        (a.clone(), "csrf-sha256$s1")
    );
    assert_eq!(
        store.session("sha256$s1", 1_000).expect("get"),
        None,
        "over at its end instant"
    );
    assert_eq!(store.session("sha256$unknown", 0).expect("get"), None);

    assert_eq!(store.purge_expired_sessions(1_500).expect("purge"), 1);
    assert_eq!(store.session("sha256$s1", 0).expect("get"), None, "purged");
    assert!(store.end_session("sha256$s2").expect("delete"));
    assert!(
        !store.end_session("sha256$s2").expect("delete"),
        "a second delete finds nothing"
    );
    store
        .add_session(&session("sha256$s4", a, 5_000))
        .expect("create");
    store
        .add_session(&session("sha256$s5", a, 5_000))
        .expect("create");
    assert_eq!(store.end_sessions_of(a).expect("delete all"), 2);
    assert!(
        store.session("sha256$s3", 0).expect("get").is_some(),
        "another account's session stays"
    );
    assert_eq!(store.end_sessions_of(a).expect("none left"), 0);
    store.end_session("sha256$s3").expect("clean up");

    // Keeping the newest: five live (added in this order, ending at different times), one over, and one
    // of another account. Newest is the last added, whatever the stamps say.
    for (hash, ends, stamp) in [
        ("sha256$k1", 9_000, 50),
        ("sha256$k2", 9_000, 40),
        ("sha256$k3", 9_000, 30),
        ("sha256$k4", 100, 20),
        ("sha256$k5", 9_000, 10),
        ("sha256$k6", 9_000, 5),
    ] {
        let mut record = session(hash, a, ends);
        record.created_at = stamp;
        store.add_session(&record).expect("create");
    }
    store
        .add_session(&session("sha256$other", b, 9_000))
        .expect("create");
    // At 500: k1 k2 k3 k5 k6 are live (k4 is over). Keep three: k3 k5 k6; k1, k2 and k4 end.
    assert_eq!(store.keep_newest_sessions(a, 3, 500).expect("keep"), 3);
    for (hash, kept) in [
        ("sha256$k1", false),
        ("sha256$k2", false),
        ("sha256$k3", true),
        ("sha256$k4", false),
        ("sha256$k5", true),
        ("sha256$k6", true),
        ("sha256$other", true),
    ] {
        assert_eq!(
            store.session(hash, 0).expect("get").is_some(),
            kept,
            "{hash}"
        );
    }
    // Nothing to end; and keeping none ends every one of the account's, not the other's.
    assert_eq!(store.keep_newest_sessions(a, 3, 500).expect("keep"), 0);
    assert_eq!(store.keep_newest_sessions(a, 0, 500).expect("keep"), 3);
    assert!(store.session("sha256$other", 0).expect("get").is_some());
    store.end_session("sha256$other").expect("clean up");
}

fn token_invariants(store: &(impl TokenStore + ?Sized), a: &AccountId, b: &AccountId) {
    let first = token("sha256$t1", a, Scope::Propose);
    let second = token("sha256$t2", a, Scope::Admin);
    store.add_token(&first).expect("create");
    store.add_token(&second).expect("create");
    store
        .add_token(&token("sha256$t3", b, Scope::Read))
        .expect("create");

    let found = store
        .token_by_hash("sha256$t1")
        .expect("find")
        .expect("there");
    assert_eq!(
        (found.id.clone(), found.scope, found.account.clone()),
        (first.id.clone(), Scope::Propose, a.clone())
    );
    assert_eq!(found.label, "build?bot", "a label is one clean line");
    assert_eq!(found.expires_at, Some(10_000));
    assert_eq!(store.token_by_hash("sha256$none").expect("find"), None);
    assert_eq!(
        store
            .tokens_of(a)
            .expect("list")
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>(),
        vec![first.id.clone(), second.id.clone()],
        "oldest first"
    );
    assert_eq!(
        store.add_token(&token("sha256$t1", b, Scope::Read)).err(),
        Some(StoreError::IdTaken("token".into())),
        "a hash is used once"
    );
    let mut same_id = token("sha256$t9", a, Scope::Read);
    same_id.id = first.id.clone();
    assert_eq!(
        store.add_token(&same_id).err(),
        Some(StoreError::IdTaken(first.id.to_string()))
    );
    assert_eq!(
        store
            .add_token(&token("sha256$t8", &AccountId::new(), Scope::Read))
            .err(),
        Some(StoreError::AccountNotFound)
    );

    // Revoking: by its owner, once; another account's guess touches nothing.
    assert!(
        !store.revoke_token(b, &first.id, 500).expect("revoke"),
        "not b's token"
    );
    assert_eq!(
        store
            .token_by_hash("sha256$t1")
            .expect("find")
            .expect("there")
            .revoked_at,
        None
    );
    assert!(store.revoke_token(a, &first.id, 500).expect("revoke"));
    assert!(!store.revoke_token(a, &first.id, 600).expect("revoke again"));
    assert_eq!(
        store
            .token_by_hash("sha256$t1")
            .expect("find")
            .expect("there")
            .revoked_at,
        Some(500),
        "the first revocation time stays"
    );
    assert!(
        !store
            .revoke_token(a, &TokenId::new(), 500)
            .expect("revoke unknown")
    );
    // "Oldest first" is by creation time, whatever order they were stored in — in both stores.
    let mut late = token("sha256$o3", a, Scope::Read);
    late.created_at = 300;
    let mut early = token("sha256$o1", a, Scope::Read);
    early.created_at = 50;
    let mut middle = token("sha256$o2", a, Scope::Read);
    middle.created_at = 200;
    for t in [&late, &early, &middle] {
        store.add_token(t).expect("create");
    }
    let order: Vec<TokenId> = store
        .tokens_of(a)
        .expect("list")
        .into_iter()
        .map(|t| t.id)
        .collect();
    let pos = |id: &TokenId| order.iter().position(|x| x == id).expect("listed");
    assert!(
        pos(&early.id) < pos(&middle.id) && pos(&middle.id) < pos(&late.id),
        "{order:?}"
    );

    // Revoking all of an account's tokens: the live ones, once, and only that account's.
    let before_b = store
        .tokens_of(b)
        .expect("list")
        .iter()
        .filter(|t| t.revoked_at.is_none())
        .count();
    let live_a = store
        .tokens_of(a)
        .expect("list")
        .iter()
        .filter(|t| t.revoked_at.is_none())
        .count() as u64;
    assert!(live_a >= 4);
    assert_eq!(store.revoke_tokens_of(a, 900).expect("revoke all"), live_a);
    assert_eq!(
        store.revoke_tokens_of(a, 950).expect("again"),
        0,
        "nothing live is left"
    );
    assert!(
        store
            .tokens_of(a)
            .expect("list")
            .iter()
            .all(|t| t.revoked_at.is_some())
    );
    assert_eq!(
        store
            .tokens_of(b)
            .expect("list")
            .iter()
            .filter(|t| t.revoked_at.is_none())
            .count(),
        before_b,
        "another account's tokens stand"
    );
    // The first revocation time of an earlier-revoked token is kept.
    let kept = store
        .token_by_hash("sha256$t1")
        .expect("find")
        .expect("there");
    assert_eq!(kept.revoked_at, Some(500));
}

#[test]
fn memory_store_holds_the_identity_invariants() {
    account_invariants(&MemoryStore::new());
}

#[test]
fn sqlite_store_holds_the_identity_invariants() {
    account_invariants(&SqliteStore::open_in_memory().expect("open"));
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "inklkunn-identity-{tag}-{}-{}",
        std::process::id(),
        AccountId::new()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    TempDir(dir)
}

#[test]
fn accounts_sessions_and_tokens_persist_across_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let alice = account("Alice", Some("alice"));
    let first = token("sha256$t1", &alice.id, Scope::Review);
    {
        let store = SqliteStore::open(&db).expect("open");
        store.add_account(&alice).expect("create");
        store
            .link_identity(&alice.id, &identity("https://idp.example", "u1"), 1)
            .expect("link");
        store
            .add_session(&session("sha256$s1", &alice.id, 9_999))
            .expect("session");
        store.add_token(&first).expect("token");
    }
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.account_by_login(&login("alice")).expect("find"),
        Some(alice.clone())
    );
    assert_eq!(
        store
            .account_by_identity(&identity("https://idp.example", "u1"))
            .expect("find"),
        Some(alice.clone())
    );
    assert!(store.session("sha256$s1", 0).expect("get").is_some());
    let kept = store
        .token_by_hash("sha256$t1")
        .expect("find")
        .expect("there");
    assert_eq!(
        (kept.scope, kept.label.as_str()),
        (Scope::Review, "build?bot")
    );
}

#[test]
fn the_database_refuses_what_the_types_already_refuse() {
    let tmp = temp("checks");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    let alice = account("Alice", Some("alice"));
    store.add_account(&alice).expect("create");
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    raw.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("pragma");
    let refused = |sql: &str| raw.execute_batch(sql).expect_err(sql).to_string();
    // A scope that is not one of the five.
    let m = refused(&format!(
        "INSERT INTO api_tokens (id, hash, account_id, scope, label, created_at) \
         VALUES ('t', 'h', '{}', 'root', '', 1)",
        alice.id
    ));
    assert!(m.contains("CHECK"), "{m}");
    // A password with no sign-in name, and a name with no password.
    let m = refused(
        "INSERT INTO accounts (id, display_name, login, password_hash, created_at) VALUES ('x', 'X', NULL, 'h', 1)",
    );
    assert!(m.contains("CHECK"), "{m}");
    let m = refused(
        "INSERT INTO accounts (id, display_name, login, password_hash, created_at) VALUES ('y', 'Y', 'yy', NULL, 1)",
    );
    assert!(m.contains("CHECK"), "{m}");
    // A session for an account that is not there.
    let m = refused(
        "INSERT INTO sessions (id_hash, account_id, csrf, created_at, expires_at) VALUES ('h', 'nobody', 'c', 1, 2)",
    );
    assert!(m.contains("FOREIGN KEY"), "{m}");
}

#[test]
fn a_version_3_database_gains_the_identity_tables_and_keeps_its_audit_trail() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    use inklkunn_store::{AuditEvent, AuditLog, AuditOutcome, AuditSink};
    let tmp = temp("v3");
    let db = tmp.0.join("store.sqlite");
    {
        let raw = rusqlite::Connection::open(&db).expect("raw open");
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 3) {
            raw.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        raw.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', '3')",
            [],
        )
        .expect("version");
        raw.execute(
            "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) \
             VALUES (1, 'anonymous', 'export', 'wiki:w', 'denied', '')",
            [],
        )
        .expect("an event");
    }
    let store = SqliteStore::open(&db).expect("a version 3 database opens");
    assert_eq!(
        store.read(0, 10).expect("read").len(),
        1,
        "the audit trail survived"
    );
    store
        .record(&AuditEvent::new(
            2,
            "anonymous",
            "export",
            "wiki:w",
            AuditOutcome::Denied,
            "",
        ))
        .expect("record");
    let alice = account("Alice", Some("alice"));
    store.add_account(&alice).expect("the new tables work");
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    let version: String = raw
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION.to_string());
}
