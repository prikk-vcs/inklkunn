//! What an appeal says: **one bounded text** (RFC 006 D-9).
//!
//! The bound is a comment's ([`MAX_TEXT_BYTES`]). Line breaks are allowed (a grievance is more than a line, as a comment is), and
//! so are tabs; a control character, a bidirectional or other direction character, a line or paragraph separator and the
//! byte-order mark are not (the characters a reason refuses, [`is_valid_reason`](crate::governance::is_valid_reason), less the
//! line break). Surrounding blank space is dropped and every line break is one `\n`.

use inklkunn_signer::recover::text::is_direction_or_format;

/// The most bytes of an appeal: the bound of a comment and of the store's own check.
pub const MAX_TEXT_BYTES: usize = inklkunn_store::governance::MAX_APPEAL_BYTES;

/// Why a text is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextFault {
    /// There is nothing to read.
    Empty,
    /// It is longer than [`MAX_TEXT_BYTES`].
    TooLong,
    /// It holds a character a text of this kind may not: a control, direction or separator character.
    NotText,
}

/// Is `c` allowed in an appeal? A line break and a tab are; every other control character, every direction control, the line and
/// paragraph separators and the byte-order mark are not.
fn is_allowed(c: char) -> bool {
    matches!(c, '\n' | '\t')
        || !(c.is_control()
            || is_direction_or_format(c)
            || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}'))
}

/// The text of an appeal as it is kept: trimmed, with `\r\n` and a lone `\r` read as `\n`; or why it is refused.
pub fn clean_text(text: &str) -> Result<String, TextFault> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.is_empty() {
        return Err(TextFault::Empty);
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(TextFault::TooLong);
    }
    if !text.chars().all(is_allowed) {
        return Err(TextFault::NotText);
    }
    Ok(text.to_owned())
}
