//! The audit trail's shape (RFC 002 D-9; requirements ID-8; threat model C-12a): **events, never content**.
//! An event says who did what to which resource and how it ended, in a small fixed vocabulary; it never
//! carries page text, a proposal's words, a password or a token. The sink is a trait so that a later release
//! can ship the trail off the box (RFC 002 D-9). The SQLite implementation keeps it in a table that the
//! database itself refuses to update or delete from, and [`export_jsonl`] writes it out one object a line.

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
    /// An observation worth keeping that is neither a grant nor a refusal: what a reconciliation found.
    Reported,
}

impl AuditOutcome {
    /// The stored and exported name.
    pub const fn as_str(self) -> &'static str {
        match self {
            AuditOutcome::Allowed => "allowed",
            AuditOutcome::Denied => "denied",
            AuditOutcome::Failed => "failed",
            AuditOutcome::Reported => "reported",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [
            AuditOutcome::Allowed,
            AuditOutcome::Denied,
            AuditOutcome::Failed,
            AuditOutcome::Reported,
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

impl AuditEvent {
    /// The event with every free field made safe again. A sink applies this to what it is given, so an
    /// event built field by field (the fields are public) cannot put a line break or an unbounded string
    /// in the trail.
    pub fn cleaned(&self) -> AuditEvent {
        AuditEvent::new(
            self.at,
            &self.principal,
            &self.action,
            &self.resource,
            self.outcome,
            &self.detail,
        )
    }
}

/// An event as the trail holds it: with its position, which only ever grows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// The position in the trail, from 1, never reused.
    pub seq: i64,
    /// The event.
    pub event: AuditEvent,
}

/// The trail read back. Reading is the only other operation: there is no update and no delete.
pub trait AuditLog: AuditSink {
    /// Up to `limit` events with a position after `after_seq`, oldest first.
    fn read(&self, after_seq: i64, limit: usize) -> Result<Vec<AuditRecord>>;
}

/// One line of the JSON Lines export (RFC 002 D-9): the keys are fixed and in this order.
#[derive(serde::Serialize)]
struct Line<'a> {
    seq: i64,
    at: i64,
    principal: &'a str,
    action: &'a str,
    resource: &'a str,
    outcome: &'a str,
    detail: &'a str,
}

/// Write the whole trail to `out` as JSON Lines, oldest first, one event per line, and return how many were
/// written. It pages through the log, so a long trail is never held in memory at once.
pub fn export_jsonl(log: &dyn AuditLog, out: &mut dyn std::io::Write) -> Result<u64> {
    const PAGE: usize = 500;
    let mut after = 0;
    let mut written = 0u64;
    loop {
        let page = log.read(after, PAGE)?;
        if page.is_empty() {
            return Ok(written);
        }
        for r in &page {
            let line = serde_json::to_string(&Line {
                seq: r.seq,
                at: r.event.at,
                principal: &r.event.principal,
                action: &r.event.action,
                resource: &r.event.resource,
                outcome: r.event.outcome.as_str(),
                detail: &r.event.detail,
            })
            .map_err(|e| crate::StoreError::Backend(e.to_string()))?;
            out.write_all(line.as_bytes())
                .and_then(|()| out.write_all(b"\n"))
                .map_err(|e| {
                    crate::StoreError::Backend(format!(
                        "the export could not be written ({})",
                        e.kind()
                    ))
                })?;
            written += 1;
        }
        after = page.last().map_or(after, |r| r.seq);
    }
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
            .push(event.cleaned());
        Ok(())
    }
}

impl AuditLog for MemoryAudit {
    fn read(&self, after_seq: i64, limit: usize) -> Result<Vec<AuditRecord>> {
        let events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(events
            .iter()
            .enumerate()
            .map(|(i, e)| AuditRecord {
                seq: i64::try_from(i + 1).unwrap_or(i64::MAX),
                event: e.clone(),
            })
            .filter(|r| r.seq > after_seq)
            .take(limit)
            .collect())
    }
}

#[cfg(test)]
mod tests;
