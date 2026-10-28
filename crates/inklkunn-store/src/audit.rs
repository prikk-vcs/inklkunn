//! The audit trail's shape (RFC 002 D-9; requirements ID-8; threat model C-12a): **events, never content**.
//! An event says who did what to which resource and how it ended, in a small fixed vocabulary; it never
//! carries page text, a proposal's words, a password or a token. The sink is a trait so that a later release
//! can ship the trail off the box (RFC 002 D-9); the SQLite implementation, which refuses updates and
//! deletes, arrives with the reconciliation (handoff 002 T6).

use std::sync::{Mutex, PoisonError};

use crate::Result;

/// The longest field an event keeps, in characters. A resource name can carry an id a client chose; the
/// trail keeps a bounded, single-line form of it.
pub const FIELD_MAX_CHARS: usize = 200;

/// How a decision or an act ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditOutcome {
    /// Allowed.
    Allowed,
    /// Denied. For a private resource this is also what an absent one produces (RFC 002 D-2).
    Denied,
    /// The decision could not be taken (a store failed); the answer given was *denied*.
    Failed,
}

impl AuditOutcome {
    /// The stored and exported name.
    pub const fn as_str(self) -> &'static str {
        match self {
            AuditOutcome::Allowed => "allowed",
            AuditOutcome::Denied => "denied",
            AuditOutcome::Failed => "failed",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [
            AuditOutcome::Allowed,
            AuditOutcome::Denied,
            AuditOutcome::Failed,
        ]
        .into_iter()
        .find(|o| o.as_str() == s)
    }
}

/// One entry of the trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// Unix seconds.
    pub at: i64,
    /// Who: `anonymous`, `account:<id>` or `system:<label>`.
    pub principal: String,
    /// What was asked: an action name (`accept`) or an operation (`role-grant`).
    pub action: String,
    /// Of what: `<kind>:<wiki id>[:<id>]`. Never a page path or a title.
    pub resource: String,
    /// How it ended.
    pub outcome: AuditOutcome,
    /// A few fixed words, without content. Empty when there is nothing to add.
    pub detail: String,
}

/// A field as the trail keeps it: one line, no control characters, at most [`FIELD_MAX_CHARS`] characters.
pub fn clean_field(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                '?'
            } else {
                c
            }
        })
        .take(FIELD_MAX_CHARS)
        .collect()
}

impl AuditEvent {
    /// An event with every free field made safe to keep and to export as one JSON line.
    pub fn new(
        at: i64,
        principal: &str,
        action: &str,
        resource: &str,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Self {
        AuditEvent {
            at,
            principal: clean_field(principal),
            action: clean_field(action),
            resource: clean_field(resource),
            outcome,
            detail: clean_field(detail),
        }
    }
}

/// Where audit events go. **Appending is the only operation**: the trail is never edited or deleted from
/// the service (RFC 002 D-9). A failure to append is reported, never swallowed; a caller that must not act
/// without a record (an allowed consequential action) treats it as a refusal.
pub trait AuditSink: Send + Sync {
    /// Append one event.
    fn record(&self, event: &AuditEvent) -> Result<()>;
}

/// A trail kept in memory, for tests and for the reference behaviour.
#[derive(Debug, Default)]
pub struct MemoryAudit {
    events: Mutex<Vec<AuditEvent>>,
}

impl MemoryAudit {
    /// An empty trail.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every event so far, oldest first.
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, event: &AuditEvent) -> Result<()> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
