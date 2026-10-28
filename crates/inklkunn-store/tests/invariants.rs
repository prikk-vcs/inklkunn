//! The same invariants on both stores (handoff 001 T4), plus reopen persistence, the schema-version
//! refusal and the stored-slug rule for SQLite.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    AccountId, AuditEvent, AuditLog, AuditOutcome, AuditSink, DisplayName, MemoryAudit,
    MemoryStore, NewWiki, Role, RoleGrant, RoleStore, RootLayout, Slug, SqliteStore, StoreError,
    Verification, VerificationState, Visibility, WikiId, WikiRecord, WikiStore, export_jsonl,
};

fn slug(s: &str) -> Slug {
    Slug::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn name(s: &str) -> DisplayName {
    DisplayName::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn invariants(store: &dyn WikiStore) {
    let a = store
        .create(NewWiki::new(slug("alpha"), name("Alpha")))
        .expect("create");
    assert_eq!(a.slug.as_str(), "alpha");
    assert_eq!(a.default_view, "heads/main");
    assert_eq!(a.verification.state, VerificationState::Unknown);

    // Slugs are unique, and an insert says exactly what was taken.
    assert_eq!(
        store
            .create(NewWiki::new(slug("alpha"), name("Again")))
            .err(),
        Some(StoreError::SlugTaken("alpha".into()))
    );
    let dup = WikiRecord::from_new(a.id.clone(), NewWiki::new(slug("other"), name("Dup")), 2)
        .expect("record");
    assert_eq!(
        store.insert(dup).err(),
        Some(StoreError::IdTaken(a.id.to_string()))
    );
    assert_eq!(
        store.list().expect("list").len(),
        1,
        "nothing half-inserted"
    );

    let mut b_new = NewWiki::new(slug("beta"), name("Beta"));
    b_new.visibility = Visibility::Restricted;
    b_new.languages = vec!["ja".into(), "en".into()];
    let b = store.create(b_new).expect("create b");
    assert_eq!(b.visibility, Visibility::Restricted);
    assert_eq!(b.languages, vec!["ja", "en"]);

    // A request with a bad free-text field is refused before anything is stored.
    let mut bad = NewWiki::new(slug("gamma"), name("Gamma"));
    bad.default_view = Some("main".into());
    assert!(matches!(
        store.create(bad),
        Err(StoreError::InvalidRequest(_))
    ));
    assert!(store.by_slug(&slug("gamma")).is_err());

    // Open by id and by slug agree.
    assert_eq!(store.get(&a.id).expect("get"), a);
    assert_eq!(store.by_slug(&slug("beta")).expect("by slug"), b);
    assert_eq!(store.get(&WikiId::new()).err(), Some(StoreError::NotFound));
    assert_eq!(
        store.by_slug(&slug("nope")).err(),
        Some(StoreError::NotFound)
    );

    // List is in creation order.
    let ids: Vec<_> = store
        .list()
        .expect("list")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec![a.id.clone(), b.id.clone()]);

    // Rename changes the slug and the name, never the id; a taken slug is refused.
    let renamed = store
        .rename(&a.id, Some(slug("alpha-two")), Some(name("Alpha II")))
        .expect("rename");
    assert_eq!(renamed.id, a.id);
    assert_eq!(renamed.slug.as_str(), "alpha-two");
    assert_eq!(renamed.display_name.as_str(), "Alpha II");
    assert_eq!(
        store.by_slug(&slug("alpha")).err(),
        Some(StoreError::NotFound)
    );
    assert_eq!(
        store.rename(&a.id, Some(slug("beta")), None).err(),
        Some(StoreError::SlugTaken("beta".into()))
    );
    assert_eq!(
        store
            .rename(&a.id, Some(slug("alpha-two")), None)
            .expect("same slug is fine")
            .slug
            .as_str(),
        "alpha-two"
    );
    assert_eq!(
        store.rename(&WikiId::new(), None, Some(name("x"))).err(),
        Some(StoreError::NotFound)
    );

    // Verification and keys.
    let v = store
        .set_verification(
            &b.id,
            Verification {
                state: VerificationState::Verified,
                at: Some(1_700_000_000),
            },
        )
        .expect("verification");
    assert_eq!(v.verification.at, Some(1_700_000_000));
    let k = store
        .set_keys(
            &b.id,
            Some("ed25519-aaaa".into()),
            Some("ed25519-bbbb".into()),
            Some("prikk-format-3".into()),
        )
        .expect("keys");
    assert_eq!(k.author_key_id.as_deref(), Some("ed25519-aaaa"));
    assert_eq!(
        store
            .get(&b.id)
            .expect("get b")
            .prikk_format_version
            .as_deref(),
        Some("prikk-format-3")
    );

    // A complete record inserted as one lands with every field.
    let mut full = WikiRecord::from_new(WikiId::new(), NewWiki::new(slug("full"), name("Full")), 3)
        .expect("record");
    full.author_key_id = Some("ed25519-cccc".into());
    full.verification = Verification {
        state: VerificationState::Verified,
        at: Some(4),
    };
    let stored = store.insert(full.clone()).expect("insert");
    assert_eq!(stored, full);
    assert_eq!(store.get(&full.id).expect("get"), full);
}

/// Role grants (RFC 002 D-2): one role per account per wiki, replaced by a new grant, isolated by wiki,
/// refused for a wiki that is not there, in the order each was last made.
fn role_invariants(store: &(impl WikiStore + RoleStore)) {
    let w1 = store
        .create(NewWiki::new(slug("roles-one"), name("Roles one")))
        .expect("wiki one");
    let w2 = store
        .create(NewWiki::new(slug("roles-two"), name("Roles two")))
        .expect("wiki two");
    let (a, b, root) = (AccountId::new(), AccountId::new(), AccountId::new());
    let grant =
        |account: &AccountId, wiki: &WikiId, role: Role, by: Option<&AccountId>, at: i64| {
            RoleGrant {
                account: account.clone(),
                wiki: wiki.clone(),
                role,
                granted_by: by.cloned(),
                granted_at: at,
            }
        };

    // Nothing is granted until it is.
    assert_eq!(store.role(&a, &w1.id).expect("role"), None);
    assert!(store.grants_in(&w1.id).expect("grants").is_empty());

    store
        .grant(grant(&a, &w1.id, Role::Contributor, None, 10))
        .expect("grant a");
    store
        .grant(grant(&b, &w1.id, Role::Admin, Some(&root), 11))
        .expect("grant b");
    assert_eq!(
        store.role(&a, &w1.id).expect("role"),
        Some(Role::Contributor)
    );
    assert_eq!(store.role(&b, &w1.id).expect("role"), Some(Role::Admin));

    // A role is per wiki: nothing leaks to the other wiki or to another account.
    assert_eq!(store.role(&a, &w2.id).expect("role"), None);
    assert_eq!(store.role(&root, &w1.id).expect("role"), None);

    // A new grant replaces the old one (a promotion is one act) and moves to the end of the order.
    store
        .grant(grant(&a, &w1.id, Role::Maintainer, Some(&b), 12))
        .expect("promote");
    assert_eq!(
        store.role(&a, &w1.id).expect("role"),
        Some(Role::Maintainer)
    );
    let all = store.grants_in(&w1.id).expect("grants");
    assert_eq!(all.len(), 2, "one role per account per wiki");
    assert_eq!(
        all.iter().map(|g| &g.account).collect::<Vec<_>>(),
        vec![&b, &a]
    );
    let promoted = all.last().expect("promoted grant");
    assert_eq!(promoted.granted_by.as_ref(), Some(&b));
    assert_eq!(promoted.granted_at, 12);
    assert_eq!(all.first().expect("first").granted_by.as_ref(), Some(&root));

    // A grant for a wiki that is not there is refused, and leaves nothing behind.
    let ghost = WikiId::new();
    assert_eq!(
        store.grant(grant(&a, &ghost, Role::Admin, None, 13)).err(),
        Some(StoreError::NotFound)
    );
    assert_eq!(store.role(&a, &ghost).expect("role"), None);
    assert!(store.grants_in(&ghost).expect("grants").is_empty());

    // Revoking says whether there was anything to revoke, and only that account's role in that wiki goes.
    store
        .grant(grant(&a, &w2.id, Role::Observer, None, 14))
        .expect("grant a in two");
    assert!(store.revoke(&a, &w1.id).expect("revoke"));
    assert!(!store.revoke(&a, &w1.id).expect("revoke again"));
    assert_eq!(store.role(&a, &w1.id).expect("role"), None);
    assert_eq!(store.role(&a, &w2.id).expect("role"), Some(Role::Observer));
    assert_eq!(store.role(&b, &w1.id).expect("role"), Some(Role::Admin));
    assert!(
        !store
            .revoke(&AccountId::new(), &w1.id)
            .expect("unknown account")
    );
}

#[test]
fn memory_store_holds_the_invariants() {
    invariants(&MemoryStore::new());
    role_invariants(&MemoryStore::new());
}

#[test]
fn sqlite_store_holds_the_invariants() {
    invariants(&SqliteStore::open_in_memory().expect("open"));
    role_invariants(&SqliteStore::open_in_memory().expect("open"));
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-store-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

#[test]
fn sqlite_store_persists_across_reopen() {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let id = {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("persist"), name("Persist")))
            .expect("create")
            .id
    };
    let store = SqliteStore::open(&db).expect("reopen");
    let r = store.get(&id).expect("still there");
    assert_eq!(r.slug.as_str(), "persist");
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn sqlite_store_refuses_a_newer_or_unreadable_schema_before_touching_it() {
    let tmp = temp("schema");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("one"), name("One")))
            .expect("create");
    }
    let doctor = |value: &str| {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.execute(
            "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
            [value],
        )
        .expect("doctor");
    };
    doctor("99");
    assert_eq!(
        SqliteStore::open(&db).err(),
        Some(StoreError::SchemaVersion {
            found: "99".into(),
            known: inklkunn_store::sqlite::SCHEMA_VERSION,
        })
    );
    doctor("garbage");
    assert!(matches!(
        SqliteStore::open(&db).err(),
        Some(StoreError::SchemaVersion { .. })
    ));
    // The refusal changed nothing: restore the version and the wiki is still there.
    doctor(&inklkunn_store::sqlite::SCHEMA_VERSION.to_string());
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.list().expect("list").len(), 1);
}

#[test]
fn a_stored_slug_later_reserved_stays_readable() {
    let tmp = temp("reserved");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        store
            .create(NewWiki::new(slug("keep"), name("Keep")))
            .expect("create");
    }
    {
        // A wiki that took a name before the name was reserved.
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.execute("UPDATE wikis SET slug = 'api' WHERE slug = 'keep'", [])
            .expect("doctor");
    }
    let store = SqliteStore::open(&db).expect("reopen");
    let all = store.list().expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all.first().map(|r| r.slug.as_str()), Some("api"));
    // But no new wiki can take it, and no rename can.
    assert!(Slug::new("api").is_err());
}

#[test]
fn the_root_layout_puts_a_wiki_under_its_shard_renames_move_nothing_and_discard_removes_both() {
    let tmp = temp("root");
    let layout = RootLayout::new(&tmp.0);
    let store = MemoryStore::new();
    let w = store
        .create(NewWiki::new(slug("laid-out"), name("Laid out")))
        .expect("create");
    let repo = layout.prepare(&w.id).expect("prepare");
    assert!(repo.is_dir());
    assert!(layout.scratch_dir(&w.id).is_dir());
    assert!(repo.starts_with(tmp.0.join(w.id.shard())));
    assert!(!repo.to_string_lossy().contains("laid-out"));
    let renamed = store
        .rename(&w.id, Some(slug("other")), None)
        .expect("rename");
    assert_eq!(
        layout.repo_dir(&renamed.id),
        repo,
        "the directory is the id's, not the slug's"
    );
    std::fs::write(repo.join("file"), b"x").expect("write");
    layout.discard_unfinished(&w.id).expect("discard");
    assert!(!repo.exists());
    assert!(!layout.scratch_dir(&w.id).exists());
    // Discarding what is already gone is not an error.
    layout.discard_unfinished(&w.id).expect("idempotent");
}

/// M0's schema, written out here on purpose: an oracle for "a database an M0 build made" that does not move
/// when the code under test does.
const M0_SCHEMA: &str = "
CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE wikis (
    id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, display_name TEXT NOT NULL,
    default_view TEXT NOT NULL, default_license TEXT NOT NULL, languages TEXT NOT NULL,
    visibility TEXT NOT NULL, policy_bootstrap TEXT NOT NULL, prikk_format_version TEXT,
    verification_state TEXT NOT NULL, verification_at INTEGER, author_key_id TEXT,
    maintainer_key_id TEXT, created_at INTEGER NOT NULL, seq INTEGER NOT NULL
);
CREATE INDEX wikis_seq ON wikis(seq);
INSERT INTO schema_meta (key, value) VALUES ('schema_version', '1');
INSERT INTO wikis VALUES ('019a0000-0000-7000-8000-000000000001', 'old-wiki', 'Old wiki', 'heads/main',
    'CC-BY-SA-4.0', '[\"en\"]', 'public', 'default', NULL, 'verified', 1700000000, 'ed25519-aaaa',
    'ed25519-bbbb', 1700000000, 1);
";

#[test]
fn an_m0_database_is_migrated_forward_with_its_wikis_and_only_once() {
    let tmp = temp("migrate");
    let db = tmp.0.join("store.sqlite");
    {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.execute_batch(M0_SCHEMA).expect("the M0 schema");
    }
    let version = || -> String {
        let conn = rusqlite::Connection::open(&db).expect("raw open");
        conn.query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version")
    };
    assert_eq!(version(), "1");
    {
        let store = SqliteStore::open(&db).expect("an M0 database opens");
        let wikis = store.list().expect("list");
        assert_eq!(wikis.len(), 1, "the M0 wiki survived");
        let old = wikis.first().expect("wiki");
        assert_eq!(old.slug.as_str(), "old-wiki");
        assert_eq!(old.author_key_id.as_deref(), Some("ed25519-aaaa"));
        // The new tables are there and work on the old wiki.
        let account = AccountId::new();
        store
            .grant(RoleGrant {
                account: account.clone(),
                wiki: old.id.clone(),
                role: Role::Admin,
                granted_by: None,
                granted_at: 5,
            })
            .expect("grant in a migrated database");
        assert_eq!(
            store.role(&account, &old.id).expect("role"),
            Some(Role::Admin)
        );
        // Version 5's additions work on the old wiki too: it has no checkpoint history, and the derived
        // cache takes a title index for it.
        assert_eq!(old.checkpoint, inklkunn_store::Checkpoint::default());
        let tip = "a".repeat(64);
        inklkunn_store::DerivedStore::put_title_index(&store, &old.id, &tip, &[], 1)
            .expect("the derived tables exist");
        assert_eq!(
            inklkunn_store::DerivedStore::title_index(&store, &old.id, &tip).expect("index"),
            Some(Vec::new())
        );
        assert!(
            store
                .request_checkpoint_refresh(&old.id, 7)
                .expect("the checkpoint columns exist")
                .checkpoint
                .refresh_requested_at
                .is_some()
        );
    }
    assert_eq!(
        version(),
        inklkunn_store::sqlite::SCHEMA_VERSION.to_string()
    );
    // Opening it again changes nothing and loses nothing.
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.list().expect("list").len(), 1);
    assert_eq!(
        version(),
        inklkunn_store::sqlite::SCHEMA_VERSION.to_string()
    );
}

#[test]
fn the_migration_list_is_ordered_and_ends_at_the_schema_version() {
    use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
    let versions: Vec<i64> = MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert_eq!(versions.first(), Some(&1), "version 1 is M0's schema");
    assert!(
        versions
            .windows(2)
            .all(|w| matches!(w, [a, b] if *b == *a + 1)),
        "{versions:?}"
    );
    assert_eq!(versions.last(), Some(&SCHEMA_VERSION));
}

/// The audit trail (RFC 002 D-9): appended in order with positions that only grow, read back in pages,
/// kept to one clean line per field however the event was built, exported as JSON Lines.
fn audit_invariants(log: &dyn AuditLog) {
    assert!(log.read(0, 10).expect("read").is_empty());
    let mut empty = Vec::new();
    assert_eq!(export_jsonl(log, &mut empty).expect("export"), 0);
    assert!(empty.is_empty());

    for n in 1..=5 {
        log.record(&AuditEvent::new(
            1_700_000_000 + n,
            "account:a",
            "accept",
            &format!("proposal:w:p{n}"),
            if n % 2 == 0 {
                AuditOutcome::Allowed
            } else {
                AuditOutcome::Denied
            },
            "",
        ))
        .expect("record");
    }
    // An event built field by field (the fields are public) is cleaned by the sink.
    log.record(&AuditEvent {
        at: 1_700_000_100,
        principal: "account:b\nforged-line".into(),
        action: "export".into(),
        resource: format!("wiki:{}", "w".repeat(500)),
        outcome: AuditOutcome::Reported,
        detail: "two\u{2028}lines".into(),
    })
    .expect("record");

    let all = log.read(0, 100).expect("read");
    assert_eq!(all.len(), 6);
    assert_eq!(
        all.iter().map(|r| r.seq).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    let (Some(first_record), Some(last_record)) = (all.first(), all.last()) else {
        panic!("six records were read");
    };
    assert_eq!(first_record.event.resource, "proposal:w:p1");
    let last = &last_record.event;
    assert_eq!(last.principal, "account:b?forged-line");
    assert_eq!(last.detail, "two?lines");
    assert_eq!(last.resource.chars().count(), 200);
    assert_eq!(last.outcome, AuditOutcome::Reported);

    // Pages: after a position, at most a limit.
    let page = log.read(2, 2).expect("page");
    assert_eq!(page.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![3, 4]);
    assert!(log.read(6, 10).expect("end").is_empty());
    assert_eq!(log.read(0, 0).expect("zero").len(), 0);

    // JSON Lines: one object per line, fixed keys, the whole trail, oldest first.
    let mut out = Vec::new();
    assert_eq!(export_jsonl(log, &mut out).expect("export"), 6);
    let text = String::from_utf8(out).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 6);
    assert!(text.ends_with('\n'));
    let line = |n: usize| -> serde_json::Value {
        serde_json::from_str(lines.get(n).copied().expect("line")).expect("json")
    };
    let field = |v: &serde_json::Value, key: &str| v.get(key).cloned().expect("field");
    let first = line(0);
    assert_eq!(field(&first, "seq"), 1);
    assert_eq!(field(&first, "outcome"), "denied");
    assert_eq!(field(&first, "action"), "accept");
    assert_eq!(field(&first, "resource"), "proposal:w:p1");
    let keys: Vec<&str> = first
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "action",
            "at",
            "detail",
            "outcome",
            "principal",
            "resource",
            "seq"
        ]
    );
    assert_eq!(
        field(&line(5), "principal"),
        "account:b?forged-line",
        "no field can become a second line"
    );
}

#[test]
fn both_audit_sinks_hold_the_trail_invariants() {
    audit_invariants(&MemoryAudit::new());
    audit_invariants(&SqliteStore::open_in_memory().expect("open"));
}

#[test]
fn the_audit_trail_cannot_be_edited_or_erased_even_through_sql() {
    let tmp = temp("audit-append-only");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("open");
    for n in 0..3 {
        store
            .record(&AuditEvent::new(
                n,
                "anonymous",
                "export",
                "wiki:w",
                AuditOutcome::Denied,
                "",
            ))
            .expect("record");
    }
    // A second connection to the same file (a stray tool, a bug, an attacker with the file): the database
    // refuses the statements that would change the trail.
    let conn = rusqlite::Connection::open(&db).expect("raw open");
    for sql in [
        "UPDATE audit_log SET outcome = 'allowed'",
        "UPDATE audit_log SET detail = 'nothing happened' WHERE seq = 1",
        "DELETE FROM audit_log",
        "DELETE FROM audit_log WHERE seq = 2",
        // The replacing insert: it deletes the old row without firing a delete trigger, so it has its own.
        "INSERT OR REPLACE INTO audit_log (seq, at, principal, action, resource, outcome, detail) VALUES (2, 9, 'p', 'a', 'r', 'allowed', 'rewritten')",
        "REPLACE INTO audit_log (seq, at, principal, action, resource, outcome, detail) VALUES (1, 9, 'p', 'a', 'r', 'allowed', 'rewritten')",
    ] {
        let r = conn.execute(sql, []);
        let e = r.expect_err(sql).to_string();
        assert!(e.contains("append-only"), "{sql}: {e}");
    }
    let all = store.read(0, 10).expect("read");
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|r| r.event.outcome == AuditOutcome::Denied));
    assert!(
        all.iter().all(|r| r.event.detail.is_empty()),
        "no row was rewritten"
    );
    // An insert is what the log does, and the positions keep growing.
    conn.execute(
        "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) VALUES (9, 'p', 'a', 'r', 'denied', '')",
        [],
    )
    .expect("insert");
    assert_eq!(
        store.read(0, 10).expect("read").last().map(|r| r.seq),
        Some(4)
    );
}

#[test]
fn the_audit_trail_persists_across_reopen_and_unknown_outcomes_are_not_guessed() {
    let tmp = temp("audit-reopen");
    let db = tmp.0.join("store.sqlite");
    {
        let store = SqliteStore::open(&db).expect("open");
        store
            .record(&AuditEvent::new(
                1,
                "system:import",
                "import",
                "wiki:w",
                AuditOutcome::Allowed,
                "ok",
            ))
            .expect("record");
    }
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(store.read(0, 10).expect("read").len(), 1);
    // A row this build cannot read is an error, never a made-up event.
    let conn = rusqlite::Connection::open(&db).expect("raw open");
    conn.execute(
        "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) VALUES (2, 'p', 'a', 'r', 'maybe', '')",
        [],
    )
    .expect("insert");
    assert!(matches!(store.read(0, 10), Err(StoreError::Backend(_))));
}
