//! The reason a grant or a restriction carries: one bounded line, as a person wrote it.

use inklkunn_store::record::is_line_or_control;

use crate::governance::holds_only_allowed_characters;

/// The most characters of a reason: a sentence or two. (The store's own bound is in bytes and is larger, so that a
/// reason of this many characters in any script fits it.)
pub const MAX_REASON_CHARS: usize = 500;

/// Why a reason is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReasonFault {
    /// A restriction has a reason, and the account is told it.
    Missing,
    /// The reason is longer than [`MAX_REASON_CHARS`].
    TooLong,
    /// The reason is more than one line, or holds a control, bidirectional or line-separator character.
    NotOneLine,
}

/// A reason, trimmed: `Ok(None)` when it is empty, and a refusal when it is longer than [`MAX_REASON_CHARS`] or is not one
/// line (a control character, a line break). Whether a reason is **required** is the act's: a restriction needs one, a
/// grant does not.
pub fn clean_reason(reason: &str) -> Result<Option<String>, ReasonFault> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Ok(None);
    }
    if reason.chars().any(is_line_or_control) || !holds_only_allowed_characters(reason) {
        return Err(ReasonFault::NotOneLine);
    }
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(ReasonFault::TooLong);
    }
    Ok(Some(reason.to_owned()))
}
