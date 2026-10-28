//! RFC 003 D-6 (planeter RFC 012 D-5), the catalog half: checks that fail the build. Each is also shown able to
//! fail (the planted cases below, beside the checks, the way the workspace gates have theirs).
//!
//! * the tables: every key in every language, no orphan or duplicate key, the same placeholders everywhere,
//!   well-formed braces, plural forms that fit the language, no markup, entity or URL in a message (`audit`);
//! * every key is used by a template or by Rust outside the catalog;
//! * no template or product source file contains user-visible literal text outside `tests/literals.allow`;
//! * `|safe` and `autoescape` are never applied to catalog output (or to anything not on the allowlist).
//!
//! The retired words and the glossary are `tests/retired_words.rs` and `tests/glossary.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod allow;
mod rust_scan;
mod template_scan;

use allow::allowlist;
use rust_scan::literal_findings;
use template_scan::template_files_findings;

use std::fs;
use std::path::{Path, PathBuf};

use inklkunn_web::catalog::{
    Dir, EN_DEF, Entry, Key, Kind, LangDef, Plural, Problem, Review, audit, languages,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every file with extension `ext` under `dir`, recursively, sorted.
fn files_under(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The Rust sources that can hold messages and literals: everything under `src/` except the catalog's own
/// tables (`src/catalog/`) and the test files (`tests.rs`, which are separate files by the project's rule).
fn product_sources() -> Vec<PathBuf> {
    let src = root().join("src");
    files_under(&src, "rs")
        .into_iter()
        .filter(|p| {
            let rel = p.strip_prefix(&src).unwrap();
            !rel.starts_with("catalog") && p.file_name().is_some_and(|n| n != "tests.rs")
        })
        .collect()
}

fn templates() -> Vec<PathBuf> {
    files_under(&root().join("templates"), "html")
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

// ---------------------------------------------------------------- the tables

#[test]
fn the_shipped_catalogs_are_complete_consistent_and_free_of_markup() {
    assert_eq!(
        audit(languages()),
        vec![],
        "the shipped catalogs have problems"
    );
}

// A language built for the planted cases: all of English, then one thing wrong.
fn leaked(entries: Vec<Entry>) -> &'static [Entry] {
    Box::leak(entries.into_boxed_slice())
}

fn derived(
    tag: &'static str,
    plural: fn(u64) -> Plural,
    f: impl Fn(&Entry) -> Option<Entry>,
) -> &'static LangDef {
    let entries: Vec<Entry> = EN_DEF.entries.iter().filter_map(f).collect();
    Box::leak(Box::new(LangDef {
        tag,
        endonym: tag,
        dir: Dir::Ltr,
        plural,
        group_separator: ',',
        review: Review::Preview,
        entries: leaked(entries),
    }))
}

fn copy(e: &Entry) -> Entry {
    Entry {
        key: e.key,
        one: e.one,
        other: e.other,
    }
}

fn with(e: &Entry, one: &'static str, other: &'static str) -> Entry {
    Entry {
        key: e.key,
        one,
        other,
    }
}

fn en_plural(n: u64) -> Plural {
    if n == 1 { Plural::One } else { Plural::Other }
}

fn only_other(_: u64) -> Plural {
    Plural::Other
}

/// English with the one message `key` replaced by `text` in both forms.
fn english_with(tag: &'static str, key: &'static str, text: &'static str) -> &'static LangDef {
    derived(tag, en_plural, |e| {
        Some(if e.key == key {
            with(e, text, text)
        } else {
            copy(e)
        })
    })
}

#[test]
fn a_key_missing_in_a_language_is_found() {
    let l = derived("t1", en_plural, |e| {
        (e.key != "not_found.heading").then(|| copy(e))
    });
    let p = audit(&[&EN_DEF, l]);
    assert!(
        p.contains(&Problem::new(Kind::Missing, "t1", "not_found.heading")),
        "{p:?}"
    );
    assert_eq!(p.len(), 1, "and nothing else: {p:?}");
}

#[test]
fn a_key_no_enum_variant_names_is_an_orphan() {
    let mut v: Vec<Entry> = EN_DEF.entries.iter().map(copy).collect();
    v.push(Entry {
        key: "spare.unused",
        one: "x",
        other: "x",
    });
    let l: &'static LangDef = Box::leak(Box::new(LangDef {
        tag: "t2",
        endonym: "t2",
        dir: Dir::Ltr,
        plural: en_plural,
        group_separator: ',',
        review: Review::Preview,
        entries: leaked(v),
    }));
    let p = audit(&[&EN_DEF, l]);
    assert!(
        p.contains(&Problem::new(Kind::Orphan, "t2", "spare.unused")),
        "{p:?}"
    );
}

#[test]
fn a_duplicate_key_is_found() {
    let mut v: Vec<Entry> = EN_DEF.entries.iter().map(copy).collect();
    v.push(copy(EN_DEF.entries.first().unwrap()));
    let l: &'static LangDef = Box::leak(Box::new(LangDef {
        tag: "t3",
        endonym: "t3",
        dir: Dir::Ltr,
        plural: en_plural,
        group_separator: ',',
        review: Review::Preview,
        entries: leaked(v),
    }));
    let p = audit(&[l]);
    assert!(
        p.iter()
            .any(|x| x.kind == Kind::Duplicate && x.lang == "t3"),
        "{p:?}"
    );
}

#[test]
fn a_placeholder_set_that_differs_from_english_is_found() {
    // A placeholder renamed, one dropped, one added.
    for (tag, key, text) in [
        ("t4", "layout.footer", "{product} version {number}"),
        ("t5", "layout.footer", "{product} version"),
        ("t6", "title.not_found", "Not found · {product} · {extra}"),
    ] {
        let l = english_with(tag, key, text);
        let p = audit(&[&EN_DEF, l]);
        assert!(
            p.contains(&Problem::new(Kind::PlaceholdersDiffer, tag, key)),
            "{text}: {p:?}"
        );
    }
    // The order and the words around a placeholder are the translator's.
    let l = english_with("t7", "layout.footer", "version {version} of {product}");
    assert_eq!(audit(&[&EN_DEF, l]), vec![]);
}

#[test]
fn plural_forms_that_do_not_fit_the_language_are_found() {
    // A counted message needs both forms where the language has two…
    let one_form = derived("t8", en_plural, |e| {
        Some(if e.key == "layout.footer" {
            with(e, "{count} things", "{count} things")
        } else {
            copy(e)
        })
    });
    let p = audit(&[&EN_DEF, one_form]);
    assert!(
        p.contains(&Problem::new(
            Kind::PluralFormsDisagree,
            "t8",
            "layout.footer"
        )),
        "{p:?}"
    );
    // …a language whose rule has only `other` must not pretend to have two…
    let two_forms = derived("t9", only_other, |e| {
        Some(if e.key == "layout.footer" {
            with(e, "{count} thing", "{count} things")
        } else {
            copy(e)
        })
    });
    let p = audit(&[two_forms]);
    assert!(
        p.contains(&Problem::new(
            Kind::PluralFormsDisagree,
            "t9",
            "layout.footer"
        )),
        "{p:?}"
    );
    // …and a message without a count has one text.
    let uncounted = derived("t10", en_plural, |e| {
        Some(if e.key == "layout.skip" {
            with(e, "Skip", "Skip over")
        } else {
            copy(e)
        })
    });
    let p = audit(&[uncounted]);
    assert!(
        p.contains(&Problem::new(
            Kind::PluralFormsDisagree,
            "t10",
            "layout.skip"
        )),
        "{p:?}"
    );
    // The right shapes pass.
    let ok = derived("t11", en_plural, |e| {
        Some(if e.key == "layout.footer" {
            with(e, "{count} thing", "{count} things")
        } else {
            copy(e)
        })
    });
    assert_eq!(audit(&[ok]), vec![]);
}

#[test]
fn markup_an_entity_or_a_url_in_a_message_is_found() {
    for (tag, text) in [
        ("t12", "Skip <b>now</b>"),
        ("t13", "Fish &amp; chips"),
        ("t14", "See https://example.invalid/ for more"),
        ("t15", "See ftp://example.invalid/"),
        ("t16", "A > B"),
    ] {
        let l = english_with(tag, "layout.skip", text);
        let p = audit(&[&EN_DEF, l]);
        assert!(
            p.contains(&Problem::new(Kind::Markup, tag, "layout.skip")),
            "{text}: {p:?}"
        );
    }
}

#[test]
fn a_brace_that_is_not_a_well_formed_placeholder_is_found() {
    for (tag, text) in [
        ("t17", "Skip {to"),
        ("t18", "Skip to} content"),
        ("t19", "Skip {} content"),
        ("t20", "Skip {To} content"),
        ("t21", "Skip {{to}} content"),
        ("t22", "Skip { to } content"),
    ] {
        let l = english_with(tag, "layout.skip", text);
        let p = audit(&[l]);
        assert!(
            p.contains(&Problem::new(Kind::Braces, tag, "layout.skip")),
            "{text}: {p:?}"
        );
    }
}

// ---------------------------------------------------------------- use of keys

/// Does `text` name the key variant `Key::<variant>` as a whole word?
fn names_key(text: &str, variant: &str) -> bool {
    let needle = format!("Key::{variant}");
    text.match_indices(&needle).any(|(at, _)| {
        !text
            .get(at + needle.len()..)
            .and_then(|r| r.chars().next())
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn unused_keys(text: &str) -> Vec<&'static str> {
    Key::ALL
        .iter()
        .filter(|k| !names_key(text, k.variant()))
        .map(|k| k.id())
        .collect()
}

fn sources_outside_the_catalog() -> String {
    let mut all = String::new();
    for p in templates().into_iter().chain(product_sources()) {
        all += &read(&p);
        all.push('\n');
    }
    all
}

#[test]
fn every_key_is_used_by_a_template_or_by_rust_outside_the_catalog() {
    let unused = unused_keys(&sources_outside_the_catalog());
    assert!(
        unused.is_empty(),
        "keys no template or Rust path uses (remove them, or use them): {unused:?}"
    );
}

#[test]
fn the_unused_key_check_can_fail_and_matches_whole_names() {
    let all_but_one: String = Key::ALL
        .iter()
        .filter(|k| k.id() != "not_found.body")
        .map(|k| format!("page.get(Key::{});\n", k.variant()))
        .collect();
    assert_eq!(unused_keys(&all_but_one), ["not_found.body"]);
    // `Key::NotFoundBodyLonger` is another name; `Key::NotFoundBody)` and `Key::NotFoundBody ` are this one.
    assert!(names_key("x(Key::NotFoundBody)", "NotFoundBody"));
    assert!(names_key("{{ t.get(Key::NotFoundBody ) }}", "NotFoundBody"));
    assert!(!names_key("Key::NotFoundBodyLonger", "NotFoundBody"));
    assert!(!names_key("Key::NotFoundBody_x", "NotFoundBody"));
    assert!(!names_key("NotFoundBody", "NotFoundBody"));
}

#[test]
fn no_source_file_holds_user_visible_literal_text_outside_the_allowlist() {
    let allow = allowlist();
    let files: Vec<(String, String)> = product_sources()
        .iter()
        .map(|p| (file_name(p), read(p)))
        .collect();
    assert!(
        files.len() >= 8,
        "the sources were not found: {}",
        files.len()
    );
    let found = literal_findings(&allow, &files);
    assert!(
        found.is_empty(),
        "literal text outside the catalog and the allowlist:\n{}",
        found.join("\n")
    );
    // Both checks of the allowlist are made after the templates have been read as well.
    let tpl = template_files_findings(&allow);
    assert!(tpl.is_empty(), "{}", tpl.join("\n"));
    assert_eq!(
        allow.unused(),
        Vec::<String>::new(),
        "allowlist entries nothing matches (remove them)"
    );
}
