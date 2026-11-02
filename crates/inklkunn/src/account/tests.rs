#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use inklkunn_auth::{AuthConfig, Authenticator};
use inklkunn_core::IdentityAuthority;
use inklkunn_store::{
    AccountStore, AuditLog, DisplayName, ExternalIdentity, InvitationStore, NewWiki, Role,
    RoleStore, Slug, SqliteStore, WikiStore,
};

use super::*;

const NOW: i64 = 1_800_000_000;
const PASSWORD: &str = "a password that is long enough";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-account-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

/// A data directory with a store that holds one wiki, `garden`.
fn world(tag: &str) -> (Dir, SqliteStore, String) {
    let d = dir(tag);
    let store = SqliteStore::open(&d.0.join("store.sqlite")).unwrap();
    let wiki = store
        .create(NewWiki::new(
            Slug::new("garden").unwrap(),
            DisplayName::new("The Garden").unwrap(),
        ))
        .unwrap();
    (d, store, wiki.id.to_string())
}

fn cli(d: &Dir, args: &str, extra: &[(&str, &str)]) -> (i32, String, String) {
    let mut env: HashMap<String, OsString> = HashMap::new();
    env.insert("INKLKUNN_DATA_DIR".to_owned(), d.0.clone().into_os_string());
    for (k, v) in extra {
        env.insert((*k).to_owned(), OsString::from(v));
    }
    let args: Vec<String> = args.split_whitespace().map(str::to_owned).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run_with(
        &args,
        &|k| env.get(k).cloned(),
        NOW,
        &mut out,
        &mut err,
        || Some(IdentityAuthority::for_tests()),
    );
    (
        status,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// An authenticator over the store with a cheap hash, claiming nothing (a test may have many).
fn auth_over(store: &std::sync::Arc<SqliteStore>) -> Authenticator {
    Authenticator::new(
        store.clone(),
        AuthConfig::default(),
        IdentityAuthority::for_tests(),
    )
    .unwrap()
}

/// The invitation secret in the output of `invite`: the last path segment of the line that has the link.
fn secret_of(out: &str) -> String {
    out.lines()
        .find_map(|l| {
            l.rsplit_once("/-/invite/")
                .map(|(_, s)| s.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("no link in {out}"))
}

fn actions(store: &SqliteStore) -> Vec<(String, String, String, String)> {
    store
        .read(0, 1000)
        .unwrap()
        .into_iter()
        .map(|r| {
            (
                r.event.action.clone(),
                r.event.resource.clone(),
                r.event.outcome.as_str().to_owned(),
                r.event.principal.clone(),
            )
        })
        .collect()
}

#[test]
fn a_command_line_that_is_not_understood_is_a_usage_error_before_the_store_is_looked_at() {
    let d = dir("usage");
    for args in [
        "",
        "invite --role admin",
        "invite --wiki garden",
        "invite --role owner --wiki garden",
        "invite --days 0",
        "invite --days 31",
        "invite --days soon",
        "invite --frobnicate",
        "invite --role",
        "disable",
        "link alice",
        "link alice https://idp.example",
        "grant alice admin",
        "grant alice owner garden",
        "revoke-grant alice",
        "delete alice",
    ] {
        let (status, out, err) = cli(&d, args, &[]);
        assert_eq!((status, out.as_str()), (2, ""), "{args:?}: {err}");
        assert!(err.contains("usage:"), "{args:?}: {err}");
    }
}

#[test]
fn a_missing_store_is_named_and_not_made() {
    let d = dir("nostore");
    let (status, out, err) = cli(&d, "invite", &[]);
    assert_eq!((status, out.as_str()), (1, ""));
    assert!(err.contains("there is no store at"), "{err}");
    assert!(!d.0.join("store.sqlite").exists(), "a typo made a store");
    let (status, _, err) = run_without_data_dir();
    assert_eq!(status, 1);
    assert!(err.contains("INKLKUNN_DATA_DIR"), "{err}");
}

fn run_without_data_dir() -> (i32, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run_with(&["invite"], &|_| None, NOW, &mut out, &mut err, || {
        Some(IdentityAuthority::for_tests())
    });
    (
        status,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn an_invitation_is_made_shown_once_audited_and_opens_an_account() {
    let (d, store, _) = world("invite");
    let (status, out, err) = cli(
        &d,
        "invite",
        &[("INKLKUNN_APP_ORIGIN", "https://wiki.example/")],
    );
    assert_eq!(status, 0, "{err}");
    let secret = secret_of(&out);
    assert!(secret.starts_with("inv_"), "{out}");
    assert!(
        out.contains(&format!("https://wiki.example/-/invite/{secret}")),
        "the link is under the application origin: {out}"
    );
    assert!(out.contains("2027-01-22"), "ends in seven days: {out}");
    assert!(out.contains("only now"), "{out}");
    // Nothing else carries it: not the error stream, not the audit trail.
    assert!(!err.contains(&secret));
    let trail = format!("{:?}", actions(&store));
    assert!(!trail.contains(&secret), "{trail}");
    let events = actions(&store);
    let ours: Vec<_> = events.iter().filter(|e| e.0 == "account-invite").collect();
    assert_eq!(ours.len(), 2, "begun and finished: {events:?}");
    assert!(ours.iter().all(|e| e.3 == "system:operator-cli"));
    assert_eq!(ours.first().map(|e| e.1.as_str()), Some("invitation"));
    assert!(
        ours.get(1).is_some_and(|e| e.1.starts_with("invitation:")),
        "{ours:?}"
    );

    // The person who opens it makes an account with a password of their own.
    let store = std::sync::Arc::new(store);
    let auth = auth_over(&store);
    assert!(auth.invitation_usable(&secret, NOW + 60).unwrap());
    let signed_in = auth
        .redeem_with_password(
            &secret,
            "carol",
            "Carol",
            PASSWORD,
            &auth.client_address("203.0.113.7".parse().unwrap(), None),
            NOW + 60,
        )
        .unwrap();
    assert_eq!(
        store
            .account(&signed_in.account)
            .unwrap()
            .unwrap()
            .display_name
            .as_str(),
        "Carol"
    );
    assert!(!auth.invitation_usable(&secret, NOW + 61).unwrap(), "once");
    // Another invitation is another secret.
    let (_, out2, _) = cli(&d, "invite", &[]);
    assert_ne!(secret_of(&out2), secret);
    assert!(
        out2.lines().any(|l| l.starts_with("/-/invite/inv_")),
        "without an origin the path alone is given: {out2}"
    );
}

#[test]
fn the_first_administrator_is_made_with_an_invitation_that_names_a_role() {
    let (d, store, wiki_id) = world("first-admin");
    let (status, out, err) = cli(&d, "invite --role admin --wiki garden --days 3", &[]);
    assert_eq!(status, 0, "{err}");
    let secret = secret_of(&out);
    assert!(out.contains("2027-01-18"), "three days: {out}");
    let store = std::sync::Arc::new(store);
    let auth = auth_over(&store);
    let signed_in = auth
        .redeem_with_password(
            &secret,
            "root",
            "Root",
            PASSWORD,
            &auth.client_address("203.0.113.7".parse().unwrap(), None),
            NOW + 5,
        )
        .unwrap();
    let wiki = inklkunn_store::WikiId::parse(&wiki_id).unwrap();
    assert_eq!(
        store.role(&signed_in.account, &wiki).unwrap(),
        Some(Role::Admin)
    );
    // By id as well as by slug; and a wiki that is not there is refused before an invitation exists.
    let (status, _, _) = cli(
        &d,
        &format!("invite --role maintainer --wiki {wiki_id}"),
        &[],
    );
    assert_eq!(status, 0);
    let (status, out, err) = cli(&d, "invite --role admin --wiki nowhere", &[]);
    assert_eq!((status, out.as_str()), (1, ""));
    assert!(err.contains("no wiki `nowhere`"), "{err}");
    let begun_for_nowhere = actions(&store)
        .iter()
        .filter(|e| e.0 == "account-invite")
        .count();
    assert_eq!(
        begun_for_nowhere, 4,
        "two made, begun and finished each; none for the missing wiki"
    );
}

#[test]
fn an_act_the_trail_cannot_take_is_not_begun() {
    let (d, store, _) = world("trail-first");
    // Break the trail: an act that cannot be recorded must not happen.
    {
        let raw = rusqlite::Connection::open(d.0.join("store.sqlite")).unwrap();
        raw.execute_batch("DROP TABLE audit_log;").unwrap();
    }
    let before = |s: &SqliteStore| s.invitation_by_hash("sha256$none").unwrap().is_none();
    assert!(before(&store));
    let (status, out, err) = cli(&d, "invite --role admin --wiki garden", &[]);
    assert_eq!((status, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("nothing was done"), "{err}");
    let raw = rusqlite::Connection::open(d.0.join("store.sqlite")).unwrap();
    let made: i64 = raw
        .query_row("SELECT COUNT(*) FROM invitations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(made, 0, "the invitation was made before the trail took it");

    // The same for an account act: a role held and an account disabled before the trail broke stay as they were.
    let alice = store_account(&d, &store, "alice");
    let bob = store_account(&d, &store, "bobby");
    let wiki = store.by_slug(&Slug::new("garden").unwrap()).unwrap();
    store
        .grant(RoleGrant {
            account: bob.clone(),
            wiki: wiki.id.clone(),
            role: Role::Reviewer,
            granted_by: None,
            granted_at: NOW,
        })
        .unwrap();
    store.set_disabled(&bob, true).unwrap();
    for args in [
        "disable alice",
        "link alice https://idp.example subject-9",
        "grant alice maintainer garden",
        "revoke-grant bobby garden",
        "enable bobby",
    ] {
        let (status, _, err) = cli(&d, args, &[]);
        assert_eq!(status, 1, "{args}: {err}");
        assert!(err.contains("nothing was done"), "{args}: {err}");
    }
    assert!(!store.account(&alice).unwrap().unwrap().disabled);
    assert!(
        store
            .account_by_identity(
                &ExternalIdentity::new("https://idp.example", "subject-9").unwrap()
            )
            .unwrap()
            .is_none(),
        "the link was made before the trail took it"
    );
    assert_eq!(store.role(&alice, &wiki.id).unwrap(), None);
    assert_eq!(store.role(&bob, &wiki.id).unwrap(), Some(Role::Reviewer));
    assert!(store.account(&bob).unwrap().unwrap().disabled);
}

fn store_account(_d: &Dir, store: &SqliteStore, login: &str) -> inklkunn_store::AccountId {
    use inklkunn_store::{Account, AccountId, LocalCredential, LoginName};
    let account = Account {
        id: AccountId::new(),
        display_name: DisplayName::new(login).unwrap(),
        local: Some(LocalCredential {
            login: LoginName::parse(login).unwrap(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaC1mb3ItdGVzdHM1"
                .to_owned(),
        }),
        created_at: NOW - 1000,
        disabled: false,
    };
    store.add_account(&account).unwrap();
    account.id
}

#[test]
fn an_account_is_disabled_and_enabled_by_login_or_id_and_both_are_audited() {
    let (d, store, _) = world("disable");
    let alice = store_account(&d, &store, "alice");
    let (status, out, err) = cli(&d, "disable ALICE", &[]);
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("is disabled"), "{out}");
    assert!(store.account(&alice).unwrap().unwrap().disabled);
    let (status, out, _) = cli(&d, &format!("enable {alice}"), &[]);
    assert_eq!(status, 0);
    assert!(out.contains("is enabled"), "{out}");
    assert!(!store.account(&alice).unwrap().unwrap().disabled);
    let seen: Vec<String> = actions(&store)
        .into_iter()
        .filter(|e| e.0.starts_with("account-"))
        .map(|e| format!("{} {} {}", e.0, e.1, e.2))
        .collect();
    let id = alice.to_string();
    assert_eq!(
        seen,
        [
            format!("account-disable account:{id} allowed"),
            format!("account-disable account:{id} allowed"),
            format!("account-enable account:{id} allowed"),
            format!("account-enable account:{id} allowed"),
        ]
    );
    // Nobody by that name, and a name that cannot be one, are told so, and nothing is audited.
    for who in ["nobody", "!!", "019a0000-0000-7000-8000-000000000001"] {
        let (status, _, err) = cli(&d, &format!("disable {who}"), &[]);
        assert_eq!(status, 1, "{who}");
        assert!(
            err.contains(who) || err.contains("no account"),
            "{who}: {err}"
        );
    }
    assert_eq!(
        actions(&store)
            .iter()
            .filter(|e| e.0 == "account-disable")
            .count(),
        2
    );
}

#[test]
fn a_disabled_account_cannot_sign_in_and_its_sessions_and_tokens_are_gone() {
    let (d, store, _) = world("disable-live");
    let store = std::sync::Arc::new(store);
    let auth = auth_over(&store);
    let made = auth
        .register_local("dana", "Dana", PASSWORD, NOW - 100)
        .unwrap();
    let client = auth.client_address("203.0.113.1".parse().unwrap(), None);
    let session = auth.sign_in("dana", PASSWORD, &client, NOW - 50).unwrap();
    let token = auth
        .issue_token(
            &made.id,
            inklkunn_store::Scope::Propose,
            "ci",
            86_400,
            NOW - 40,
        )
        .unwrap();
    let (status, _, err) = cli(&d, "disable dana", &[]);
    assert_eq!(status, 0, "{err}");
    let secret = session.session.secret().as_str().to_owned();
    assert!(auth.principal_for_session(&secret, NOW).unwrap().is_none());
    assert!(
        auth.principal_for_token(token.secret.as_str(), NOW)
            .is_err()
    );
    assert!(auth.sign_in("dana", PASSWORD, &client, NOW).is_err());
    let (status, _, _) = cli(&d, "enable dana", &[]);
    assert_eq!(status, 0);
    assert!(auth.sign_in("dana", PASSWORD, &client, NOW + 1).is_ok());
    assert!(
        auth.principal_for_token(token.secret.as_str(), NOW + 1)
            .is_err(),
        "enabling does not bring a revoked token back"
    );
}

#[test]
fn a_role_is_granted_and_taken_away_and_each_is_audited() {
    let (d, store, wiki_id) = world("grant");
    let alice = store_account(&d, &store, "alice");
    let wiki = inklkunn_store::WikiId::parse(&wiki_id).unwrap();
    let (status, out, err) = cli(&d, "grant alice reviewer garden", &[]);
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("reviewer in garden"), "{out}");
    assert_eq!(store.role(&alice, &wiki).unwrap(), Some(Role::Reviewer));
    let grants = store.grants_in(&wiki).unwrap();
    let grant = grants.first().unwrap();
    assert_eq!(grant.granted_by, None, "the operator granted it");
    // A promotion replaces the role.
    let (status, _, _) = cli(&d, &format!("grant {alice} admin {wiki_id}"), &[]);
    assert_eq!(status, 0);
    assert_eq!(store.role(&alice, &wiki).unwrap(), Some(Role::Admin));
    let (status, out, _) = cli(&d, "revoke-grant alice garden", &[]);
    assert_eq!(status, 0);
    assert!(out.contains("no longer has a role in garden"), "{out}");
    assert_eq!(store.role(&alice, &wiki).unwrap(), None);
    let (status, out, _) = cli(&d, "revoke-grant alice garden", &[]);
    assert_eq!(status, 0);
    assert!(out.contains("held no role"), "{out}");
    let names: Vec<String> = actions(&store)
        .into_iter()
        .filter(|e| e.0.starts_with("account-"))
        .map(|e| e.0)
        .collect();
    assert_eq!(names.iter().filter(|n| *n == "account-grant").count(), 4);
    assert_eq!(
        names
            .iter()
            .filter(|n| *n == "account-revoke-grant")
            .count(),
        4
    );
    // An account or a wiki that is not there is refused.
    for args in ["grant nobody admin garden", "grant alice admin nowhere"] {
        let (status, _, err) = cli(&d, args, &[]);
        assert_eq!(status, 1, "{args}: {err}");
    }
}

#[test]
fn an_identity_is_linked_by_login_or_id_audited_and_one_identity_has_one_account() {
    let (d, store, _) = world("link");
    let alice = store_account(&d, &store, "alice");
    let bob = store_account(&d, &store, "bobby");
    let identity = ExternalIdentity::new("https://idp.example", "subject-1").unwrap();

    let (status, out, err) = cli(&d, "link ALICE https://idp.example subject-1", &[]);
    assert_eq!(status, 0, "{err}");
    assert!(
        out.contains(&alice.to_string()) && out.contains("subject-1"),
        "{out}"
    );
    assert_eq!(
        store.account_by_identity(&identity).unwrap().map(|a| a.id),
        Some(alice.clone())
    );
    // Again for the same account is fine; another account is refused, and nothing moves.
    let (status, _, err) = cli(
        &d,
        &format!("link {alice} https://idp.example subject-1"),
        &[],
    );
    assert_eq!(status, 0, "{err}");
    let (status, out, err) = cli(&d, "link bobby https://idp.example subject-1", &[]);
    assert_eq!((status, out.as_str()), (1, ""), "{err}");
    assert!(err.contains("one person has one account"), "{err}");
    assert_eq!(
        store.account_by_identity(&identity).unwrap().map(|a| a.id),
        Some(alice.clone())
    );
    // By id, as well as by sign-in name.
    let (status, _, err) = cli(
        &d,
        &format!("link {bob} https://idp.example subject-2"),
        &[],
    );
    assert_eq!(status, 0, "{err}");
    // An account that is not there and an identity no provider could report change nothing, and the second
    // never reaches the trail.
    let too_long = "s".repeat(300);
    for args in [
        "link nobody https://idp.example subject-3".to_owned(),
        format!("link alice https://idp.example {too_long}"),
    ] {
        let (status, out, err) = cli(&d, &args, &[]);
        assert_eq!((status, out.as_str()), (1, ""), "{args}");
        assert!(!err.is_empty(), "{args}");
    }
    assert!(
        store
            .account_by_identity(
                &ExternalIdentity::new("https://idp.example", "subject-3").unwrap()
            )
            .unwrap()
            .is_none()
    );
    // Audited before it is done and again when it ends: the refused one as failed.
    let seen: Vec<String> = actions(&store)
        .into_iter()
        .filter(|e| e.0 == "account-link")
        .map(|e| format!("{} {}", e.1, e.2))
        .collect();
    assert_eq!(
        seen,
        [
            format!("account:{alice} allowed"),
            format!("account:{alice} allowed"),
            format!("account:{alice} allowed"),
            format!("account:{alice} allowed"),
            format!("account:{bob} allowed"),
            format!("account:{bob} failed"),
            format!("account:{bob} allowed"),
            format!("account:{bob} allowed"),
        ]
    );
}
