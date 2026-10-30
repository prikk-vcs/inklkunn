//! Mentions in a comment (RFC 004 D-9: "a reply or a mention"): `@name`, where `name` is a sign-in name.
//!
//! The syntax is deliberately small. A mention is `@` at the start of the text or after a character that is
//! not part of a word, followed by a sign-in name (letters a to z, digits, `.`, `_`, `-`; a trailing `.`, `-`
//! or `_` is punctuation, not part of the name). At most [`MAX_MENTIONS`] are read from one comment, so a
//! comment cannot be used to flood people. Whether a name belongs to anyone who may be told is not decided
//! here, and the sender is never told either way (no oracle for which sign-in names exist).

use inklkunn_store::LoginName;

/// The most mentions one comment may make.
pub const MAX_MENTIONS: usize = 10;

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// The sign-in names a comment mentions, once each, in the order they first appear.
pub fn mentions(text: &str) -> Vec<LoginName> {
    let mut out: Vec<LoginName> = Vec::new();
    let mut before: Option<char> = None;
    for (at, c) in text.char_indices() {
        let after_word = before.is_some_and(|b| b.is_alphanumeric() || b == '_');
        before = Some(c);
        if c != '@' || after_word {
            continue;
        }
        let rest = text.get(at + 1..).unwrap_or("");
        let word: String = rest.chars().take_while(|c| name_char(*c)).collect();
        let name = word.trim_end_matches(['.', '-', '_']);
        if let Ok(login) = LoginName::parse(name)
            && !out.contains(&login)
        {
            out.push(login);
            if out.len() == MAX_MENTIONS {
                break;
            }
        }
    }
    out
}
