use super::*;

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
}

#[test]
fn records_fill_defaults() {
    let r = WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(Slug::new("wiki-w").unwrap_or_else(|e| panic!("{e}")), "W"),
        1,
    );
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
}
