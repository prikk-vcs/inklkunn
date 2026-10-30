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
    // A check that could not run is not a verdict: its old stored name no longer parses as one.
    assert_eq!(VerificationState::parse("could-not-check"), None);
    assert_eq!(r.last_attempt, None);
    for why in [NotRun::Busy, NotRun::Limit, NotRun::Unavailable] {
        assert_eq!(NotRun::parse(why.as_str()), Some(why));
    }
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

#[test]
fn account_and_proposal_ids_follow_the_wiki_id_rules() {
    let a = AccountId::new();
    assert_eq!(a.as_str().len(), 36);
    assert_eq!(a.as_str().as_bytes().get(14), Some(&b'7'), "{a}");
    assert_eq!(AccountId::parse(a.as_str()), Some(a.clone()));
    assert_eq!(AccountId::parse("anonymous"), None);
    assert_eq!(AccountId::parse("system"), None);
    assert_eq!(AccountId::parse(&a.as_str().to_uppercase()), None);
    assert_eq!(AccountId::parse(&format!("{a} ")), None);
    let p = ProposalId::new();
    assert_eq!(ProposalId::parse(p.as_str()), Some(p.clone()));
    // Fresh ids are distinct, and the two kinds are separate types: an account id is not a proposal id
    // even when the text is the same (the compiler refuses mixing them; this checks the values differ).
    assert_ne!(a.as_str(), p.as_str());
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..1000 {
        assert!(seen.insert(AccountId::new()));
    }
}

#[test]
fn account_and_proposal_ids_deserialize_only_through_validation() {
    for bad in [
        "\"../../etc\"",
        "\"\"",
        "\"system\"",
        "\"A0000000-0000-7000-8000-000000000001\"",
    ] {
        assert!(serde_json::from_str::<AccountId>(bad).is_err(), "{bad}");
        assert!(serde_json::from_str::<ProposalId>(bad).is_err(), "{bad}");
    }
    let id: AccountId = serde_json::from_str("\"019a0000-0000-7000-8000-000000000002\"")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        serde_json::to_string(&id).unwrap_or_default(),
        "\"019a0000-0000-7000-8000-000000000002\""
    );
}

#[test]
fn a_display_name_has_no_line_break_of_any_kind() {
    // LF, CR, NEL are controls; U+2028 and U+2029 are not, yet readers that split lines on them would see
    // a forged trailer inside a signed message (found writing the patch-message contract, RFC 002 D-5).
    for bad in [
        "a\nb",
        "a\rb",
        "a\u{85}b",
        "a\u{2028}b",
        "a\u{2029}b",
        "a\tb",
        "a\0b",
        "a\u{7f}b",
    ] {
        assert_eq!(
            DisplayName::new(bad),
            Err(DisplayNameError::Control),
            "{bad:?}"
        );
    }
    // Other marks stay: a display name is any script, with the punctuation people use.
    for ok in [
        "Aiko Tanaka",
        "日本語 (にほんご)",
        "قارئ",
        "O'Brien-Smith, Jr.",
        "a\u{200b}b",
    ] {
        assert!(DisplayName::new(ok).is_ok(), "{ok:?}");
    }
}

#[test]
fn rfc3339_writes_utc_to_the_second_across_epochs_and_leap_days() {
    for (secs, text) in [
        (0, "1970-01-01T00:00:00Z"),
        (1_700_000_000, "2023-11-14T22:13:20Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_709_164_799, "2024-02-28T23:59:59Z"),
        (1_709_164_800, "2024-02-29T00:00:00Z"),
        (1_709_251_200, "2024-03-01T00:00:00Z"),
        (-1, "1969-12-31T23:59:59Z"),
    ] {
        assert_eq!(rfc3339(secs), text, "{secs}");
    }
}
