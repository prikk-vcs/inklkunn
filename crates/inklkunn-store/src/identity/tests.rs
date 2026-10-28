#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn a_login_name_is_folded_and_bounded() {
    assert_eq!(
        LoginName::parse("Alice.W-1_x").unwrap().as_str(),
        "alice.w-1_x"
    );
    assert_eq!(
        LoginName::parse("ALICE").unwrap(),
        LoginName::parse("alice").unwrap(),
        "sign-in names are case-insensitive"
    );
    assert_eq!(LoginName::parse("ab"), Err(LoginNameError::Length));
    assert_eq!(
        LoginName::parse(&"a".repeat(65)),
        Err(LoginNameError::Length)
    );
    assert!(LoginName::parse(&"a".repeat(64)).is_ok());
    for bad in [
        "-alice",
        ".alice",
        "_alice",
        "ali ce",
        "ali\nce",
        "alicé",
        "ａｌｉｃｅ",
        "ali/ce",
        "ali@ce",
    ] {
        assert_eq!(
            LoginName::parse(bad),
            Err(LoginNameError::Characters),
            "{bad:?}"
        );
    }
}

#[test]
fn an_external_identity_is_an_issuer_and_a_subject_and_nothing_loose() {
    let id = ExternalIdentity::new("https://idp.example", "user-42").unwrap();
    assert_eq!(
        (id.issuer(), id.subject()),
        ("https://idp.example", "user-42")
    );
    assert_eq!(
        ExternalIdentity::new("", "s"),
        Err(ExternalIdentityError::Issuer)
    );
    assert_eq!(
        ExternalIdentity::new("https://idp.example/ x", "s"),
        Err(ExternalIdentityError::Issuer)
    );
    assert_eq!(
        ExternalIdentity::new(&"h".repeat(513), "s"),
        Err(ExternalIdentityError::Issuer)
    );
    assert_eq!(
        ExternalIdentity::new("https://i", ""),
        Err(ExternalIdentityError::Subject)
    );
    assert_eq!(
        ExternalIdentity::new("https://i", &"s".repeat(256)),
        Err(ExternalIdentityError::Subject)
    );
    assert_eq!(
        ExternalIdentity::new("https://i", "a\u{0}b"),
        Err(ExternalIdentityError::Subject)
    );
    assert!(ExternalIdentity::new("https://i", &"s".repeat(255)).is_ok());
}

#[test]
fn a_label_is_one_short_line() {
    assert_eq!(clean_label("ci\nbot\u{2028}x"), "ci?bot?x");
    assert_eq!(clean_label(&"é".repeat(200)).chars().count(), 80);
}

#[test]
fn debug_output_never_carries_a_hash_or_a_session_secret() {
    let credential = LocalCredential {
        login: LoginName::parse("alice").unwrap(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$SALT$HASH".into(),
    };
    assert!(!format!("{credential:?}").contains("argon2id"));
    let session = SessionRecord {
        id_hash: "sha256$aaaa".into(),
        account: AccountId::new(),
        csrf: "csrf-secret-value".into(),
        created_at: 1,
        expires_at: 2,
    };
    let shown = format!("{session:?}");
    assert!(
        !shown.contains("sha256") && !shown.contains("csrf-secret-value"),
        "{shown}"
    );
    let token = TokenRecord {
        id: TokenId::new(),
        hash: "sha256$bbbb".into(),
        account: AccountId::new(),
        scope: Scope::Read,
        label: "ci".into(),
        created_at: 1,
        expires_at: None,
        revoked_at: None,
    };
    assert!(!format!("{token:?}").contains("sha256"));
}

#[test]
fn an_argon2id_hash_is_recognised_by_its_shape_and_nothing_else_is() {
    for good in [
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo",
        "$argon2id$v=19$m=65536,t=3,p=4$YWJj$ZGVm",
    ] {
        assert!(looks_like_an_argon2id_hash(good), "{good}");
    }
    for bad in [
        "",
        "!",
        "plaintext",
        "$argon2i$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA",
        "$argon2id$m=19456,t=2,p=1$c2FsdA$aGFzaA",
        "$argon2id$v=19$m=19456,t=2$c2FsdA$aGFzaA",
        "$argon2id$v=19$t=2,m=19456,p=1$c2FsdA$aGFzaA",
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA",
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA$extra",
        "$argon2id$v=19$m=19456,t=2,p=1$$aGFzaA",
        "$argon2id$v=19$m=19456,t=2,p=1$c2 Fs$aGFzaA",
        "$bcrypt$12$abcdefghijklmnopqrstuv",
    ] {
        assert!(!looks_like_an_argon2id_hash(bad), "{bad:?}");
    }
    assert!(!looks_like_an_argon2id_hash(&format!(
        "$argon2id$v=19$m=1,t=1,p=1$a${}",
        "b".repeat(600)
    )));
}
