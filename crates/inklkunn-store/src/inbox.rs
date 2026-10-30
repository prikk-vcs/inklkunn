//! Watches and inbox events (RFC 004 D-9; requirement PR-11): what a person follows, and what happened that
//! involves them. **An event carries ids and a kind, never content** (D-9: "events never carry content the
//! recipient could not read"): the page that shows it reads the proposal, through `authorize`, as the
//! recipient. Who gets which event, and when, is RFC 004's handoff T6; this module is the records.

use crate::Result;
use crate::record::{AccountId, InboxEventId, ProposalId, WikiId};

/// What a watch follows (RFC 004 D-9; RD-8).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WatchTarget {
    /// One page, by its path.
    Page(String),
    /// A namespace, by its prefix.
    Namespace(String),
}

impl WatchTarget {
    /// The stored kind and the path or prefix.
    pub fn parts(&self) -> (&'static str, &str) {
        match self {
            WatchTarget::Page(p) => ("page", p),
            WatchTarget::Namespace(p) => ("namespace", p),
        }
    }

    /// Rebuild from the stored kind and text.
    pub fn from_parts(kind: &str, target: &str) -> Option<Self> {
        match kind {
            "page" => Some(WatchTarget::Page(target.to_owned())),
            "namespace" => Some(WatchTarget::Namespace(target.to_owned())),
            _ => None,
        }
    }
}

/// An account watching a page or a namespace of a wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRecord {
    /// Who watches.
    pub account: AccountId,
    /// The wiki.
    pub wiki: WikiId,
    /// What is watched.
    pub target: WatchTarget,
    /// Unix seconds.
    pub created_at: i64,
}

/// What happened (RFC 004 D-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InboxEventKind {
    /// The recipient's proposal was reviewed.
    Reviewed,
    /// The recipient's proposal was accepted.
    Accepted,
    /// The recipient's proposal was declined.
    Declined,
    /// The recipient's proposal needs a revision.
    NeedsRevision,
    /// The recipient's proposal was withdrawn (told to the acceptors who held it).
    Withdrawn,
    /// The recipient's proposal was superseded.
    Superseded,
    /// Someone replied to the recipient's comment.
    Replied,
    /// Someone mentioned the recipient.
    Mentioned,
    /// A page the recipient watches changed.
    WatchedPageChanged,
}

impl InboxEventKind {
    /// Every kind.
    pub const ALL: [InboxEventKind; 9] = [
        InboxEventKind::Reviewed,
        InboxEventKind::Accepted,
        InboxEventKind::Declined,
        InboxEventKind::NeedsRevision,
        InboxEventKind::Withdrawn,
        InboxEventKind::Superseded,
        InboxEventKind::Replied,
        InboxEventKind::Mentioned,
        InboxEventKind::WatchedPageChanged,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            InboxEventKind::Reviewed => "reviewed",
            InboxEventKind::Accepted => "accepted",
            InboxEventKind::Declined => "declined",
            InboxEventKind::NeedsRevision => "needs-revision",
            InboxEventKind::Withdrawn => "withdrawn",
            InboxEventKind::Superseded => "superseded",
            InboxEventKind::Replied => "replied",
            InboxEventKind::Mentioned => "mentioned",
            InboxEventKind::WatchedPageChanged => "watched-page-changed",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        InboxEventKind::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// An event in one account's inbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEvent {
    /// The id.
    pub id: InboxEventId,
    /// The recipient.
    pub account: AccountId,
    /// The wiki.
    pub wiki: WikiId,
    /// The proposal it is about, when it is about one.
    pub proposal: Option<ProposalId>,
    /// What happened.
    pub kind: InboxEventKind,
    /// Who did it, when a person did.
    pub actor: Option<AccountId>,
    /// Unix seconds.
    pub created_at: i64,
    /// When the recipient read it; the only field that ever changes.
    pub read_at: Option<i64>,
}

/// The watch and inbox records. Every implementation passes the same invariant tests (`tests/inbox.rs`).
pub trait InboxStore: Send + Sync {
    /// Follow a page or a namespace. `true` when the watch is new, `false` when the account already had it.
    /// The wiki ([`NotFound`](crate::StoreError::NotFound)) and the account
    /// ([`AccountNotFound`](crate::StoreError::AccountNotFound)) must be there.
    fn watch(&self, watch: &WatchRecord) -> Result<bool>;

    /// Stop following; `true` when the watch existed.
    fn unwatch(&self, account: &AccountId, wiki: &WikiId, target: &WatchTarget) -> Result<bool>;

    /// What the account follows, oldest first.
    fn watches_of(&self, account: &AccountId) -> Result<Vec<WatchRecord>>;

    /// Every watch in the wiki, oldest first: who to tell when a page changes.
    fn watches_in(&self, wiki: &WikiId) -> Result<Vec<WatchRecord>>;

    /// Put an event in an inbox. The id must be new ([`IdTaken`](crate::StoreError::IdTaken)).
    fn push_event(&self, event: &InboxEvent) -> Result<()>;

    /// The account's events, newest first, at most `limit`; only the unread ones when `unread_only`.
    fn inbox(
        &self,
        account: &AccountId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<InboxEvent>>;

    /// Mark one of the account's events read at `at` (the first reading is kept). `true` when the account
    /// has that event, `false` when it does not: an event of someone else's inbox is not found.
    fn mark_read(&self, account: &AccountId, event: &InboxEventId, at: i64) -> Result<bool>;
}
