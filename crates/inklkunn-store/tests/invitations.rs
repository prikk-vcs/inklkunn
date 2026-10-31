//! Invitations (the sign-in handoff's U2, schema version 14): the same invariants on both stores (single
//! use, expiry, all-or-none redemption, the role granted with the account), then what only SQLite can show
//! (the write-once triggers, the checks, two connections racing one link, the migration from every earlier
//! version).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::sqlite::{MIGRATIONS, SCHEMA_VERSION};
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, ExternalIdentity, InvitationId,
    InvitationRecord, InvitationStore, LocalCredential, LoginName, MemoryStore, Role, RoleStore,
    SqliteStore, StoreError, WikiId, WikiStore,
};
use support::{temp, wiki};

const MADE: i64 = 1_000;
const ENDS: i64 = 2_000;

fn phc(tag: &str) -> String {
    format!(
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaC1mb3ItdGVzdHM{}",
        tag.len()
    )
}

fn person(login: Option<&str>) -> Account {
    Account {
        id: AccountId::new(),
        display_name: DisplayName::new("Newcomer").expect("a name"),
        local: login.map(|l| LocalCredential {
            login: LoginName::parse(l).expect("a login"),
            password_hash: phc(l),
        }),
        created_at: MADE + 10,
        disabled: false,
    }
}

fn invitation(hash: &str, grant: Option<(WikiId, Role)>) -> InvitationRecord {
    InvitationRecord {
        id: InvitationId::new(),
        hash: format!("sha256${hash}"),
        created_at: MADE,
        expires_at: ENDS,
        grant,
        used: None,
    }
}

fn invariants(store: &(impl InvitationStore + AccountStore + RoleStore + WikiStore)) {
    let w = wiki(store, "garden");

    // Made, found by its hash, unused.
    let first = invitation("aaaa", Some((w.id.clone(), Role::Admin)));
    store.add_invitation(&first).expect("add");
    assert_eq!(
        store.invitation_by_hash(&first.hash).expect("find"),
        Some(first.clone())
    );
    assert_eq!(store.invitation_by_hash("sha256$none").expect("find"), None);
    assert!(first.usable_at(MADE) && first.usable_at(ENDS - 1) && !first.usable_at(ENDS));

    // A hash or an id used twice is refused, and a wiki that is not there is refused.
    let same_hash = invitation("aaaa", None);
    assert!(matches!(
        store.add_invitation(&same_hash),
        Err(StoreError::IdTaken(_))
    ));
    let mut same_id = invitation("bbbb", None);
    same_id.id = first.id.clone();
    assert!(matches!(
        store.add_invitation(&same_id),
        Err(StoreError::IdTaken(_))
    ));
    assert_eq!(
        store
            .add_invitation(&invitation("cccc", Some((WikiId::new(), Role::Reviewer))))
            .err(),
        Some(StoreError::NotFound)
    );
    // One that claims to be used is not a new invitation.
    let mut claimed = invitation("dddd", None);
    claimed.used = first.used.clone().or(Some(inklkunn_store::InvitationUse {
        account: AccountId::new(),
        at: 1,
    }));
    assert_eq!(
        store.add_invitation(&claimed).err(),
        Some(StoreError::AlreadyRecorded)
    );

    // Not before it is made? (The clock is the caller's; the store only compares with `expires_at`.)
    // A name that is taken refuses the redemption and leaves the link as it was.
    let existing = person(Some("taken"));
    store.add_account(&existing).expect("an account");
    assert_eq!(
        store
            .redeem_invitation(&first.hash, &person(Some("taken")), MADE + 10)
            .err(),
        Some(StoreError::LoginTaken("taken".into()))
    );
    // A hash that is not an Argon2id string likewise.
    let mut bad = person(Some("badhash"));
    if let Some(l) = bad.local.as_mut() {
        l.password_hash = "plaintext".into();
    }
    assert_eq!(
        store.redeem_invitation(&first.hash, &bad, MADE + 10).err(),
        Some(StoreError::InvalidCredential)
    );
    assert_eq!(
        store.invitation_by_hash(&first.hash).expect("find"),
        Some(first.clone()),
        "a refusal spends nothing"
    );
    assert_eq!(
        store
            .account_by_login(&LoginName::parse("badhash").unwrap())
            .unwrap(),
        None
    );

    // Over at its end, unknown, and expired are one answer.
    let late = person(Some("late"));
    assert_eq!(
        store.redeem_invitation(&first.hash, &late, ENDS).err(),
        Some(StoreError::InvitationUnusable)
    );
    assert_eq!(
        store
            .redeem_invitation("sha256$unknown", &late, MADE + 10)
            .err(),
        Some(StoreError::InvitationUnusable)
    );
    assert_eq!(
        store.account(&late.id).expect("get"),
        None,
        "nothing was made"
    );

    // Used: the account exists, holds the role (granted by no account), and the use is recorded.
    let alice = person(Some("alice"));
    let grant = store
        .redeem_invitation(&first.hash, &alice, MADE + 20)
        .expect("redeem")
        .expect("the invitation named a role");
    assert_eq!(
        (grant.role, grant.granted_by.as_ref(), &grant.wiki),
        (Role::Admin, None, &w.id)
    );
    assert_eq!(store.account(&alice.id).expect("get"), Some(alice.clone()));
    assert_eq!(
        store.role(&alice.id, &w.id).expect("role"),
        Some(Role::Admin)
    );
    let used = store
        .invitation_by_hash(&first.hash)
        .expect("find")
        .expect("there");
    assert_eq!(
        used.used,
        Some(inklkunn_store::InvitationUse {
            account: alice.id.clone(),
            at: MADE + 20
        })
    );
    assert!(!used.usable_at(MADE + 21));

    // Single use: the second person gets the same answer as for a link that never was, and nothing is made.
    let bob = person(Some("bob"));
    assert_eq!(
        store.redeem_invitation(&first.hash, &bob, MADE + 30).err(),
        Some(StoreError::InvitationUnusable)
    );
    assert_eq!(store.account(&bob.id).expect("get"), None);
    assert_eq!(store.role(&bob.id, &w.id).expect("role"), None);

    // An invitation with no role makes an account with none, and one with no password makes an external one.
    store
        .add_invitation(&invitation("eeee", None))
        .expect("add");
    let carol = person(None);
    assert_eq!(
        store
            .redeem_invitation("sha256$eeee", &carol, MADE + 40)
            .expect("redeem"),
        None
    );
    assert_eq!(store.account(&carol.id).expect("get"), Some(carol));
}

#[test]
fn invitations_obey_the_same_invariants_on_both_stores() {
    invariants(&MemoryStore::new());
    invariants(&SqliteStore::open_in_memory().expect("store"));
}

/// Redeeming with a provider identity: the account, its identity, the role and the use are one step.
fn redeem_with_identity(store: &(impl InvitationStore + AccountStore + RoleStore + WikiStore)) {
    let w = wiki(store, "garden");
    let idp = ExternalIdentity::new("https://idp.example", "subject-1").expect("identity");
    store
        .add_invitation(&invitation("ffff", Some((w.id.clone(), Role::Reviewer))))
        .expect("add");
    let dana = person(None);
    let grant = store
        .redeem_invitation_with_identity("sha256$ffff", &dana, &idp, MADE + 50)
        .expect("redeem");
    assert_eq!(grant.map(|g| g.role), Some(Role::Reviewer));
    assert_eq!(
        store.account_by_identity(&idp).expect("by"),
        Some(dana.clone())
    );
    assert_eq!(
        store.role(&dana.id, &w.id).expect("role"),
        Some(Role::Reviewer)
    );
    let used = store
        .invitation_by_hash("sha256$ffff")
        .expect("find")
        .expect("there");
    assert_eq!(used.used.map(|u| u.account), Some(dana.id.clone()));

    // An identity another account holds: nothing is spent, made or granted; the link works for someone else.
    store
        .add_invitation(&invitation("gggg", Some((w.id.clone(), Role::Admin))))
        .expect("add");
    let erin = person(None);
    assert_eq!(
        store
            .redeem_invitation_with_identity("sha256$gggg", &erin, &idp, MADE + 60)
            .err(),
        Some(StoreError::IdentityTaken)
    );
    assert_eq!(store.account(&erin.id).expect("get"), None);
    assert_eq!(store.role(&erin.id, &w.id).expect("role"), None);
    assert!(
        store
            .invitation_by_hash("sha256$gggg")
            .expect("find")
            .expect("there")
            .usable_at(MADE + 61)
    );

    // A link that is used, over or unknown is the one answer, before the identity is looked at.
    assert_eq!(
        store
            .redeem_invitation_with_identity("sha256$ffff", &erin, &idp, MADE + 70)
            .err(),
        Some(StoreError::InvitationUnusable)
    );
    let other = ExternalIdentity::new("https://idp.example", "subject-2").expect("identity");
    assert_eq!(
        store
            .redeem_invitation_with_identity("sha256$none", &erin, &other, MADE + 70)
            .err(),
        Some(StoreError::InvitationUnusable)
    );
    assert_eq!(store.account_by_identity(&other).expect("by"), None);
}

#[test]
fn redeeming_with_a_provider_identity_is_one_step_on_both_stores() {
    redeem_with_identity(&MemoryStore::new());
    redeem_with_identity(&SqliteStore::open_in_memory().expect("store"));
}

/// Two people open one link at the same moment: exactly one gets an account.
fn race(store: std::sync::Arc<impl InvitationStore + AccountStore + 'static>) {
    store
        .add_invitation(&invitation("race", None))
        .expect("add");
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .redeem_invitation("sha256$race", &person(Some(&format!("racer{n}"))), MADE + 5)
                    .is_ok()
            })
        })
        .collect();
    let winners = handles
        .into_iter()
        .map(|h| h.join().expect("thread"))
        .filter(|won| *won)
        .count();
    assert_eq!(winners, 1, "one link makes one account");
    let made = (0..8)
        .filter(|n| {
            store
                .account_by_login(&LoginName::parse(&format!("racer{n}")).unwrap())
                .unwrap()
                .is_some()
        })
        .count();
    assert_eq!(made, 1);
}

#[test]
fn eight_people_opening_one_link_make_one_account() {
    race(std::sync::Arc::new(MemoryStore::new()));
    let tmp = temp("invite-race");
    race(std::sync::Arc::new(
        SqliteStore::open(&tmp.0.join("store.sqlite")).expect("store"),
    ));
}

fn raw(db: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open(db).expect("raw open")
}

fn message(db: &std::path::Path, sql: &str) -> String {
    raw(db)
        .execute(sql, [])
        .expect_err("the database must refuse this")
        .to_string()
}

#[test]
fn the_database_itself_keeps_invitations_written_once() {
    let tmp = temp("invite-triggers");
    let db = tmp.0.join("store.sqlite");
    let store = SqliteStore::open(&db).expect("store");
    let w = wiki(&store, "garden");
    let inv = invitation("ffff", Some((w.id.clone(), Role::Maintainer)));
    store.add_invitation(&inv).expect("add");
    let alice = person(Some("alice"));
    store
        .redeem_invitation(&inv.hash, &alice, MADE + 1)
        .expect("redeem");

    for (what, sql) in [
        (
            "update",
            "UPDATE invitations SET expires_at = expires_at + 999",
        ),
        ("delete", "DELETE FROM invitations"),
        (
            "replace by id",
            &format!(
                "INSERT OR REPLACE INTO invitations (id, hash, created_at, expires_at) VALUES ('{}', 'sha256$other', 1, 2)",
                inv.id
            ),
        ),
        (
            "replace by hash",
            &format!(
                "INSERT OR REPLACE INTO invitations (id, hash, created_at, expires_at) VALUES ('{}', '{}', 1, 2)",
                InvitationId::new(),
                inv.hash
            ),
        ),
        ("use: update", "UPDATE invitation_uses SET at = 0"),
        ("use: delete", "DELETE FROM invitation_uses"),
        (
            "use: replace",
            &format!(
                "INSERT OR REPLACE INTO invitation_uses (invitation_id, account_id, at) VALUES ('{}', '{}', 1)",
                inv.id, alice.id
            ),
        ),
    ] {
        let m = message(&db, sql);
        assert!(m.contains("append-only"), "{what}: {m}");
    }

    // The checks: a wiki and a role come together; a role is one of the six; it ends after it is made.
    let m = message(
        &db,
        &format!(
            "INSERT INTO invitations (id, hash, created_at, expires_at, wiki_id, role) VALUES ('{}', 'sha256$x1', 1, 2, '{}', NULL)",
            InvitationId::new(),
            w.id
        ),
    );
    assert!(m.contains("CHECK"), "{m}");
    let m = message(
        &db,
        &format!(
            "INSERT INTO invitations (id, hash, created_at, expires_at, wiki_id, role) VALUES ('{}', 'sha256$x2', 1, 2, '{}', 'owner')",
            InvitationId::new(),
            w.id
        ),
    );
    assert!(m.contains("CHECK"), "{m}");
    let m = message(
        &db,
        &format!(
            "INSERT INTO invitations (id, hash, created_at, expires_at) VALUES ('{}', 'sha256$x3', 5, 5)",
            InvitationId::new()
        ),
    );
    assert!(m.contains("CHECK"), "{m}");
    // One account is made from at most one invitation.
    store
        .add_invitation(&invitation("gggg", None))
        .expect("another");
    let other = invitation_id_of(&store, "sha256$gggg");
    let m = message(
        &db,
        &format!(
            "INSERT INTO invitation_uses (invitation_id, account_id, at) VALUES ('{other}', '{}', 9)",
            alice.id
        ),
    );
    assert!(m.contains("UNIQUE") || m.contains("append-only"), "{m}");
}

fn invitation_id_of(store: &SqliteStore, hash: &str) -> InvitationId {
    store
        .invitation_by_hash(hash)
        .expect("find")
        .expect("there")
        .id
}

#[test]
fn a_database_from_before_invitations_gains_them_and_keeps_its_accounts() {
    let tmp = temp("invite-migrate");
    let db = tmp.0.join("store.sqlite");
    let latest_before = MIGRATIONS
        .iter()
        .map(|(v, _)| *v)
        .filter(|v| *v < 14)
        .max()
        .expect("earlier versions");
    let account = AccountId::new();
    {
        let conn = raw(&db);
        for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v < 14) {
            conn.execute_batch(sql)
                .unwrap_or_else(|e| panic!("v{version}: {e}"));
        }
        conn.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
            [latest_before.to_string()],
        )
        .expect("version");
        conn.execute(
            "INSERT INTO accounts (id, display_name, created_at) VALUES (?1, 'Old hand', 1)",
            [account.as_str()],
        )
        .expect("an account");
    }
    let store = SqliteStore::open(&db).expect("an older database opens");
    let version: String = raw(&db)
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION.to_string());
    assert!(store.account(&account).expect("get").is_some());
    // The new tables work.
    store
        .add_invitation(&invitation("hhhh", None))
        .expect("an invitation");
    let n: i64 = raw(&db)
        .query_row("SELECT COUNT(*) FROM invitations", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1);
}

#[test]
fn invitations_persist_across_reopen() {
    let tmp = temp("invite-reopen");
    let db = tmp.0.join("store.sqlite");
    let inv = invitation("iiii", None);
    {
        let store = SqliteStore::open(&db).expect("store");
        store.add_invitation(&inv).expect("add");
        store
            .redeem_invitation(&inv.hash, &person(Some("dora")), MADE + 1)
            .expect("redeem");
    }
    let store = SqliteStore::open(&db).expect("reopen");
    let found = store
        .invitation_by_hash(&inv.hash)
        .expect("find")
        .expect("there");
    assert!(found.used.is_some());
    assert_eq!(
        store
            .redeem_invitation(&inv.hash, &person(Some("eve")), MADE + 2)
            .err(),
        Some(StoreError::InvitationUnusable)
    );
}
