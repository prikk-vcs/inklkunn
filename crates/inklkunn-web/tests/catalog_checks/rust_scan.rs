//! Literal text in Rust sources: a small lexer that reads the string literals a person could see, and the check
//! of them against the allowlist; with the planted cases that show both can fail.

use super::allow::{Allow, none, parse_allowlist};

/// Does this literal look like something a person could read: three letters in a row, or a letter that is not
/// ASCII? (Identifiers, header names and the like are on the allowlist, with their reasons.)
pub(crate) fn looks_like_words(lit: &str) -> bool {
    let mut run = 0;
    for c in lit.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
            if c.is_alphabetic() {
                return true;
            }
        }
    }
    false
}

/// The string literals of a Rust source that could reach a person, with their line numbers: outside comments
/// and attributes, in ordinary, raw and byte strings, with character literals and lifetimes told apart from
/// them.
pub(crate) fn rust_literals(src: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = src.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let (mut i, mut line, mut attr) = (0_usize, 1_usize, 0_usize);
    while let Some(c) = at(i) {
        let before_is_ident = i.checked_sub(1).and_then(at).is_some_and(is_ident);
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            '/' if at(i + 1) == Some('/') => {
                while at(i).is_some_and(|c| c != '\n') {
                    i += 1;
                }
            }
            '/' if at(i + 1) == Some('*') => {
                let mut depth = 1;
                i += 2;
                while depth > 0 {
                    match (at(i), at(i + 1)) {
                        (None, _) => break,
                        (Some('*'), Some('/')) => {
                            depth -= 1;
                            i += 2;
                        }
                        (Some('/'), Some('*')) => {
                            depth += 1;
                            i += 2;
                        }
                        (Some('\n'), _) => {
                            line += 1;
                            i += 1;
                        }
                        _ => i += 1,
                    }
                }
            }
            '#' if attr == 0
                && (at(i + 1) == Some('[')
                    || (at(i + 1) == Some('!') && at(i + 2) == Some('['))) =>
            {
                attr = 1;
                i += if at(i + 1) == Some('!') { 3 } else { 2 };
            }
            '[' if attr > 0 => {
                attr += 1;
                i += 1;
            }
            ']' if attr > 0 => {
                attr -= 1;
                i += 1;
            }
            '"' | 'r' | 'b' | 'c' if !before_is_ident => {
                // A string starts here if the prefix letters (b, c, r, br, cr) are followed by hashes and a quote.
                let mut j = i;
                if matches!(at(j), Some('b' | 'c')) {
                    j += 1;
                }
                let raw = at(j) == Some('r');
                if raw {
                    j += 1;
                }
                let mut hashes = 0;
                while raw && at(j) == Some('#') {
                    hashes += 1;
                    j += 1;
                }
                if at(j) != Some('"') {
                    i += 1;
                    continue;
                }
                let start_line = line;
                j += 1;
                let mut lit = String::new();
                loop {
                    match at(j) {
                        None => break,
                        Some('"') if raw => {
                            let closes = (1..=hashes).all(|k| at(j + k) == Some('#'));
                            if closes {
                                j += 1 + hashes;
                                break;
                            }
                            lit.push('"');
                            j += 1;
                        }
                        Some('"') => {
                            j += 1;
                            break;
                        }
                        Some('\\') if !raw => {
                            j += 1;
                            match at(j) {
                                Some('n') => lit.push('\n'),
                                Some('t') => lit.push('\t'),
                                Some('r') => lit.push('\r'),
                                Some('0') => lit.push('\0'),
                                Some('\n') => {
                                    // A line continuation skips the newline and the indentation after it.
                                    line += 1;
                                    while at(j + 1).is_some_and(|c| c.is_whitespace()) {
                                        if at(j + 1) == Some('\n') {
                                            line += 1;
                                        }
                                        j += 1;
                                    }
                                }
                                Some('u') => {
                                    // `\u{…}`: skipped, it is one character of no interest here.
                                    while at(j).is_some_and(|c| c != '}') {
                                        j += 1;
                                    }
                                    lit.push('?');
                                }
                                Some('x') => {
                                    j += 2;
                                    lit.push('?');
                                }
                                Some(other) => lit.push(other),
                                None => {}
                            }
                            j += 1;
                        }
                        Some(other) => {
                            if other == '\n' {
                                line += 1;
                            }
                            lit.push(other);
                            j += 1;
                        }
                    }
                }
                if attr == 0 && looks_like_words(&lit) {
                    out.push((start_line, lit));
                }
                i = j;
            }
            '\'' => {
                if at(i + 1) == Some('\\') {
                    // A character literal with an escape: skip to its closing quote.
                    i += 2;
                    while at(i).is_some_and(|c| c != '\'') {
                        i += 1;
                    }
                    i += 1;
                } else if at(i + 2) == Some('\'') {
                    i += 3;
                } else {
                    // A lifetime or a label.
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

pub(crate) fn literal_findings(allow: &Allow, files: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for (name, src) in files {
        if allow.scoped(name) {
            continue;
        }
        for (line, lit) in rust_literals(src) {
            if !allow.literal_ok(name, &lit) {
                found.push(format!("{name}:{line}: {lit:?}"));
            }
        }
    }
    found
}

#[test]
fn the_rust_scanner_finds_prose_and_spares_comments_attributes_and_tokens_of_other_kinds() {
    let src = r##"
// "A comment says Hello there" and /* "so does this" */
/* block with "Another sentence" and /* nested "inner prose" */ more */
#![allow(missing_docs)]
#[template(path = "base.html")]
#[doc = "Documentation prose"]
fn f<'a>(x: &'a str) -> &'a str {
    respond("Something went wrong here");
    let c = '"';
    let d = '\'';
    let e = '\n';
    let raw = r#"A raw "quoted" sentence"#;
    let bytes = b"Bytes prose";
    let short = "ok";
    let token = "x-1";
    let multi = "first line \
                 second line";
    let esc = "tab\there";
    'label: loop { break 'label; }
    "A sentence after a label"
}
"##;
    let lits: Vec<String> = rust_literals(src).into_iter().map(|(_, l)| l).collect();
    assert_eq!(
        lits,
        [
            "Something went wrong here",
            "A raw \"quoted\" sentence",
            "Bytes prose",
            "first line second line",
            "tab\there",
            "A sentence after a label",
        ],
        "a comment, an attribute, a character, a lifetime and a token are not prose"
    );
    // The line of a literal is its first line.
    let lines: Vec<usize> = rust_literals("fn f() {\n\n  g(\"Hello there\");\n}\n")
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert_eq!(lines, [3]);
    // Non-ASCII words are found too, and a char literal of a quote does not open a string.
    assert_eq!(
        rust_literals("let a = \"こんにちは\"; let q = '\"'; let b = \"ok\";").len(),
        1
    );
}

#[test]
fn a_planted_literal_in_a_source_file_is_found_and_the_allowlist_excuses_exactly_what_it_names() {
    let files = vec![
        (
            "a.rs".to_owned(),
            "fn f() { respond(\"Something went wrong\"); g(\"/x/lang\"); h(\"text/css\"); }"
                .to_owned(),
        ),
        (
            "b.rs".to_owned(),
            "fn f() { respond(\"Something went wrong\"); }".to_owned(),
        ),
        (
            "c.rs".to_owned(),
            "fn f() { respond(\"Anything at all\"); }".to_owned(),
        ),
    ];
    // Nothing excused: every prose literal is found, with its file.
    let found = literal_findings(&none(), &files);
    assert_eq!(found.len(), 5, "{found:?}");
    assert!(found.iter().any(|l| l.starts_with("b.rs:1:")), "{found:?}");
    // An exact entry excuses one literal in one file, a `*` file one literal anywhere, a prefix a family, and a
    // scope a whole file.
    let allow = parse_allowlist(
        "exact :: a.rs :: Something went wrong :: a test\n\
         prefix :: * :: / :: a path\n\
         exact :: * :: text/css :: a media type\n\
         scope :: c.rs :: * :: not user-facing\n",
    );
    let found = literal_findings(&allow, &files);
    assert_eq!(found, ["b.rs:1: \"Something went wrong\""], "{found:?}");
    assert_eq!(allow.unused(), Vec::<String>::new());
    // An entry nothing needs is reported.
    let allow = parse_allowlist(
        "exact :: a.rs :: Gone :: nothing says this any more\nscope :: z.rs :: * :: no such file\n",
    );
    literal_findings(&allow, &files);
    assert_eq!(allow.unused(), ["exact::a.rs::Gone", "scope::z.rs"]);
}
