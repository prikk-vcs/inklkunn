//! The record of a view's creation (RFC 005 part 2 D-9; RFC 002 D-4; internal design SGN-4, WR-8; handoff 005
//! K-T8): **the recorded act the signer's `create_view` is bound to**, and where an alternative view's origin
//! lives. A view is a prikk branch, which has no place for what it was forked from or what it is for, so the
//! service keeps both here, beside the acceptance and system-act records of the same family: written before the
//! signer is asked, ended once, never changed.
//!
//! The record names the view by its short name (`draft` for `heads/draft`), the view it was forked from the same
//! way, and **the block that view's tip was when the fork was made**. It names the account that created it,
//! because the act is the audited one: the record is the audit of a view's creation.

use crate::record::{AccountId, ViewActId, WikiId};

/// What an alternative view is for (RFC 005 part 2 D-9). The two words are the whole vocabulary; neither says
/// that a view is better or more official than another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewKind {
    /// A working state: it may be merged back or dropped.
    Experimental,
    /// A competing understanding, kept beside the default view.
    Interpretation,
}

impl ViewKind {
    /// Every kind.
    pub const ALL: [ViewKind; 2] = [ViewKind::Experimental, ViewKind::Interpretation];

    /// The stored name, and the word a form carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            ViewKind::Experimental => "experimental",
            ViewKind::Interpretation => "interpretation",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ViewKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// The start of a view's creation, written **before** the signer is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewActRecord {
    /// The id; the signer's `ViewCreationAct` names it.
    pub id: ViewActId,
    /// The wiki.
    pub wiki: WikiId,
    /// The new view's short name (`draft` for `heads/draft`).
    pub name: String,
    /// The short name of the view it is forked from.
    pub forked_from: String,
    /// The block that view's tip was when the fork was made.
    pub from_block: String,
    /// What the view is for.
    pub kind: ViewKind,
    /// Who created it.
    pub created_by: AccountId,
    /// Unix seconds.
    pub created_at: i64,
}

/// How a view's creation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewActOutcome {
    /// The view is there.
    Created,
    /// Nothing was created; `word` is the fixed word of the refusal (never prikk's text).
    Failed {
        /// The word.
        word: String,
    },
}

/// The end of a view's creation, written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewActEnd {
    /// The act.
    pub act: ViewActId,
    /// How it ended.
    pub outcome: ViewActOutcome,
    /// Unix seconds.
    pub at: i64,
}

/// A view-creation act with its end, if it has one. `end` is `None` while it is **open**: an act that was
/// interrupted, which the view may or may not have survived (the repository says).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewActEntry {
    /// The start.
    pub act: ViewActRecord,
    /// The end.
    pub end: Option<ViewActEnd>,
}

impl ViewActEntry {
    /// Does this act stand as the record of where its view came from? One that **failed** took nothing in: the
    /// name is free again, and a view of that name made some other way is not this act's.
    pub fn stands(&self) -> bool {
        !matches!(
            self.end.as_ref().map(|e| &e.outcome),
            Some(ViewActOutcome::Failed { .. })
        )
    }
}
