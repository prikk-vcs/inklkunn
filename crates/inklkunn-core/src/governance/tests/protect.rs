//! The `protect` list: its spelling, its refusals, and the one matching rule (whole path segments) G-T3 uses.

use super::*;

const GOOD: &str = "protect:\n  - namespace: Policies\n    require: review\n    reason: \"Rules are read first.\"\n  - path: Home\n    require:\n      rung: maintainer\n    reason: \"One hand.\"\n";

#[test]
fn a_protection_is_a_namespace_or_a_path_with_a_requirement_and_a_reason() {
    let p = policy(GOOD);
    let [ns, page] = p.protections() else {
        panic!("two protections");
    };
    assert_eq!(ns.target(), &Target::Namespace("Policies".to_owned()));
    assert_eq!(ns.require(), Require::Review);
    assert_eq!(ns.reason(), "Rules are read first.");
    assert_eq!(page.target(), &Target::Path("Home".to_owned()));
    assert_eq!(page.require(), Require::Rung(Role::Maintainer));
    assert_eq!(page.target().title(), "Home");
    // Flow forms read the same.
    let flow = policy(
        "protect:\n  - {path: Home, require: {rung: admin}, reason: Why}\n  - {namespace: A/B, require: review, reason: Why}\n",
    );
    assert_eq!(flow.protections().len(), 2);
    assert_eq!(
        flow.protections().first().unwrap().require(),
        Require::Rung(Role::Admin)
    );
    // Empty is none.
    assert!(policy("protect: []\n").protections().is_empty());
    assert!(policy("protect:\n").protections().is_empty());
}

#[test]
fn exactly_one_of_path_and_namespace() {
    let both = policy_fault(
        "protect:\n  - path: A\n    namespace: B\n    require: review\n    reason: x\n",
    );
    assert_eq!(
        (both.refusal, both.field, both.item),
        (Refusal::WrongShape, "protect", Some(1))
    );
    assert_eq!(both.line, Some(2));
    let neither = policy_fault("protect:\n  - require: review\n    reason: x\n");
    assert_eq!(
        (neither.refusal, neither.field, neither.item),
        (Refusal::Missing, "protect.path", Some(1))
    );
}

#[test]
fn require_is_the_word_review_or_a_rung_in_a_mapping_and_never_both_misread() {
    let entry =
        |require: &str| format!("protect:\n  - path: A\n    require: {require}\n    reason: x\n");
    assert!(validate_policy(&entry("review"), &PlatformLimits::default()).is_ok());
    // `review` is not a rung and `reviewer` is not `review`: a rung's name as the bare word is refused.
    for bad in [
        "reviewer",
        "maintainer",
        "admin",
        "Review",
        "required",
        "\"\"",
        "rung",
    ] {
        let f = policy_fault(&entry(bad));
        assert_eq!(
            (f.refusal, f.field),
            (Refusal::UnknownValue, "protect.require"),
            "{bad}"
        );
    }
    for bad in ["3", "[review]", "true"] {
        let f = policy_fault(&entry(bad));
        assert_eq!(f.refusal, Refusal::WrongShape, "{bad}");
    }
    // The mapping has one key, `rung`, with a rung of the ladder.
    for rung in Role::ALL {
        let text = format!(
            "protect:\n  - path: A\n    require: {{rung: {}}}\n    reason: x\n",
            rung.as_str()
        );
        let p = policy(&text);
        assert_eq!(
            p.protections().first().unwrap().require(),
            Require::Rung(rung)
        );
    }
    let f = policy_fault("protect:\n  - path: A\n    require: {rung: review}\n    reason: x\n");
    assert_eq!(
        (f.refusal, f.field),
        (Refusal::UnknownValue, "protect.require.rung")
    );
    let f = policy_fault("protect:\n  - path: A\n    require: {}\n    reason: x\n");
    assert_eq!(
        (f.refusal, f.field),
        (Refusal::Missing, "protect.require.rung")
    );
    let f = policy_fault("protect:\n  - path: A\n    require: {review: true}\n    reason: x\n");
    assert_eq!(
        (f.refusal, f.field),
        (Refusal::UnknownKey, "protect.require")
    );
    let f = policy_fault("protect:\n  - path: A\n    reason: x\n");
    assert_eq!((f.refusal, f.field), (Refusal::Missing, "protect.require"));
}

#[test]
fn a_reason_is_required_and_bounded() {
    let with = |reason: &str| {
        format!("protect:\n  - path: A\n    require: review\n    reason: {reason}\n")
    };
    for empty in ["\"\"", "\"   \""] {
        assert_eq!(policy_fault(&with(empty)).refusal, Refusal::Missing);
    }
    let longest = "r".repeat(MAX_REASON_CHARS);
    assert!(validate_policy(&with(&format!("\"{longest}\"")), &PlatformLimits::default()).is_ok());
    let f = policy_fault(&with(&format!("\"{longest}r\"")));
    assert_eq!((f.refusal, f.field), (Refusal::Bounds, "protect.reason"));
    assert_eq!(policy_fault(&with("12")).refusal, Refusal::WrongShape);
    // A reason is one line: an escaped line break is a forbidden character.
    assert_eq!(
        policy_fault(&with("\"a\\nb\"")).refusal,
        Refusal::ForbiddenCharacter
    );
    // Japanese counts in characters, not bytes.
    let ja = "あ".repeat(MAX_REASON_CHARS);
    assert!(validate_policy(&with(&format!("\"{ja}\"")), &PlatformLimits::default()).is_ok());
}

#[test]
fn titles_have_one_form_so_a_name_means_one_thing() {
    let with = |key: &str, title: &str| {
        format!("protect:\n  - {key}: \"{title}\"\n    require: review\n    reason: x\n")
    };
    for key in ["path", "namespace"] {
        let field = if key == "path" {
            "protect.path"
        } else {
            "protect.namespace"
        };
        for ok in [
            "Help",
            "Help/Start",
            "a b/c d",
            "日本語/ページ",
            "A.b",
            "x-y_z",
        ] {
            assert!(
                validate_policy(&with(key, ok), &PlatformLimits::default()).is_ok(),
                "{key} {ok}"
            );
        }
        for bad in [
            "Help/",
            "/Help",
            "Help//x",
            " Help",
            "Help ",
            "Help/ x",
            "a/ /b",
            ".",
            "..",
            "a/../b",
            "a/./b",
            "",
            "e\u{0301}", // not NFC
        ] {
            let f = validate_policy(&with(key, bad), &PlatformLimits::default())
                .expect_err(&format!("{key} {bad:?}"));
            assert!(
                matches!(f.refusal, Refusal::UnknownValue | Refusal::Missing),
                "{key} {bad:?}: {:?}",
                f.refusal
            );
            assert_eq!(f.field, field);
        }
    }
    let longest = "t".repeat(MAX_TARGET_CHARS);
    assert!(validate_policy(&with("path", &longest), &PlatformLimits::default()).is_ok());
    let f = policy_fault(&with("path", &format!("{longest}t")));
    assert_eq!(f.refusal, Refusal::Bounds);
}

#[test]
fn the_same_target_twice_is_refused_and_a_path_and_its_namespace_are_two_targets() {
    let f = policy_fault(
        "protect:\n  - path: A\n    require: review\n    reason: x\n  - path: A\n    require: {rung: admin}\n    reason: y\n",
    );
    assert_eq!((f.refusal, f.item), (Refusal::Duplicate, Some(2)));
    assert_eq!(f.line, Some(5));
    // A page and a namespace of the same title are different things to protect.
    let p = policy(
        "protect:\n  - path: A\n    require: review\n    reason: x\n  - namespace: A\n    require: review\n    reason: y\n",
    );
    assert_eq!(p.protections().len(), 2);
}

#[test]
fn the_list_is_bounded() {
    let entry = |i: usize| format!("  - path: P{i}\n    require: review\n    reason: x\n");
    let many = |n: usize| format!("protect:\n{}", (0..n).map(entry).collect::<String>());
    assert_eq!(policy(&many(MAX_PROTECT)).protections().len(), MAX_PROTECT);
    let f = policy_fault(&many(MAX_PROTECT + 1));
    assert_eq!((f.refusal, f.field), (Refusal::Bounds, "protect"));
}

#[test]
fn a_largest_valid_policy_fits_the_files_bound_even_in_four_byte_characters() {
    // The longest title a page path allows in Japanese: four components, each below the path's component bound.
    let part = "あ".repeat(25);
    let target = format!("{part}/{part}/{part}/{part}/");
    let reason = "𠮷".repeat(MAX_REASON_CHARS);
    let promotion = "𠮷".repeat(MAX_PROMOTION_CHARS);
    let mut text = String::new();
    text.push_str("promotion:\n");
    for rung in Role::ALL {
        text.push_str(&format!("  {}: \"{promotion}\"\n", rung.as_str()));
    }
    text.push_str("protect:\n");
    for i in 0..MAX_PROTECT {
        text.push_str(&format!(
            "  - path: \"{target}{i}\"\n    require: review\n    reason: \"{reason}\"\n"
        ));
    }
    assert!(text.len() < MAX_POLICY_BYTES, "{} bytes", text.len());
    assert_eq!(policy(&text).protections().len(), MAX_PROTECT);
}

// ----------------------------------------------------------------------------------------------- matching

#[test]
fn a_namespace_covers_the_whole_segments_below_it() {
    // The rule: a proper prefix of whole segments.
    for (ns, title, covers) in [
        ("Help", "Help/x", true),
        ("Help", "Help/x/y", true),
        ("Help/x", "Help/x/y", true),
        // The page of the same name is not below its own namespace: protect it with `path`.
        ("Help", "Help", false),
        // A prefix of the text is not a prefix of segments.
        ("Hel", "Help/x", false),
        ("Hel", "Help", false),
        ("Help", "Helpful/x", false),
        ("Help", "Helper", false),
        ("Help/x", "Help/xy", false),
        ("Help/x", "Help/xy/z", false),
        ("Help", "Hel/p", false),
        ("Help", "x/Help/y", false),
        ("Help/x", "Help", false),
        // Case matters: two titles.
        ("help", "Help/x", false),
        ("Help", "help/x", false),
        ("HELP", "Help/x", false),
        // A trailing slash is a form no protection holds: it covers nothing, not even what `Help` would.
        ("Help/", "Help/x", false),
        ("Help/", "Help//x", false),
        ("Help/", "Help/", false),
        // A title with a trailing slash is not below anything.
        ("Help", "Help/", false),
        ("Help", "Help//", false),
        // Nothing is below the empty namespace.
        ("", "Help/x", false),
        ("", "", false),
        ("/", "/x", false),
    ] {
        assert_eq!(namespace_covers(ns, title), covers, "{ns:?} over {title:?}");
    }
}

#[test]
fn a_target_covers_by_title_and_by_the_repository_path_of_the_page() {
    let ns = Target::namespace("Help").unwrap();
    let page = Target::path("Help").unwrap();
    assert!(ns.covers_title("Help/x"));
    assert!(!ns.covers_title("Help"));
    assert!(page.covers_title("Help"));
    assert!(!page.covers_title("Help/x"));
    assert!(!page.covers_title("help"));
    // By the path of the page file.
    assert!(ns.covers_path("pages/Help/x.md"));
    assert!(ns.covers_path("pages/Help/x/y.md"));
    assert!(!ns.covers_path("pages/Help.md"));
    assert!(!ns.covers_path("pages/Hel/p.md"));
    assert!(!ns.covers_path("pages/Helpful/x.md"));
    assert!(page.covers_path("pages/Help.md"));
    assert!(!page.covers_path("pages/Help/x.md"));
    // Not a page's path: covers nothing.
    for not_a_page in [
        "",
        "Help/x",
        "governance/policy.yaml",
        "pages/Help/x",
        "pages/.md",
        "pages//x.md",
        "attachments/Help/x.md",
    ] {
        assert!(!ns.covers_path(not_a_page), "{not_a_page:?}");
        assert!(!page.covers_path(not_a_page), "{not_a_page:?}");
    }
    // Titles with characters a path encodes are matched as titles.
    let japanese = Target::namespace("日本語").unwrap();
    let path = inklkunn_markup::title_to_path("日本語/ページ").unwrap();
    assert!(path.is_ascii());
    assert!(japanese.covers_path(&path));
    assert!(japanese.covers_title("日本語/ページ"));
    let spaced = Target::path("a b/c").unwrap();
    assert!(spaced.covers_path(&inklkunn_markup::title_to_path("a b/c").unwrap()));
    // A target is only made of a canonical title.
    assert!(Target::namespace("Help/").is_none());
    assert!(Target::path(" Help").is_none());
    assert!(Target::path("").is_none());
}

#[test]
fn a_policy_lists_the_protections_that_cover_a_page() {
    let p = policy(
        "protect:\n  - namespace: Policies\n    require: review\n    reason: a\n  - path: Policies/Core\n    require: {rung: admin}\n    reason: b\n  - path: Home\n    require: review\n    reason: c\n",
    );
    let reasons = |title: &str| {
        p.protections_of_title(title)
            .map(|x| x.reason().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        reasons("Policies/Core"),
        ["a", "b"],
        "both are listed, in file order"
    );
    assert_eq!(reasons("Policies/Other"), ["a"]);
    assert_eq!(reasons("Home"), ["c"]);
    assert!(reasons("Elsewhere").is_empty());
    assert_eq!(p.protections_of_path("pages/Policies/Core.md").count(), 2);
    assert_eq!(p.protections_of_path("pages/Home.md").count(), 1);
    assert_eq!(p.protections_of_path("pages/Policies.md").count(), 0);
    assert!(p.protections().first().unwrap().covers_title("Policies/x"));
    assert!(p.protections().get(2).unwrap().covers_path("pages/Home.md"));
}
