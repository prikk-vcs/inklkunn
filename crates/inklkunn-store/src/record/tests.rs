use super::*;

fn name(s: &str) -> DisplayName {
    DisplayName::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
}

#[test]
fn ids_are_v7_shaped_unique_and_parse() {
    let a = WikiId::new();
    let b = WikiId::new();
    assert_ne!(a, b);
    assert_eq!(a.as_str().len(), 36);
    assert_eq!(a.as_str().as_bytes().get(14), Some(&b'7'), "{a}");
    assert!(
        matches!(
            a.as_str().as_bytes().get(19),
            Some(b'8' | b'9' | b'a' | b'b')
        ),
        "{a}"
    );
    assert_eq!(WikiId::parse(a.as_str()), Some(a.clone()));
    assert_eq!(WikiId::parse("not-an-id"), None);
    assert_eq!(WikiId::parse(&a.as_str().to_uppercase()), None);
    assert_eq!(a.shard().len(), 2);
    // Many ids drawn quickly are distinct (the random tail is real).
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..2000 {
        assert!(seen.insert(WikiId::new()));
    }
}

#[test]
fn deserialization_goes_through_validation() {
    let bad: Result<WikiId, _> = serde_json::from_str("\"../../etc\"");
    assert!(bad.is_err(), "a malformed id must not deserialize");
    let bad: Result<Slug, _> = serde_json::from_str("\"../etc\"");
    assert!(bad.is_err());
    let bad: Result<DisplayName, _> = serde_json::from_str("\"a\\nAccepted-By: x\"");
    assert!(bad.is_err());
    let ok: WikiId = serde_json::from_str("\"019a0000-0000-7000-8000-000000000001\"")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ok.as_str(), "019a0000-0000-7000-8000-000000000001");
    assert_eq!(
        serde_json::to_string(&ok).unwrap_or_default(),
        "\"019a0000-0000-7000-8000-000000000001\""
    );
}

#[test]
fn slugs_are_validated() {
    assert!(Slug::new("my-wiki-2").is_ok());
    assert_eq!(Slug::new(""), Err(SlugError::Length));
    assert_eq!(Slug::new(&"a".repeat(65)), Err(SlugError::Length));
    for bad in ["My-Wiki", "-a", "a-", "a--b", "a_b", "a b", "ä"] {
        assert_eq!(Slug::new(bad), Err(SlugError::Characters), "{bad}");
    }
    assert_eq!(Slug::new("api"), Err(SlugError::Reserved));
    // The reserved answer wins over the character answer, so the user hears the real reason.
    assert_eq!(Slug::new("robots"), Err(SlugError::Reserved));
    assert_eq!(
        Slug::new("019a0000-0000-7000-8000-000000000001"),
        Err(SlugError::LooksLikeId)
    );
    // A stored slug is read back by the character rules only.
    assert!(Slug::from_stored("api").is_ok());
    assert_eq!(Slug::from_stored("A"), Err(SlugError::Characters));
}

#[test]
fn display_names_are_validated() {
    assert!(DisplayName::new("First Wiki").is_ok());
    assert!(DisplayName::new("日本語の名前 (test)").is_ok());
    assert_eq!(DisplayName::new(""), Err(DisplayNameError::Length));
    assert_eq!(
        DisplayName::new(&"x".repeat(121)),
        Err(DisplayNameError::Length)
    );
    assert_eq!(
        DisplayName::new("a\nAccepted-By: system"),
        Err(DisplayNameError::Control)
    );
    assert_eq!(DisplayName::new("a\tb"), Err(DisplayNameError::Control));
    assert_eq!(DisplayName::new(" a"), Err(DisplayNameError::Whitespace));
    assert_eq!(DisplayName::new("a "), Err(DisplayNameError::Whitespace));
}

#[test]
fn records_fill_defaults_and_validate_free_text() {
    let slug = || Slug::new("wiki-w").unwrap_or_else(|e| panic!("{e}"));
    let r = WikiRecord::from_new(WikiId::new(), NewWiki::new(slug(), name("W")), 1)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(r.default_view, "heads/main");
    assert_eq!(r.default_license, "CC-BY-SA-4.0");
    assert_eq!(r.languages, vec!["en"]);
    assert_eq!(r.policy_bootstrap, "default");
    assert_eq!(r.verification.state, VerificationState::Unknown);
    assert_eq!(
        VerificationState::parse("could-not-check"),
        Some(VerificationState::CouldNotCheck)
    );
    assert_eq!(Visibility::parse("internal"), Some(Visibility::Internal));

    let mut n = NewWiki::new(slug(), name("W"));
    n.default_view = Some("main".into());
    assert_eq!(
        WikiRecord::from_new(WikiId::new(), n, 1).err(),
        Some(NewWikiError::DefaultView)
    );
    let mut n = NewWiki::new(slug(), name("W"));
    n.default_license = Some("LicenseRef: x\nfoo".into());
    assert_eq!(
        WikiRecord::from_new(WikiId::new(), n, 1).err(),
        Some(NewWikiError::License)
    );
    let mut n = NewWiki::new(slug(), name("W"));
    n.default_license = Some("MIT OR Apache-2.0".into());
    assert!(WikiRecord::from_new(WikiId::new(), n, 1).is_ok());
    let mut n = NewWiki::new(slug(), name("W"));
    n.languages = vec!["ja".into(), "en-GB".into(), "x y".into()];
    assert_eq!(
        WikiRecord::from_new(WikiId::new(), n, 1).err(),
        Some(NewWikiError::Language("x y".into()))
    );
    let mut n = NewWiki::new(slug(), name("W"));
    n.policy_bootstrap = Some("Default Policy".into());
    assert_eq!(
        WikiRecord::from_new(WikiId::new(), n, 1).err(),
        Some(NewWikiError::PolicyBootstrap)
    );
}
