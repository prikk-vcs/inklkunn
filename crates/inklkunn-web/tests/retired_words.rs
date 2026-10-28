//! RFC 003 D-6 / INC-3 (planeter RFC 012 D-4): the version-control vocabulary does not reach a reader. A
//! message of any language this build can serve must not contain a word `docs/glossary.md` section 3.1
//! retires, **except inside an explanation the reader opens on demand** (a message whose key starts
//! `explain.`, which a template may print only inside a `<details>`), and except for the entries of
//! `tests/retired_words.allow`, each with its reason. The check is shown able to fail below (planted cases).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_web::catalog::{
    Dir, EN_DEF, Entry, JA_DEF, Key, LangDef, Plural, Review, languages, plural_msg,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------- the words, from the glossary

/// The retired words: English forms (matched whole) and Japanese terms (matched anywhere), as section 3.1 of
/// the glossary lists them.
#[derive(Debug, PartialEq, Eq)]
struct Retired {
    english: Vec<String>,
    japanese: Vec<String>,
}

fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split(" | ")
        .map(|c| c.trim().to_owned())
        .collect()
}

fn list(cell: &str) -> Vec<String> {
    cell.split(',')
        .map(|w| w.trim().to_owned())
        .filter(|w| !w.is_empty() && w != "—")
        .collect()
}

/// The rows of the table under `### 3.1` of the glossary.
fn parse_retired(glossary: &str) -> Retired {
    let (mut on, mut header_seen) = (false, false);
    let mut out = Retired {
        english: Vec::new(),
        japanese: Vec::new(),
    };
    for line in glossary.lines() {
        if line.starts_with('#') {
            on = line.starts_with("### 3.1");
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
        if !header_seen {
            continue;
        }
        let c = cells(line);
        assert_eq!(c.len(), 4, "a retired-words row has four cells: {line}");
        out.english.extend(list(c.first().unwrap()));
        out.japanese.extend(list(c.get(1).unwrap()));
    }
    out
}

fn retired() -> Retired {
    parse_retired(&read(root().join("../../docs/glossary.md")))
}

// ---------------------------------------------------------------- the allowlist

#[derive(Debug, PartialEq, Eq)]
struct Allowed {
    key: String,
    term: String,
}

fn parse_allow(src: &str) -> Result<Vec<Allowed>, String> {
    let mut out = Vec::new();
    for line in src.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, " :: ").collect();
        let [key, term, reason] = parts.as_slice() else {
            return Err(format!("needs key :: term :: reason: {line}"));
        };
        if [key, term, reason].iter().any(|p| p.trim().is_empty()) {
            return Err(format!("needs key :: term :: reason: {line}"));
        }
        out.push(Allowed {
            key: key.trim().to_owned(),
            term: term.trim().to_owned(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- matching

/// The text without its `{placeholders}` (their names are code, not words).
fn without_placeholders(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Is `word` in `text` as a whole word, whatever its case? Only ASCII letters, digits and `_` join a word:
/// Japanese text has no spaces, so an English word glued to Japanese text is still a whole word.
fn contains_word(text: &str, word: &str) -> bool {
    let (text, word) = (text.to_lowercase(), word.to_lowercase());
    let is_word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(&word).any(|(at, _)| {
        let before = text.get(..at).and_then(|t| t.chars().next_back());
        let after = text.get(at + word.len()..).and_then(|t| t.chars().next());
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

/// The retired terms one message form uses.
fn terms_in<'a>(text: &str, retired: &'a Retired) -> Vec<&'a str> {
    let text = without_placeholders(text);
    let mut out: Vec<&str> = retired
        .english
        .iter()
        .filter(|w| contains_word(&text, w))
        .map(String::as_str)
        .collect();
    out.extend(
        retired
            .japanese
            .iter()
            .filter(|t| text.contains(t.as_str()))
            .map(String::as_str),
    );
    out
}

/// An explanation is the one place the vocabulary may appear: it is printed only on demand.
fn is_explanation(key: &str) -> bool {
    key.starts_with("explain.")
}

/// (problems, stale allowlist entries) for the given languages.
fn check(langs: &[&LangDef], retired: &Retired, allow: &[Allowed]) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut used = vec![false; allow.len()];
    for lang in langs {
        for e in lang.entries {
            if is_explanation(e.key) {
                continue;
            }
            let mut seen: Vec<&str> = Vec::new();
            for form in [e.one, e.other] {
                for term in terms_in(form, retired) {
                    if seen.contains(&term) {
                        continue;
                    }
                    seen.push(term);
                    match allow.iter().position(|a| a.key == e.key && a.term == term) {
                        Some(i) => {
                            if let Some(u) = used.get_mut(i) {
                                *u = true;
                            }
                        }
                        None => problems.push(format!(
                            "{} `{}` uses the retired {term:?}: {form:?}",
                            lang.tag, e.key
                        )),
                    }
                }
            }
        }
    }
    let stale = allow
        .iter()
        .zip(used)
        .filter(|(_, u)| !u)
        .map(|(a, _)| {
            format!(
                "`{}` :: {:?} is allowed but no message uses it",
                a.key, a.term
            )
        })
        .collect();
    (problems, stale)
}

fn planted(entries: Vec<Entry>) -> &'static LangDef {
    fn one_other(n: u64) -> Plural {
        if n == 1 { Plural::One } else { Plural::Other }
    }
    Box::leak(Box::new(LangDef {
        tag: "planted",
        endonym: "planted",
        dir: Dir::Ltr,
        plural: one_other,
        group_separator: ',',
        review: Review::Preview,
        entries: Box::leak(entries.into_boxed_slice()),
    }))
}

fn message(key: &'static str, text: &'static str) -> Entry {
    Entry {
        key,
        one: text,
        other: text,
    }
}

// ---------------------------------------------------------------- the tests on the real catalogs

#[test]
fn the_glossary_lists_the_retired_words_the_handoff_names() {
    let r = retired();
    for word in ["sealed", "commit", "patch", "block", "ref", "rebase"] {
        assert!(
            r.english.iter().any(|w| w == word),
            "{word} is not retired: {:?}",
            r.english
        );
    }
    for term in [
        "シール",
        "封印",
        "コミット",
        "パッチ",
        "ブロック",
        "リファレンス",
        "リベース",
    ] {
        assert!(
            r.japanese.iter().any(|w| w == term),
            "{term}: {:?}",
            r.japanese
        );
    }
}

#[test]
fn no_message_of_any_language_uses_a_retired_word_and_the_allowlist_has_no_dead_lines() {
    let allow = parse_allow(&read(root().join("tests/retired_words.allow"))).unwrap();
    let (problems, stale) = check(languages(), &retired(), &allow);
    assert_eq!(problems, Vec::<String>::new());
    assert_eq!(stale, Vec::<String>::new());
    // Both languages are looked at, not only English.
    assert_eq!(languages().len(), 2);
}

// ---------------------------------------------------------------- the check can fail

fn found_in(text: &str) -> Vec<String> {
    let l = planted(vec![message(
        "planted.key",
        Box::leak(text.to_owned().into_boxed_str()),
    )]);
    check(&[l], &retired(), &[]).0
}

#[test]
fn every_retired_english_word_is_found_wherever_it_hides() {
    for (text, term) in [
        ("The change was sealed.", "sealed"),
        ("Sealed work", "sealed"),
        ("Unsealed work", "unsealed"),
        ("It seals the history.", "seals"),
        ("Commit your edit", "commit"),
        ("A COMMIT was made", "commit"),
        ("Two commits ago", "commits"),
        ("It was committed", "committed"),
        ("Rebase the proposal", "rebase"),
        ("A patch is one edit", "patch"),
        ("Three patches", "patches"),
        ("Show the block", "block"),
        ("Blocks are shown", "blocks"),
        ("Show refs · {repo}", "refs"),
        ("Go to the ref", "ref"),
        ("(commit)", "commit"),
        ("pre-commit", "commit"),
        ("commitしました", "commit"),
        ("{count} patches", "patches"),
    ] {
        let found = found_in(text);
        assert!(
            found.iter().any(|p| p.contains(&format!("{term:?}"))),
            "{text:?} should be found as {term:?}: {found:?}"
        );
    }
    // Hidden in the other plural form only.
    let l = planted(vec![plural_msg(
        "planted.count",
        "{count} change",
        "{count} sealed changes",
    )]);
    assert_eq!(check(&[l], &retired(), &[]).0.len(), 1);
}

#[test]
fn every_retired_japanese_term_is_found_and_ordinary_words_are_not_mistaken_for_one() {
    for term in [
        "シール",
        "封印",
        "コミット",
        "リベース",
        "パッチ",
        "ブロック",
        "リファレンス",
    ] {
        let found = found_in(&format!("これは{term}です。"));
        assert!(
            found.iter().any(|p| p.contains(&format!("{term:?}"))),
            "{term:?} should be found: {found:?}"
        );
    }
    for ok in [
        "検証済み",
        "署名して履歴に記録済み",
        "変更 {target}",
        "{ref}",
        "メインコンテンツへ移動",
        "コミュニティ",
        "この翻訳はプレビュー版です。",
    ] {
        assert_eq!(found_in(ok), Vec::<String>::new(), "{ok:?}");
    }
}

#[test]
fn ordinary_words_and_placeholders_are_not_mistaken_for_retired_ones() {
    for text in [
        "History",
        "Verification",
        "The commitment is firm",
        "A reference book",
        "Use {refs} here",
        "{patch} is a name",
        "Your prefs are saved",
        "The page is blocked by a firewall",
        "A referee decides",
        "The checks were resealed by hand later",
        "Proofs",
        "No branch has changed.",
        "A recorded change.",
    ] {
        // A whole-word match, not a substring match ("commitment", "blocked", "prefs"), and placeholder names
        // are code.
        assert_eq!(found_in(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn an_explanation_may_use_the_vocabulary_and_nothing_else_may() {
    let text = "A patch is one recorded edit.";
    let r = retired();
    let explanation = planted(vec![message("explain.patch", text)]);
    assert_eq!(check(&[explanation], &r, &[]).0, Vec::<String>::new());
    let ordinary = planted(vec![message("history.patch", text)]);
    assert_eq!(check(&[ordinary], &r, &[]).0.len(), 1);
    let almost = planted(vec![message("explanation.patch", text)]);
    assert_eq!(
        check(&[almost], &r, &[]).0.len(),
        1,
        "the prefix is `explain.`"
    );
}

#[test]
fn an_allowlist_entry_silences_exactly_its_message_and_term_and_a_dead_one_fails() {
    let r = retired();
    let l = planted(vec![
        message("planted.a", "The command is commit."),
        message("planted.b", "Also commit."),
    ]);
    let allow =
        parse_allow("planted.a :: commit :: the name of a command in operator text\n").unwrap();
    let (problems, stale) = check(&[l], &r, &allow);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems.first().unwrap().contains("planted.b"));
    assert!(stale.is_empty());
    // An entry nothing needs.
    let allow = parse_allow("planted.zzz :: ref :: gone\n").unwrap();
    let (_, stale) = check(&[planted(vec![message("planted.ok", "Fine.")])], &r, &allow);
    assert_eq!(stale.len(), 1, "{stale:?}");
    // An entry without a reason, or in another shape, is refused.
    assert!(parse_allow("planted.a :: ref\n").is_err());
    assert!(parse_allow("planted.a :: ref :: \n").is_err());
    assert!(parse_allow("planted.a\n").is_err());
}

#[test]
fn the_glossary_table_is_read_whole() {
    let r = parse_retired(
        "### 3.1 Retired words\n\n| A | B | C | D |\n|---|---|---|---|\n| x, y | ア, イ | z | why |\n| w | — | v | why |\n\n### 3.2 Other\n\n| A | B | C | D |\n|---|---|---|---|\n| nope | ウ | q | q |\n",
    );
    assert_eq!(r.english, ["x", "y", "w"]);
    assert_eq!(r.japanese, ["ア", "イ"]);
    assert_eq!(
        parse_retired("nothing"),
        Retired {
            english: vec![],
            japanese: vec![]
        }
    );
    assert!(
        std::panic::catch_unwind(|| parse_retired("### 3.1 x\n|---|\n| only | three | cells |\n"))
            .is_err()
    );
}

// ---------------------------------------------------------------- explanations are printed on demand only

/// Every place a template names an explanation key outside a `<details>` element.
fn explanations_outside_details(name: &str, template: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut depth = 0_i32;
    let mut at = 0;
    while at < template.len() {
        let rest = template.get(at..).unwrap_or("");
        if rest.starts_with("<details") {
            depth += 1;
            at += "<details".len();
        } else if rest.starts_with("</details>") {
            depth -= 1;
            at += "</details>".len();
        } else if rest.starts_with("Key::Explain") && depth <= 0 {
            let word: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == ':')
                .collect();
            found.push(format!("{name}: {word} is printed outside a <details>"));
            at += word.len();
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    found
}

#[test]
fn an_explanation_key_is_named_only_inside_a_details_element() {
    let mut found = Vec::new();
    let templates = root().join("templates");
    for e in std::fs::read_dir(templates).unwrap().flatten() {
        let p = e.path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        found.extend(explanations_outside_details(&name, &read(p)));
    }
    assert_eq!(found, Vec::<String>::new());
    // The explanation keys that exist all start with the prefix the check relies on.
    for key in Key::ALL {
        assert_eq!(
            is_explanation(key.id()),
            key.variant().starts_with("Explain"),
            "{}: the id prefix `explain.` and the variant prefix `Explain` go together",
            key.id()
        );
    }
}

#[test]
fn the_details_check_can_fail() {
    let outside = "<p>{{ page.get(Key::ExplainSigned) }}</p>";
    assert_eq!(explanations_outside_details("x.html", outside).len(), 1);
    let inside = "<details class=\"explain\"><summary>{{ page.get(Key::X) }}</summary><p>{{ page.get(Key::ExplainSigned) }}</p></details>";
    assert!(explanations_outside_details("x.html", inside).is_empty());
    let after = format!("{inside}<p>{{{{ page.get(Key::ExplainPatch) }}}}</p>");
    let found = explanations_outside_details("x.html", &after);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found.first().unwrap().contains("Key::ExplainPatch"));
    let nested = "<details><details><p>{{ Key::ExplainA }}</p></details><p>{{ Key::ExplainB }}</p></details>";
    assert!(explanations_outside_details("x.html", nested).is_empty());
}

/// Both real languages hold only messages of the catalog; a sanity check that the retired list is not empty
/// and that English is among what is checked.
#[test]
fn the_check_covers_english_and_japanese() {
    assert!(languages().iter().any(|d| std::ptr::eq(*d, &EN_DEF)));
    assert!(languages().iter().any(|d| std::ptr::eq(*d, &JA_DEF)));
}
