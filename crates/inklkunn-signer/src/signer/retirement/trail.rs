//! What the audit trail says of a retirement (handoff 004, X2).
//!
//! The key record is authenticated, but nothing in the key directory says which version of it is the newest: a
//! directory restored from a backup taken while a retirement was unfinished carries a `retiring` state for a key whose
//! retirement was finished since. `abandon-retirement` must not adopt that key again. The audit trail is the one record
//! outside the key directory that is appended to and never edited (RFC 002 D-9), so the commands that **make** a
//! retirement's record (`rotate --retire-old`, `retire`, `finish-retirement`) write the retired key's id into the first
//! words of the detail of the event that says they ended, in one fixed form, and abandon reads it back here, strictly.
//!
//! The form is `retirement recorded: key <id>` at the very start of the detail (an event keeps at most 200 characters, `FIELD_MAX_CHARS`,
//! and a key id has at most 128), followed by nothing or by `; `
//! and more words. An event of the three actions, in the outcome *allowed*, for this wiki, whose detail begins so, says
//! that key's retirement was finished. Every other event (the "begun" ones, the failed ones, an older build's: its
//! detail has no such form) says nothing, and a key that no event names is **not known to be finished**, which for
//! abandon means: go on, as the key record says.

use inklkunn_store::{AuditLog, AuditOutcome, StoreError, WikiId};

/// The action of a rotation (`rotate`).
pub const ACTION_ROTATE: &str = "keys-rotate";
/// The action of the retirement of a superseded key (`retire`, X1).
pub const ACTION_RETIRE: &str = "keys-retire";
/// The action of `finish-retirement`.
pub const ACTION_FINISH_RETIREMENT: &str = "keys-finish-retirement";

/// What a detail says first when the retirement of a key ended with its record made.
const RECORDED: &str = "retirement recorded: key ";

/// The detail of an event that says the retirement of `key` ended with its record made, then `rest` (words for a
/// person; nothing is read from them).
pub fn recorded_detail(key: &str, rest: &str) -> String {
    if rest.is_empty() {
        format!("{RECORDED}{key}")
    } else {
        format!("{RECORDED}{key}; {rest}")
    }
}

/// The key a detail says was retired, if it begins in the fixed form: the id is a key id as prikk accepts one (ASCII
/// letters, digits, `-` and `_`, at most 128) and ends the detail or is followed by `; `.
pub fn recorded_key(detail: &str) -> Option<&str> {
    let rest = detail.strip_prefix(RECORDED)?;
    let id = match rest.split_once("; ") {
        Some((id, _)) => id,
        None => rest,
    };
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    valid.then_some(id)
}

/// Does the audit trail say that the retirement of `key` in `wiki` was finished (its record made)? The whole trail is
/// read, oldest first, a page at a time. An error is the trail not being readable, and is no answer.
pub fn retirement_finished_in(
    trail: &dyn AuditLog,
    wiki: &WikiId,
    key: &str,
) -> Result<bool, StoreError> {
    const PAGE: usize = 500;
    let resource = format!("wiki:{wiki}");
    let mut after = 0;
    loop {
        let page = trail.read(after, PAGE)?;
        let Some(last) = page.last() else {
            return Ok(false);
        };
        after = last.seq;
        for record in &page {
            let event = &record.event;
            if event.outcome == AuditOutcome::Allowed
                && event.resource == resource
                && matches!(
                    event.action.as_str(),
                    ACTION_ROTATE | ACTION_RETIRE | ACTION_FINISH_RETIREMENT
                )
                && recorded_key(&event.detail) == Some(key)
            {
                return Ok(true);
            }
        }
    }
}
