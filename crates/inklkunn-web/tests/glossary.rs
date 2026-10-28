//! RFC 003 D-6 / INC-3 (planeter RFC 012 D-4): `docs/glossary.md` and the catalogs say the same thing, in both
//! languages. Every row of the glossary's message table (section 2) names a catalog key, and its English and
//! Japanese cells are the catalog's texts character for character; every catalog message has a row; every
//! concept of section 1 marked *in use* really is in a message of both languages. So the page cannot drift
//! from what the pages say, and a catalog edit that skips the glossary fails here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use inklkunn_web::catalog::{EN_DEF, Entry, JA_DEF, Key, msg, plural_msg};

fn glossary() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/glossary.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split(" | ")
        .map(|c| c.trim().to_owned())
        .collect()
}

/// The data rows of the table under the first heading that starts with `heading`, up to the next heading.
fn table(src: &str, heading: &str) -> Vec<Vec<String>> {
    let (mut on, mut header_seen) = (false, false);
    let mut out = Vec::new();
    for line in src.lines() {
        if line.starts_with('#') {
            on = line.starts_with(heading);
            header_seen = false;
            continue;
        }
        if !on || !line.starts_with('|') {
            continue;
        }
        if line.contains("|---") {
            header_seen = true;
            continue;
        }
        if header_seen {
            out.push(cells(line));
        }
    }
    out
}

/// A catalog entry as the glossary writes it: `"text"`, or `"one form" / "other form"`.
fn as_cell(one: &str, other: &str) -> String {
    if one == other {
        format!("\"{one}\"")
    } else {
        format!("\"{one}\" / \"{other}\"")
    }
}

fn entry<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Entry> {
    entries.iter().find(|e| e.key == key)
}

// ---------------------------------------------------------------- section 2: the messages

/// What is wrong between the glossary's message table and the two catalogs.
fn message_disagreements(rows: &[Vec<String>], en: &[Entry], ja: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for r in rows {
        let [key, where_, english, japanese] = r.as_slice() else {
            out.push(format!(
                "a message row has key, where, English and Japanese: {r:?}"
            ));
            continue;
        };
        let key = key.trim_matches('`');
        if where_.is_empty() {
            out.push(format!("`{key}` says nowhere where it appears"));
        }
        if !seen.insert(key.to_owned()) {
            out.push(format!("`{key}` is listed twice"));
        }
        for (name, cell, entries) in [("English", english, en), ("Japanese", japanese, ja)] {
            match entry(entries, key) {
                None => out.push(format!("`{key}` is not a message of the {name} catalog")),
                Some(e) => {
                    let want = as_cell(e.one, e.other);
                    if *cell != want {
                        out.push(format!(
                            "`{key}`: the glossary's {name} says {cell} but the catalog says {want}"
                        ));
                    }
                }
            }
        }
    }
    for e in en.iter().chain(ja) {
        if !seen.contains(e.key) {
            out.push(format!(
                "`{}` is a catalog message with no row in the glossary",
                e.key
            ));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn every_message_row_is_the_catalog_text_in_both_languages_and_every_message_has_a_row() {
    let rows = table(&glossary(), "## 2.");
    assert!(
        rows.len() >= Key::ALL.len(),
        "the message table was not read: {} rows",
        rows.len()
    );
    assert_eq!(
        message_disagreements(&rows, EN_DEF.entries, JA_DEF.entries),
        Vec::<String>::new()
    );
    // Every key is named, in the catalog's order, so the table is the reviewers' sheet.
    let keys: Vec<String> = rows
        .iter()
        .map(|r| r.first().unwrap().trim_matches('`').to_owned())
        .collect();
    let want: Vec<&str> = Key::ALL.iter().map(|k| k.id()).collect();
    assert_eq!(
        keys, want,
        "the glossary lists the messages in the catalog's order"
    );
}

#[test]
fn the_message_check_can_fail() {
    let en = [
        msg("a.one", "Hello {name}"),
        plural_msg("a.two", "{count} thing", "{count} things"),
    ];
    let ja = [
        msg("a.one", "こんにちは {name}"),
        plural_msg("a.two", "{count} 件", "{count} 件"),
    ];
    let row = |k: &str, w: &str, e: &str, j: &str| {
        vec![format!("`{k}`"), w.to_owned(), e.to_owned(), j.to_owned()]
    };
    let good = vec![
        row(
            "a.one",
            "somewhere",
            "\"Hello {name}\"",
            "\"こんにちは {name}\"",
        ),
        row(
            "a.two",
            "elsewhere",
            "\"{count} thing\" / \"{count} things\"",
            "\"{count} 件\"",
        ),
    ];
    assert_eq!(message_disagreements(&good, &en, &ja), Vec::<String>::new());
    // The English words changed in the catalog and not on the page (and the other way round).
    let bad = edit(&good, 0, 2, "\"Hello, {name}\"");
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("the glossary's English says"))
    );
    // The Japanese words.
    let bad = edit(&good, 0, 3, "\"こんばんは {name}\"");
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("the glossary's Japanese says"))
    );
    // A plural written as one form.
    let bad = edit(&good, 1, 2, "\"{count} things\"");
    assert!(!message_disagreements(&bad, &en, &ja).is_empty());
    // A catalog message with no row.
    let bad = vec![good.first().unwrap().clone()];
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("`a.two` is a catalog message with no row"))
    );
    // A row for a message that does not exist, a row listed twice, a row with nowhere, a short row.
    let mut bad = good.clone();
    bad.push(row("a.three", "x", "\"x\"", "\"x\""));
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("`a.three` is not a message"))
    );
    let mut bad = good.clone();
    bad.push(good.first().unwrap().clone());
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("listed twice"))
    );
    let bad = edit(&good, 0, 1, "");
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("nowhere"))
    );
    let bad = vec![vec!["`a.one`".to_owned()]];
    assert!(
        message_disagreements(&bad, &en, &ja)
            .iter()
            .any(|l| l.contains("a message row has"))
    );
}

/// A copy of `rows` with the cell at `r`, `c` replaced.
fn edit(rows: &[Vec<String>], r: usize, c: usize, value: &str) -> Vec<Vec<String>> {
    let mut rows = rows.to_vec();
    *rows.get_mut(r).unwrap().get_mut(c).unwrap() = value.to_owned();
    rows
}

// ---------------------------------------------------------------- section 1: the concepts

/// Is the English `term` in `text` as a whole word or phrase, whatever its case?
fn english_has(text: &str, term: &str) -> bool {
    let (text, term) = (text.to_lowercase(), term.to_lowercase());
    text.match_indices(&term).any(|(at, _)| {
        let before = text.get(..at).and_then(|t| t.chars().next_back());
        let after = text.get(at + term.len()..).and_then(|t| t.chars().next());
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// What is wrong with the concept table: a row not shaped as the page says, a word *in use* that no message
/// of its language holds, a word used for two concepts.
fn concept_disagreements(rows: &[Vec<String>], en: &[Entry], ja: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_en = BTreeSet::new();
    let text = |entries: &[Entry]| {
        entries
            .iter()
            .flat_map(|e| [e.one, e.other])
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (en_text, ja_text) = (text(en), text(ja));
    for r in rows {
        let [concept, english, japanese, in_use, why, status] = r.as_slice() else {
            out.push(format!("a concept row has six cells: {r:?}"));
            continue;
        };
        if [concept, english, japanese, why, status]
            .iter()
            .any(|c| c.is_empty())
        {
            out.push(format!("`{english}` has an empty cell: {r:?}"));
        }
        if !seen_en.insert(english.to_lowercase()) {
            out.push(format!("{english:?} is the word for two concepts"));
        }
        match in_use.as_str() {
            "yes" => {
                if !english_has(&en_text, english) {
                    out.push(format!(
                        "{english:?} is marked in use and no English message has it"
                    ));
                }
                if !ja_text.contains(japanese.as_str()) {
                    out.push(format!(
                        "{japanese:?} is marked in use and no Japanese message has it"
                    ));
                }
            }
            "not yet" => {}
            other => out.push(format!(
                "{english:?}: in use is `yes` or `not yet`, not {other:?}"
            )),
        }
    }
    out
}

#[test]
fn every_concept_marked_in_use_is_in_a_message_of_both_languages() {
    let rows = table(&glossary(), "## 1.");
    assert!(
        rows.len() >= 20,
        "the concept table was not read: {} rows",
        rows.len()
    );
    assert_eq!(
        concept_disagreements(&rows, EN_DEF.entries, JA_DEF.entries),
        Vec::<String>::new()
    );
    // The words the handoff names are on the page.
    let english: Vec<String> = rows
        .iter()
        .map(|r| r.get(1).unwrap().to_lowercase())
        .collect();
    for word in [
        "view",
        "page",
        "wanted page",
        "held for review",
        "change",
        "proposal",
        "verified",
        "verification failed",
        "could not be checked now",
    ] {
        assert!(
            english.iter().any(|w| w == word),
            "the glossary does not fix `{word}`"
        );
    }
    // A retired word is not a word the interface says for a concept.
    for r in &rows {
        for cell in [r.get(1).unwrap(), r.get(2).unwrap()] {
            for bad in ["sealed", "commit", "patch", "block", "rebase"] {
                assert!(
                    !english_has(cell, bad),
                    "the word shown for a concept is retired: {cell}"
                );
            }
        }
    }
}

#[test]
fn the_concept_check_can_fail() {
    let en = [msg("a.one", "Choose a page")];
    let ja = [msg("a.one", "ページを選ぶ")];
    let row = |e: &str, j: &str, u: &str| {
        ["concept", e, j, u, "why", "status"]
            .map(str::to_owned)
            .to_vec()
    };
    assert_eq!(
        concept_disagreements(
            &[
                row("page", "ページ", "yes"),
                row("view", "ビュー", "not yet")
            ],
            &en,
            &ja
        ),
        Vec::<String>::new()
    );
    // In use but absent, in English and in Japanese.
    assert!(
        concept_disagreements(&[row("view", "ページ", "yes")], &en, &ja)
            .iter()
            .any(|l| l.contains("no English message"))
    );
    assert!(
        concept_disagreements(&[row("page", "ビュー", "yes")], &en, &ja)
            .iter()
            .any(|l| l.contains("no Japanese message"))
    );
    // A word is matched whole: `page` is not in `pageant`.
    assert!(!concept_disagreements(&[row("age", "ページ", "yes")], &en, &ja).is_empty());
    // A word for two concepts, an empty cell, a short row, an unknown flag.
    assert!(
        concept_disagreements(
            &[row("page", "ページ", "yes"), row("Page", "x", "not yet")],
            &en,
            &ja
        )
        .iter()
        .any(|l| l.contains("two concepts"))
    );
    assert!(
        concept_disagreements(&[row("", "ページ", "yes")], &en, &ja)
            .iter()
            .any(|l| l.contains("empty cell"))
    );
    assert!(
        concept_disagreements(&[vec!["x".to_owned()]], &en, &ja)
            .iter()
            .any(|l| l.contains("six cells"))
    );
    assert!(
        concept_disagreements(&[row("page", "ページ", "maybe")], &en, &ja)
            .iter()
            .any(|l| l.contains("`yes` or `not yet`"))
    );
}

#[test]
fn the_page_is_read_by_heading_and_stops_at_the_next_one() {
    let src = "## 1. A\n\n| h | h |\n|---|---|\n| a | b |\n\n## 2. B\n\n| h |\n|---|\n| c |\n";
    assert_eq!(table(src, "## 1."), [["a", "b"]]);
    assert_eq!(table(src, "## 2."), [["c"]]);
    assert!(table(src, "## 3.").is_empty());
}
