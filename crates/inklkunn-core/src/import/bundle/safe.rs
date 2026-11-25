//! What the bundle plan says about names that came from a bundle: **display-safe and bounded**, and what it never says.
//!
//! Every name a report or a refusal holds (a person, a licence, a page's path, a record's type, a field's name) is written
//! by [`shown`]: control characters, the bidirectional controls and the invisible format characters are written as
//! `\u{…}` escapes, a backslash is doubled, and a name longer than the bound is cut with the number of characters left out. A
//! reader of the report cannot be shown text that reorders or hides what follows it, and no name is long enough to fill a
//! terminal. [`looks_like_an_address`] is the one test that an e-mail address is present (RFC 007 D-3), made to catch the
//! accident of an address in a name or a field, not a person who hides one.

use std::fmt::Write as _;

/// How many characters of a name the plan shows.
pub const NAME_CHARS: usize = 120;

/// Is `c` a character that changes how text around it is drawn or that is not drawn at all: a control character, a
/// bidirectional control or isolate, a zero-width or joining format character, the byte-order mark, a line or paragraph
/// separator?
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
        )
}

/// `name` as a report may show it: escaped as above, and cut at `max` characters (the cut says how many were left out).
pub fn shown_up_to(name: &str, max: usize) -> String {
    let mut out = String::new();
    let mut count = 0_usize;
    let mut left_out = 0_usize;
    for c in name.chars() {
        if count >= max {
            left_out += 1;
            continue;
        }
        count += 1;
        if c == '\\' {
            out.push_str("\\\\");
        } else if is_hidden(c) {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    if left_out > 0 {
        let _ = write!(out, "…(and {left_out} more character(s))");
    }
    out
}

/// `name` as a report may show it, cut at [`NAME_CHARS`].
pub fn shown(name: &str) -> String {
    shown_up_to(name, NAME_CHARS)
}

/// Is there an e-mail address in `text`: a word with one `@` between a local part and a domain that has a dot and ends in two
/// or more letters or digits, or a `mailto:` address? The words are split at whitespace and at the punctuation that surrounds
/// an address in prose.
pub fn looks_like_an_address(text: &str) -> bool {
    if text.to_ascii_lowercase().contains("mailto:") {
        return true;
    }
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '<' | '>' | '(' | ')' | ',' | ';' | '"' | '\'' | '[' | ']'
            )
    })
    .any(word_is_an_address)
}

fn word_is_an_address(word: &str) -> bool {
    let Some((local, domain)) = word.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') || local.contains('/') {
        return false;
    }
    let domain = domain.trim_end_matches('.');
    let Some((host, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    !host.is_empty()
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_alphanumeric())
        && domain
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-'))
}
