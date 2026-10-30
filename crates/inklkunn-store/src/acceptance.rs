//! Acceptances, intents and the lease holder (RFC 004 D-4, D-6, D-7; internal design CON-1, CON-3, CON-6):
//! the records of the act that takes a revision into the history, kept so that a retry finds the record and
//! never seals twice, and so that recovery can tell what was meant from what happened.
//!
//! **Nothing here is ever updated or removed.** An acceptance is written once, when it starts (*pending
//! seal*); how it ended is a second record, written once ([`AcceptanceResult`]). An intent is written once,
//! before the signer is asked; how it ended is a second record, written once ([`IntentCompletion`]). The
//! trait has no operation that changes either, and the SQLite store's triggers refuse an `UPDATE`, a
//! `DELETE` and a replacing `INSERT` even from a connection that bypasses this crate (the audit log's
//! device, version 3). The lease holder is different on purpose: it is a witness of who holds a wiki's write
//! lease *now*, replaced and cleared as the lease changes hands.
//!
//! This module is only the records. The acceptance itself (the lease, the merge, the signer) is RFC 004's
//! handoff T4.

use crate::Result;
use crate::access::Role;
use crate::record::{AcceptanceId, AccountId, IntentId, ProposalId, WikiId};

/// The start of an acceptance: written before the intent and before the signer is asked, so a crash at any
/// later step leaves this behind to be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceRecord {
    /// The id.
    pub id: AcceptanceId,
    /// The proposal.
    pub proposal: ProposalId,
    /// The revision being accepted: with the proposal, the key a retry finds (CON-6).
    pub revision: u32,
    /// The wiki.
    pub wiki: WikiId,
    /// The view the change lands on.
    pub view: String,
    /// Who accepts.
    pub acceptor: AccountId,
    /// The role the acceptor held, shown with the acceptance (PR-12).
    pub acceptor_role: Role,
    /// The policy version the decision was made under (PR-12).
    pub policy_version: String,
    /// Who proposed (kept so the day's accepted bytes per proposer are one query).
    pub proposer: AccountId,
    /// The digest of the merged content, as the acceptance token carries it.
    pub digest: String,
    /// The size of the merged content, for the accepted-bytes quota.
    pub bytes: u64,
    /// Unix seconds.
    pub created_at: i64,
}

/// How an acceptance ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceOutcome {
    /// The change is in the history.
    Sealed {
        /// The block.
        block_id: String,
        /// The patch.
        patch_id: String,
    },
    /// It did not happen; nothing in the history changed. `word` is the fixed outcome (a glossary key), never
    /// prikk's text.
    Failed {
        /// The outcome's word.
        word: String,
    },
}

/// The end of an acceptance, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceResult {
    /// The acceptance.
    pub acceptance: AcceptanceId,
    /// How it ended.
    pub outcome: AcceptanceOutcome,
    /// Unix seconds.
    pub at: i64,
}

/// An acceptance with its end, if it has one. `result` is `None` while it is *pending seal*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceEntry {
    /// The start.
    pub acceptance: AcceptanceRecord,
    /// The end.
    pub result: Option<AcceptanceResult>,
}

/// What [`AcceptanceStore::begin_acceptance`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptanceBegin {
    /// No acceptance of this revision is pending or sealed: the record was written.
    Started,
    /// One is: nothing was written, and this is it. A retry of an accept that has already happened (or that
    /// is still happening) is told so, never sealed again.
    Existing(Box<AcceptanceEntry>),
}

/// What the service meant to seal (CON-3), written before it asks the signer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRecord {
    /// The id.
    pub id: IntentId,
    /// The acceptance it belongs to (one intent per acceptance).
    pub acceptance: AcceptanceId,
    /// The wiki.
    pub wiki: WikiId,
    /// The view.
    pub view: String,
    /// The block the change was made on top of.
    pub base_block: String,
    /// The digest of the message that will be sealed.
    pub message_digest: String,
    /// Unix seconds.
    pub created_at: i64,
}

/// How an intent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntentEnd {
    /// The change was sealed (by the acceptance, or by recovery completing it).
    Completed,
    /// The acceptance stopped before anything was committed: the intent never became a change.
    Abandoned,
    /// Recovery found a queued patch that did not match and discarded it.
    Discarded,
}

impl IntentEnd {
    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            IntentEnd::Completed => "completed",
            IntentEnd::Abandoned => "abandoned",
            IntentEnd::Discarded => "discarded",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [
            IntentEnd::Completed,
            IntentEnd::Abandoned,
            IntentEnd::Discarded,
        ]
        .into_iter()
        .find(|v| v.as_str() == s)
    }
}

/// The end of an intent, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentCompletion {
    /// The intent.
    pub intent: IntentId,
    /// How it ended.
    pub end: IntentEnd,
    /// Unix seconds.
    pub at: i64,
}

/// An intent with its end, if it has one. `completion` is `None` while it is **open**: the thing recovery
/// looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentEntry {
    /// The intent.
    pub intent: IntentRecord,
    /// The end.
    pub completion: Option<IntentCompletion>,
}

/// Who holds a wiki's write lease now (CON-1): a witness recovery reads, not the lock itself (the lock is an
/// in-process mutex over the per-wiki `flock`, RFC 004 D-4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseHolder {
    /// The wiki.
    pub wiki: WikiId,
    /// Who holds it: an instance name the service chose for the process, never a person's name.
    pub holder: String,
    /// What the lease is held for (`acceptance`, `export`, `recovery`, …), as a short fixed word.
    pub purpose: String,
    /// The acceptance it is held for, when it is.
    pub acceptance: Option<AcceptanceId>,
    /// Unix seconds.
    pub since: i64,
}

/// The acceptance, intent and lease records. Every implementation passes the same invariant tests
/// (`tests/acceptance_records.rs`). Errors are the plain facts of [`crate::StoreError`]; a record that is
/// written once and exists under its key is [`AlreadyRecorded`](crate::StoreError::AlreadyRecorded), and
/// nothing changes.
pub trait AcceptanceStore: Send + Sync {
    /// Start an acceptance of one revision, **unless one is pending or sealed** (CON-6): then nothing is
    /// written and that one is returned. An acceptance that failed does not stand in the way: the proposal
    /// stayed open and the acceptor may try again, which writes a new record. The proposal must be there
    /// ([`NotFound`](crate::StoreError::NotFound)).
    fn begin_acceptance(&self, acceptance: &AcceptanceRecord) -> Result<AcceptanceBegin>;

    /// Record how an acceptance ended. Once: a second end is `AlreadyRecorded`.
    fn finish_acceptance(&self, result: &AcceptanceResult) -> Result<()>;

    /// The acceptances of one revision, oldest first, each with its end.
    fn acceptances_of(&self, proposal: &ProposalId, revision: u32) -> Result<Vec<AcceptanceEntry>>;

    /// The bytes accepted for `proposer` in the wiki at or after `since` (acceptances that are pending or
    /// sealed; a failed one took nothing in), as `(when, bytes)`, oldest first: the accepted-bytes quota's
    /// current value.
    fn accepted_bytes_since(
        &self,
        wiki: &WikiId,
        proposer: &AccountId,
        since: i64,
    ) -> Result<Vec<(i64, u64)>>;

    /// Write an intent. One per acceptance: a second is `AlreadyRecorded`.
    fn record_intent(&self, intent: &IntentRecord) -> Result<()>;

    /// Record how an intent ended. Once: a second end is `AlreadyRecorded`.
    fn complete_intent(&self, completion: &IntentCompletion) -> Result<()>;

    /// The wiki's intents that have not ended, oldest first: what recovery starts from.
    fn open_intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>>;

    /// Every intent of the wiki with its end, oldest first.
    fn intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>>;

    /// Who holds the wiki's write lease, if anyone is recorded.
    fn lease_holder(&self, wiki: &WikiId) -> Result<Option<LeaseHolder>>;

    /// Record `holder` as the wiki's lease holder, replacing whoever was recorded, and return that one: the
    /// caller (the lease, or recovery) decides what a replaced record means.
    fn record_lease_holder(&self, holder: &LeaseHolder) -> Result<Option<LeaseHolder>>;

    /// Clear the record if `holder` is the recorded holder; `true` when it was. A stale release by a
    /// process that no longer holds the lease clears nothing.
    fn release_lease_holder(&self, wiki: &WikiId, holder: &str) -> Result<bool>;
}
