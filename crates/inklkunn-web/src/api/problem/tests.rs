#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::catalog::{Dir, EN, EN_DEF, Entry, Lang, LangDef, Plural, Review};

use super::*;

fn ja() -> T {
    T::new(Lang::from_tag("ja").unwrap())
}

fn en() -> T {
    T::new(EN)
}

fn other_only(_: u64) -> Plural {
    Plural::Other
}

/// English wrapped in guillemets: every word a problem takes from the catalog shows, and a word that does not
/// come from the catalog does not.
fn wrapped() -> T {
    static DEF: OnceLock<&'static LangDef> = OnceLock::new();
    let def = DEF.get_or_init(|| {
        let entries: Vec<Entry> = EN_DEF
            .entries
            .iter()
            .map(|e| {
                let text: &'static str = Box::leak(format!("‹{}›", e.other).into_boxed_str());
                Entry {
                    key: e.key,
                    one: text,
                    other: text,
                }
            })
            .collect();
        Box::leak(Box::new(LangDef {
            tag: "qaa",
            endonym: "Test language",
            dir: Dir::Ltr,
            plural: other_only,
            group_separator: ',',
            review: Review::Preview,
            entries: Box::leak(entries.into_boxed_slice()),
        }))
    });
    T::new(Lang::from_def(def))
}

fn text(body: &Value, member: &str) -> String {
    body.get(member)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no {member} in {body}"))
        .to_owned()
}

#[test]
fn the_registry_lists_each_code_once_with_a_stable_lower_case_name_and_a_urn_type() {
    let slugs: BTreeSet<&str> = Code::ALL.iter().map(|c| c.slug()).collect();
    assert_eq!(slugs.len(), Code::ALL.len(), "a code is listed twice");
    let types: BTreeSet<String> = Code::ALL.iter().map(|c| c.type_uri()).collect();
    assert_eq!(types.len(), Code::ALL.len());
    for code in Code::ALL {
        let slug = code.slug();
        assert!(
            slug.split('-')
                .all(|word| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase())),
            "{slug}"
        );
        assert_eq!(code.type_uri(), format!("urn:inklkunn:problem:{slug}"));
        let status = code.status();
        assert!(status.is_client_error() || status.is_server_error() || status.is_redirection());
    }
}

#[test]
fn the_registry_is_complete() {
    // Every variant of the enum is in ALL: the match below fails to compile when one is added and not named,
    // and the count must follow.
    let named = |code: Code| match code {
        Code::NotFound
        | Code::PageNotFound
        | Code::AmbiguousTitle
        | Code::RedirectBroken
        | Code::PageUnreadable
        | Code::InvalidTitle
        | Code::NoSuchView
        | Code::InvalidParameter
        | Code::DiffPathRequired
        | Code::InvalidCursor
        | Code::NotAvailable
        | Code::Unavailable
        | Code::Failed
        | Code::MethodNotAllowed
        | Code::ChangeNotFound => 1,
    };
    assert_eq!(Code::ALL.iter().map(|c| named(*c)).sum::<usize>(), 15);
}

#[test]
fn every_code_has_a_title_a_detail_and_a_hint_in_both_languages_with_no_stray_marker() {
    for code in Code::ALL {
        let body_en = Problem::new(*code)
            .named("parameter", "limit")
            .named("view", "draft")
            .body(en());
        let body_ja = Problem::new(*code)
            .named("parameter", "limit")
            .named("view", "draft")
            .body(ja());
        for (lang, body) in [("en", &body_en), ("ja", &body_ja)] {
            for member in ["title", "detail", "hint"] {
                let words = text(body, member);
                assert!(!words.trim().is_empty(), "{lang} {code:?} {member}");
                assert!(
                    !words.starts_with('['),
                    "a missing message shows as a marker: {lang} {code:?} {member}: {words}"
                );
                assert!(
                    !words.contains('{') && !words.contains('}'),
                    "a placeholder was left unfilled: {lang} {code:?} {member}: {words}"
                );
            }
        }
    }
}

#[test]
fn the_type_and_status_are_the_same_in_every_language_and_the_words_are_not() {
    for code in Code::ALL {
        let a = Problem::new(*code).body(en());
        let b = Problem::new(*code).body(ja());
        assert_eq!(a.get("type"), b.get("type"), "{code:?}");
        assert_eq!(a.get("status"), b.get("status"), "{code:?}");
        for member in ["title", "detail", "hint"] {
            assert_ne!(a.get(member), b.get(member), "{code:?} {member}");
        }
    }
}

#[test]
fn a_problem_has_no_word_of_its_own_every_sentence_is_the_catalogs() {
    for code in Code::ALL {
        let body = Problem::new(*code).body(wrapped());
        for member in ["title", "detail", "hint"] {
            let words = text(&body, member);
            assert!(
                words.starts_with('‹') && words.ends_with('›'),
                "{code:?} {member} is not a catalog message: {words}"
            );
        }
        // And the machine members are not words: they do not change with the language.
        assert_eq!(text(&body, "type"), code.type_uri());
    }
}

#[test]
fn a_named_value_fills_the_sentence_and_is_a_member_of_the_same_name() {
    let body = Problem::new(Code::InvalidParameter)
        .named("parameter", "limit")
        .body(en());
    assert_eq!(text(&body, "parameter"), "limit");
    assert!(text(&body, "detail").contains("limit"), "{body}");
    assert!(text(&body, "hint").contains("limit"), "{body}");
    let body = Problem::new(Code::NoSuchView)
        .named("view", "draft")
        .body(ja());
    assert_eq!(text(&body, "view"), "draft");
    assert!(text(&body, "detail").contains("draft"), "{body}");
}

#[test]
fn a_member_can_never_replace_what_the_standard_names() {
    let body = Problem::new(Code::NotAvailable)
        .member("type", json!("urn:evil"))
        .member("status", json!(200))
        .member("title", json!("fine"))
        .member("feature", json!("history"))
        .body(en());
    assert_eq!(text(&body, "type"), "urn:inklkunn:problem:not-available");
    assert_eq!(body.get("status"), Some(&json!(501)));
    assert_ne!(text(&body, "title"), "fine");
    assert_eq!(text(&body, "feature"), "history");
}

#[test]
fn a_problem_without_extras_has_exactly_the_five_standard_members() {
    let body = Problem::new(Code::NotFound).body(en());
    let names: BTreeSet<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["type", "title", "status", "detail", "hint"])
    );
}

#[test]
fn the_retired_words_do_not_appear_in_any_problem_outside_an_explanation() {
    // The glossary's retired words (seal, commit, rebase, patch, block, ref) are not said in English, and the
    // Japanese terms are not either; `tests/retired_words.rs` holds the catalog to it, this holds the
    // problems' own rendering (placeholders filled).
    let english = ["seal", "commit", "rebase", "patch", "block", "ref"];
    let japanese = [
        "シール",
        "封印",
        "コミット",
        "リベース",
        "パッチ",
        "ブロック",
        "リファレンス",
    ];
    for code in Code::ALL {
        for (t, terms) in [(en(), &english[..]), (ja(), &japanese[..])] {
            let body = Problem::new(*code)
                .named("parameter", "limit")
                .named("view", "draft")
                .body(t);
            for member in ["title", "detail", "hint"] {
                let words = text(&body, member);
                for term in terms {
                    let found = if t.lang() == EN {
                        words
                            .to_lowercase()
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == *term || w.strip_suffix('s') == Some(*term))
                    } else {
                        words.contains(term)
                    };
                    assert!(!found, "{code:?} {member} says `{term}`: {words}");
                }
            }
        }
    }
}
