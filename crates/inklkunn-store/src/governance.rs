//! The records of governance and moderation (RFC 006 D-6 - D-10; handoff 006 G-T1; store schema 19): **plain data and the
//! traits behind which they are kept**, SQLite for the service and memory for tests. What a record *means* (who may act,
//! which rung an action needs, what a protection requires) is `inklkunn-core`'s; this module is what is kept.
//!
//! - **Grant decisions** ([`GrantDecisionRecord`], D-6): who gave or took which rung, under which rules, and why.
//!   `role_grants` stays the current state; [`GovernanceStore::record_grant_decision`] changes it and records the decision
//!   in one step, so the two cannot disagree.
//! - **Restrictions** and **temporary protections** (D-7) are *measures*: a chain of events ([`MeasureEvent`]) in which a
//!   measure is set, renewed, lifted or reversed, each later event naming the one it follows. A measure **ends by the
//!   clock** (its `expires_at`), never by a row that something has to write: the reads take the time to ask about.
//! - **Tombstones** ([`TombstoneEvent`], D-8): hide and unhide events of an item and its parts. **The hidden text is never
//!   here**; the store holds the fact of a hiding and its reason category, and the surface that shows the item shows the
//!   tombstone in its place (INV-13).
//! - **Appeals** ([`AppealRecord`], D-9): one bounded text against an action, decided once, with the database refusing a
//!   second open appeal per action per person and a decision by the person who acted or by the appellant. Quotas count
//!   appeals as RFC 004 D-11 counts proposals: [`GovernanceStore::open_appeal_count`] and
//!   [`GovernanceStore::appeals_since`].
//! - **The governance snapshot** ([`GovernanceSnapshot`], D-5) is *derived*: the rules in force at a tip, as read, and the
//!   fault that makes a wiki refuse writes. It is in its own trait, [`GovernanceSnapshotStore`], and is dropped and
//!   rebuilt with the rest of the derived store ([`DerivedStore::clear`](crate::DerivedStore::clear) and
//!   [`DerivedStore::retain_tips`](crate::DerivedStore::retain_tips) drop it too).
//! - **Showing hidden content** (D-8) is not kept here: it is an event of the audit trail ([`crate::AuditEvent`]), which is
//!   already append-only and never carries content.
//!
//! **Written once.** Every authoritative record is append-only (the SQLite store's triggers refuse an `UPDATE`, a `DELETE`
//! and a replacing `INSERT`; this trait has no method that could change or remove one). The one exception is an appeal,
//! which is decided once. A record that is malformed is refused with [`StoreError::Backend`] naming what is wrong, before
//! anything is written; the database's own `CHECK`s say the same again.

use crate::access::Role;
use crate::record::{
    AccountId, AppealId, GrantDecisionId, ProtectionEventId, RestrictionEventId, TombstoneEventId,
    WikiId,
};
use crate::{Result, StoreError};

/// The longest reason, decision reason or moderator's note, in bytes: a proposal's reasons' bound.
pub const MAX_REASON_BYTES: usize = 4096;
/// The longest appeal, in bytes: a comment's bound.
pub const MAX_APPEAL_BYTES: usize = 16 * 1024;
/// The longest protected path or namespace, or item reference (a page's path), in bytes: a path's bound.
pub const MAX_TARGET_BYTES: usize = 1024;
/// The longest one of the two governance files kept in a snapshot, in bytes (a page's bound). A longer file is a fault,
/// not a text kept.
pub const MAX_SNAPSHOT_FILE_BYTES: usize = 1024 * 1024;
/// The most rows a list read returns, whatever is asked for.
pub const MAX_PAGE: usize = 200;

fn refuse<T>(what: &str) -> Result<T> {
    Err(StoreError::Backend(what.to_owned()))
}

/// Is `text` a block id as the records spell one: 1 - 128 characters of lower-case hexadecimal digits? (A block id of
/// prikk 0.58 is 64; the bound leaves room for another digest without a migration.)
pub fn is_block_id(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_block(block: Option<&str>) -> Result<()> {
    match block {
        Some(b) if !is_block_id(b) => {
            refuse("a policy block is a block id in lower-case hexadecimal")
        }
        _ => Ok(()),
    }
}

fn check_text(text: &str, max: usize, what: &str) -> Result<()> {
    if text.is_empty() || text.len() > max {
        return refuse(&format!("{what} is 1 to {max} bytes"));
    }
    Ok(())
}

fn check_optional_text(text: Option<&str>, max: usize, what: &str) -> Result<()> {
    text.map_or(Ok(()), |t| check_text(t, max, what))
}

/// Is `id` the spelling of a record's id or a transition's number: 1 - `max` characters of `0-9a-f-`?
fn is_id_text(id: &str, max: usize) -> bool {
    (1..=max).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

/// Who acted and at which rung: the rung is the one they held **when they acted**, under the rules named beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The account.
    pub account: AccountId,
    /// The rung it held.
    pub rung: Role,
}

// ------------------------------------------------------------------------------------------------ grant decisions

/// One grant or revocation of a rung (RFC 006 D-6), written once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantDecisionRecord {
    /// The id.
    pub id: GrantDecisionId,
    /// The wiki.
    pub wiki: WikiId,
    /// Unix seconds.
    pub at: i64,
    /// Who decided; `None` when the operator did, from the command line (as `role_grants.granted_by` is).
    pub by: Option<Actor>,
    /// The block that set the rules in force, `None` for the defaults (a wiki without governance files).
    pub policy_block: Option<String>,
    /// The person whose rung changed.
    pub target: AccountId,
    /// The rung they held through a grant before, `None` for none.
    pub before: Option<Role>,
    /// The rung they hold through a grant after, `None` for none: a revocation is a decision whose `after` is `None`.
    pub after: Option<Role>,
    /// Why, when it was said (bounded).
    pub reason: Option<String>,
    /// The appeal whose overturn this decision is, when it is one.
    pub appeal: Option<AppealId>,
}

impl GrantDecisionRecord {
    /// Is the record well formed? A decision that changes nothing is not one.
    pub fn validate(&self) -> Result<()> {
        if self.before == self.after {
            return refuse("a grant decision changes the rung it records");
        }
        check_block(self.policy_block.as_deref())?;
        check_optional_text(self.reason.as_deref(), MAX_REASON_BYTES, "a reason")
    }
}

// ------------------------------------------------------------------------------------------------ measures

/// One event in the life of a measure, a restriction or a temporary protection (RFC 006 D-7). **A chain is a line:** the
/// first event is `Set`, every other names the one it follows, and an event is followed by at most one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeasureEvent {
    /// Put in place, with a reason and an end.
    Set,
    /// Put in place again, from a set or renewed event: a new reason and end (renewing is a new action).
    Renewed,
    /// Ended early by a person.
    Lifted,
    /// Ended by the overturn of an appeal, in the same step as the decision.
    Reversed,
}

impl MeasureEvent {
    /// Every event.
    pub const ALL: [MeasureEvent; 4] = [
        MeasureEvent::Set,
        MeasureEvent::Renewed,
        MeasureEvent::Lifted,
        MeasureEvent::Reversed,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            MeasureEvent::Set => "set",
            MeasureEvent::Renewed => "renewed",
            MeasureEvent::Lifted => "lifted",
            MeasureEvent::Reversed => "reversed",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        MeasureEvent::ALL.into_iter().find(|e| e.as_str() == s)
    }

    /// Does the event put a measure in place (it has a reason and an end)?
    pub const fn puts_in_place(self) -> bool {
        matches!(self, MeasureEvent::Set | MeasureEvent::Renewed)
    }
}

/// The shape every measure event shares: what follows what, who may have an appeal, the reason and the end.
fn check_measure(
    event: MeasureEvent,
    has_of_event: bool,
    has_appeal: bool,
    reason: Option<&str>,
    expires_at: Option<i64>,
    at: i64,
    block: Option<&str>,
) -> Result<()> {
    if (event == MeasureEvent::Set) == has_of_event {
        return refuse("every event of a measure but the first names the event it follows");
    }
    if (event == MeasureEvent::Reversed) != has_appeal {
        return refuse("a reversal names the appeal whose overturn it is, and nothing else does");
    }
    check_block(block)?;
    check_optional_text(reason, MAX_REASON_BYTES, "a reason")?;
    if event.puts_in_place() {
        if reason.is_none() {
            return refuse("a measure that is put in place has a reason");
        }
        if !expires_at.is_some_and(|end| end > at) {
            return refuse("a measure that is put in place ends after it begins");
        }
    } else if expires_at.is_some() {
        return refuse("an event that ends a measure has no end of its own");
    }
    Ok(())
}

/// One event of a restriction (RFC 006 D-7): one account's rung in one wiki capped at `observer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictionEvent {
    /// The id.
    pub id: RestrictionEventId,
    /// The wiki.
    pub wiki: WikiId,
    /// Unix seconds.
    pub at: i64,
    /// What happened.
    pub event: MeasureEvent,
    /// The restricted account.
    pub account: AccountId,
    /// Who acted, at which rung.
    pub by: Actor,
    /// The block that set the rules in force, `None` for the defaults.
    pub policy_block: Option<String>,
    /// Why (bounded): required when the restriction is put in place, optional when it is lifted or reversed.
    pub reason: Option<String>,
    /// When it ends (Unix seconds): required when it is put in place, and nothing else has one.
    pub expires_at: Option<i64>,
    /// The `Set` or `Renewed` event this follows: every event but the first.
    pub of_event: Option<RestrictionEventId>,
    /// The appeal whose overturn this is: a reversal, and nothing else.
    pub appeal: Option<AppealId>,
}

impl RestrictionEvent {
    /// Is the event well formed? (The account is never the one who acts.)
    pub fn validate(&self) -> Result<()> {
        if self.by.account == self.account {
            return refuse("nobody acts on their own restriction");
        }
        check_measure(
            self.event,
            self.of_event.is_some(),
            self.appeal.is_some(),
            self.reason.as_deref(),
            self.expires_at,
            self.at,
            self.policy_block.as_deref(),
        )
    }
}

/// What a protection protects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProtectionKind {
    /// One page, by its path.
    Page,
    /// Every page under a namespace. **Which paths a namespace covers is `inklkunn-core`'s question**; the store holds the
    /// text and matches it exactly.
    Namespace,
}

impl ProtectionKind {
    /// Every kind.
    pub const ALL: [ProtectionKind; 2] = [ProtectionKind::Page, ProtectionKind::Namespace];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProtectionKind::Page => "page",
            ProtectionKind::Namespace => "namespace",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ProtectionKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// What a temporary protection requires of a proposal there (RFC 006 D-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectionRequirement {
    /// Every proposal there is reviewed.
    Review,
    /// Acceptance there is restricted to this rung and above.
    Rung(Role),
}

/// One event of a temporary protection of a page or a namespace (RFC 006 D-7). A standing protection is the policy's, in
/// the repository, and is not kept here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionEvent {
    /// The id.
    pub id: ProtectionEventId,
    /// The wiki.
    pub wiki: WikiId,
    /// Unix seconds.
    pub at: i64,
    /// What happened.
    pub event: MeasureEvent,
    /// A page or a namespace.
    pub kind: ProtectionKind,
    /// The page's path, or the namespace, as written (1 - [`MAX_TARGET_BYTES`] bytes). Every event of a chain names the
    /// same one.
    pub target: String,
    /// What it requires: required when it is put in place, and nothing that ends it has one.
    pub requirement: Option<ProtectionRequirement>,
    /// Who acted, at which rung.
    pub by: Actor,
    /// The block that set the rules in force, `None` for the defaults.
    pub policy_block: Option<String>,
    /// Why (bounded): required when it is put in place, optional when it is lifted or reversed.
    pub reason: Option<String>,
    /// When it ends (Unix seconds): required when it is put in place, and nothing else has one.
    pub expires_at: Option<i64>,
    /// The `Set` or `Renewed` event this follows: every event but the first.
    pub of_event: Option<ProtectionEventId>,
    /// The appeal whose overturn this is: a reversal, and nothing else.
    pub appeal: Option<AppealId>,
}

impl ProtectionEvent {
    /// Is the event well formed?
    pub fn validate(&self) -> Result<()> {
        check_text(
            &self.target,
            MAX_TARGET_BYTES,
            "a protected page or namespace",
        )?;
        if self.event.puts_in_place() != self.requirement.is_some() {
            return refuse(
                "a protection that is put in place says what it requires, and nothing that ends one does",
            );
        }
        check_measure(
            self.event,
            self.of_event.is_some(),
            self.appeal.is_some(),
            self.reason.as_deref(),
            self.expires_at,
            self.at,
            self.policy_block.as_deref(),
        )
    }
}

// ------------------------------------------------------------------------------------------------ tombstones

/// What a tombstone hides something *of*: the four things RFC 006 D-8 lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HiddenItemKind {
    /// A comment on a proposal. Its id is the comment's.
    ProposalComment,
    /// A comment in a claim's thread. Its id is the comment's.
    ClaimComment,
    /// A revision of a proposal, accepted or not. Its id is the proposal's and its reference the revision's number.
    ProposalRevision,
    /// A revision of a page. Its id is the block that made it and its reference, when it is one page of a change that
    /// touched several, the page's path.
    PageRevision,
}

impl HiddenItemKind {
    /// Every kind.
    pub const ALL: [HiddenItemKind; 4] = [
        HiddenItemKind::ProposalComment,
        HiddenItemKind::ClaimComment,
        HiddenItemKind::ProposalRevision,
        HiddenItemKind::PageRevision,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            HiddenItemKind::ProposalComment => "proposal-comment",
            HiddenItemKind::ClaimComment => "claim-comment",
            HiddenItemKind::ProposalRevision => "proposal-revision",
            HiddenItemKind::PageRevision => "page-revision",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        HiddenItemKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// An item a tombstone is about: what it is, its identifier, and the part of its address an identifier does not name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HiddenItem {
    /// What kind of thing.
    pub kind: HiddenItemKind,
    /// Its identifier: 1 - 128 characters of `0-9a-f-` (a comment's or a proposal's id, a block's id).
    pub id: String,
    /// The rest of its address: a proposal revision's number (required), a page's path (optional); none for a comment.
    pub reference: Option<String>,
}

impl HiddenItem {
    fn validate(&self) -> Result<()> {
        if !is_id_text(&self.id, 128) {
            return refuse(
                "a hidden item is named by an identifier of lower-case hexadecimal digits and hyphens",
            );
        }
        check_optional_text(
            self.reference.as_deref(),
            MAX_TARGET_BYTES,
            "an item reference",
        )?;
        match (self.kind, self.reference.as_deref()) {
            (HiddenItemKind::ProposalComment | HiddenItemKind::ClaimComment, Some(_)) => {
                refuse("a comment is named by its identifier alone")
            }
            (HiddenItemKind::ProposalRevision, None) => {
                refuse("a proposal's revision is named by the proposal and the revision's number")
            }
            (HiddenItemKind::ProposalRevision, Some(n))
                if !n.bytes().all(|b| b.is_ascii_digit()) =>
            {
                refuse("a revision's number is decimal digits")
            }
            _ => Ok(()),
        }
    }
}

/// Which parts of an item a tombstone hides (RFC 006 D-8). At least one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HiddenParts {
    /// The content (a comment's body; a revision's text).
    pub content: bool,
    /// The summary (a revision's, as shown). A comment has none.
    pub summary: bool,
    /// The author.
    pub author: bool,
}

impl HiddenParts {
    /// Is nothing hidden?
    pub const fn is_empty(self) -> bool {
        !(self.content || self.summary || self.author)
    }

    /// Every part either of the two hides.
    pub const fn union(self, other: HiddenParts) -> HiddenParts {
        HiddenParts {
            content: self.content || other.content,
            summary: self.summary || other.summary,
            author: self.author || other.author,
        }
    }
}

/// Why something was hidden: the closed set of RFC 006 D-8, shown to everyone who can see the item. The stored words are
/// the displayed ones in one word with hyphens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReasonCategory {
    /// *Personal data.*
    PersonalData,
    /// *Harassment.*
    Harassment,
    /// *Unlawful.*
    Unlawful,
    /// *Copyright.*
    Copyright,
    /// *Spam.*
    Spam,
    /// *Other.*
    Other,
}

impl ReasonCategory {
    /// Every category.
    pub const ALL: [ReasonCategory; 6] = [
        ReasonCategory::PersonalData,
        ReasonCategory::Harassment,
        ReasonCategory::Unlawful,
        ReasonCategory::Copyright,
        ReasonCategory::Spam,
        ReasonCategory::Other,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ReasonCategory::PersonalData => "personal-data",
            ReasonCategory::Harassment => "harassment",
            ReasonCategory::Unlawful => "unlawful",
            ReasonCategory::Copyright => "copyright",
            ReasonCategory::Spam => "spam",
            ReasonCategory::Other => "other",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ReasonCategory::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Whether a tombstone event hides or restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TombstoneAct {
    /// Hide.
    Hide,
    /// Restore exactly what a hide hid.
    Unhide,
}

impl TombstoneAct {
    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            TombstoneAct::Hide => "hide",
            TombstoneAct::Unhide => "unhide",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        [TombstoneAct::Hide, TombstoneAct::Unhide]
            .into_iter()
            .find(|a| a.as_str() == s)
    }
}

/// One hide or one unhide (RFC 006 D-8), written once. **No hidden text is in it.** An unhide follows one hide, names the
/// same item and the same parts, and has no category of its own: the log shows the hide's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TombstoneEvent {
    /// The id.
    pub id: TombstoneEventId,
    /// The wiki.
    pub wiki: WikiId,
    /// Unix seconds.
    pub at: i64,
    /// Hide or unhide.
    pub act: TombstoneAct,
    /// What is hidden.
    pub item: HiddenItem,
    /// Which parts of it.
    pub parts: HiddenParts,
    /// Why: required for a hide, absent for an unhide.
    pub category: Option<ReasonCategory>,
    /// A free-text note (bounded), **for the `moderate` rung only**: a surface never shows it to anyone else.
    pub note: Option<String>,
    /// The person the item is about (its author), when it has an account: who may contest the hiding.
    pub affected: Option<AccountId>,
    /// Who acted, at which rung.
    pub by: Actor,
    /// The block that set the rules in force, `None` for the defaults.
    pub policy_block: Option<String>,
    /// The hide this unhides: an unhide, and nothing else.
    pub of_event: Option<TombstoneEventId>,
    /// The appeal whose overturn this is: an unhide, when it is one.
    pub appeal: Option<AppealId>,
    /// For a hide that declined an open proposal in the same act: the decline's own record, the proposal log's position
    /// (`proposal_transitions.seq`). Only a hide of a proposal's revision has one.
    pub declined_transition: Option<i64>,
}

impl TombstoneEvent {
    /// Is the event well formed?
    pub fn validate(&self) -> Result<()> {
        self.item.validate()?;
        if self.parts.is_empty() {
            return refuse("a tombstone hides at least one part");
        }
        if self.parts.summary
            && matches!(
                self.item.kind,
                HiddenItemKind::ProposalComment | HiddenItemKind::ClaimComment
            )
        {
            return refuse("a comment has no summary to hide");
        }
        check_block(self.policy_block.as_deref())?;
        check_optional_text(self.note.as_deref(), MAX_REASON_BYTES, "a note")?;
        if self.declined_transition.is_some() && self.item.kind != HiddenItemKind::ProposalRevision
        {
            return refuse("only the hiding of a proposal's revision declines a proposal");
        }
        match self.act {
            TombstoneAct::Hide => {
                if self.of_event.is_some() || self.appeal.is_some() || self.category.is_none() {
                    return refuse("a hide has a reason category and follows nothing");
                }
            }
            TombstoneAct::Unhide => {
                if self.of_event.is_none()
                    || self.category.is_some()
                    || self.declined_transition.is_some()
                {
                    return refuse("an unhide follows a hide and has no category of its own");
                }
            }
        }
        Ok(())
    }
}

/// The parts of an item that are hidden **now**, from every event of the item's chain, in the order written: the union of
/// the hides that no unhide followed. A pure reading of what the store returns ([`GovernanceStore::tombstone_events_of`]).
pub fn hidden_parts(events: &[TombstoneEvent]) -> HiddenParts {
    events
        .iter()
        .filter(|e| e.act == TombstoneAct::Hide)
        .filter(|hide| {
            !events
                .iter()
                .any(|e| e.act == TombstoneAct::Unhide && e.of_event.as_ref() == Some(&hide.id))
        })
        .fold(HiddenParts::default(), |all, hide| all.union(hide.parts))
}

// ------------------------------------------------------------------------------------------------ appeals

/// What an appeal is against (RFC 006 D-9): the five kinds of action that show *contest this*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppealedKind {
    /// A proposal's decline. Its id is the decline's position in the proposal log (`proposal_transitions.seq`), in decimal.
    Decline,
    /// A tombstone: a hide event's id.
    Tombstone,
    /// A restriction: a `Set` or `Renewed` event's id.
    Restriction,
    /// A temporary protection: a `Set` or `Renewed` event's id.
    Protection,
    /// A grant decision's id.
    Grant,
}

impl AppealedKind {
    /// Every kind.
    pub const ALL: [AppealedKind; 5] = [
        AppealedKind::Decline,
        AppealedKind::Tombstone,
        AppealedKind::Restriction,
        AppealedKind::Protection,
        AppealedKind::Grant,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            AppealedKind::Decline => "decline",
            AppealedKind::Tombstone => "tombstone",
            AppealedKind::Restriction => "restriction",
            AppealedKind::Protection => "protection",
            AppealedKind::Grant => "grant",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        AppealedKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// The action an appeal is against: what kind, and the record's id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppealedAction {
    /// What kind of action.
    pub kind: AppealedKind,
    /// The record's id (up to 36 characters of `0-9a-f-`; for a decline, the log position in decimal).
    pub id: String,
}

/// When and by whom an appeal was decided, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    /// Unix seconds.
    pub at: i64,
    /// Who decided: never the one who took the action, and never the appellant.
    pub by: AccountId,
    /// Why (bounded).
    pub reason: String,
}

/// How an appeal stands: **decided once**, and then for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealState {
    /// Waiting for the `decide_appeal` rung.
    Open,
    /// The action stands.
    Upheld(Decided),
    /// The action was reversed, in the same step, by the record named here (of the appealed kind).
    Overturned(Decided, String),
}

impl AppealState {
    /// The stored name of the state.
    pub const fn as_str(&self) -> &'static str {
        match self {
            AppealState::Open => "open",
            AppealState::Upheld(_) => "upheld",
            AppealState::Overturned(..) => "overturned",
        }
    }
}

/// One appeal (RFC 006 D-9): one bounded text, against one action, by one person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealRecord {
    /// The id.
    pub id: AppealId,
    /// The wiki.
    pub wiki: WikiId,
    /// Unix seconds, when it was filed.
    pub at: i64,
    /// What it is against.
    pub action: AppealedAction,
    /// Who took the action appealed, when an account did: the database refuses their decision. `None` when the operator
    /// did (a grant from the command line).
    pub action_actor: Option<AccountId>,
    /// Who appeals.
    pub appellant: AccountId,
    /// What they say (1 - [`MAX_APPEAL_BYTES`] bytes).
    pub text: String,
    /// How it stands. A new appeal is [`AppealState::Open`].
    pub state: AppealState,
}

impl AppealRecord {
    /// Is the record well formed to be filed? (It is open.)
    pub fn validate(&self) -> Result<()> {
        if !is_id_text(&self.action.id, 36) {
            return refuse("an appealed action is named by a record's id");
        }
        check_text(&self.text, MAX_APPEAL_BYTES, "an appeal")?;
        if self.state != AppealState::Open {
            return refuse("an appeal is filed open");
        }
        Ok(())
    }
}

/// The record that reverses the action an appeal overturned, written **in the same step** as the decision. The record names
/// the appeal and is made by the person who decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reversal {
    /// A grant decision restoring the rung (and `role_grants` with it).
    Grant(GrantDecisionRecord),
    /// A [`MeasureEvent::Reversed`] event of a restriction.
    Restriction(RestrictionEvent),
    /// A [`MeasureEvent::Reversed`] event of a protection.
    Protection(ProtectionEvent),
    /// An unhide.
    Tombstone(TombstoneEvent),
    /// A decline undone by another store's act (the proposal log's position, `proposal_transitions.seq`): the proposals
    /// store made its own transition, and this names it.
    Decline {
        /// The position in the proposal log of the transition that undid the decline.
        transition: i64,
    },
}

/// How an appeal ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealOutcome {
    /// The action stands.
    Upheld,
    /// The action is reversed, by this record, written in the same step.
    Overturned(Box<Reversal>),
}

/// A decision on an appeal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealDecision {
    /// Unix seconds.
    pub at: i64,
    /// Who decided.
    pub by: AccountId,
    /// Why (1 - [`MAX_REASON_BYTES`] bytes).
    pub reason: String,
    /// What it came to.
    pub outcome: AppealOutcome,
}

impl AppealDecision {
    /// Is the decision well formed? A reversal is the decider's own, of its own kind, and the right event.
    pub fn validate(&self) -> Result<()> {
        check_text(&self.reason, MAX_REASON_BYTES, "a decision's reason")?;
        let AppealOutcome::Overturned(reversal) = &self.outcome else {
            return Ok(());
        };
        let by = match &**reversal {
            Reversal::Grant(d) => {
                d.validate()?;
                d.by.as_ref().map(|a| &a.account)
            }
            Reversal::Restriction(e) => {
                e.validate()?;
                if e.event != MeasureEvent::Reversed {
                    return refuse("the reversal of a restriction is a reversed event");
                }
                Some(&e.by.account)
            }
            Reversal::Protection(e) => {
                e.validate()?;
                if e.event != MeasureEvent::Reversed {
                    return refuse("the reversal of a protection is a reversed event");
                }
                Some(&e.by.account)
            }
            Reversal::Tombstone(e) => {
                e.validate()?;
                if e.act != TombstoneAct::Unhide {
                    return refuse("the reversal of a tombstone is an unhide");
                }
                Some(&e.by.account)
            }
            Reversal::Decline { transition } => {
                if *transition < 1 {
                    return refuse("a decline is undone by a position in the proposal log, from 1");
                }
                return Ok(());
            }
        };
        if by != Some(&self.by) {
            return refuse(
                "the record that reverses an action is made by the person who decided the appeal",
            );
        }
        Ok(())
    }
}

impl Reversal {
    /// The kind of action this reverses.
    pub const fn kind(&self) -> AppealedKind {
        match self {
            Reversal::Grant(_) => AppealedKind::Grant,
            Reversal::Restriction(_) => AppealedKind::Restriction,
            Reversal::Protection(_) => AppealedKind::Protection,
            Reversal::Tombstone(_) => AppealedKind::Tombstone,
            Reversal::Decline { .. } => AppealedKind::Decline,
        }
    }

    /// The appeal the record names, and the wiki it is in: what a store checks against the appeal.
    pub(crate) fn names(&self) -> Option<(&AppealId, &WikiId)> {
        match self {
            Reversal::Grant(d) => d.appeal.as_ref().map(|a| (a, &d.wiki)),
            Reversal::Restriction(e) => e.appeal.as_ref().map(|a| (a, &e.wiki)),
            Reversal::Protection(e) => e.appeal.as_ref().map(|a| (a, &e.wiki)),
            Reversal::Tombstone(e) => e.appeal.as_ref().map(|a| (a, &e.wiki)),
            Reversal::Decline { .. } => None,
        }
    }

    /// The text of the record's id, as an appeal keeps it.
    pub(crate) fn record_id(&self) -> String {
        match self {
            Reversal::Grant(d) => d.id.to_string(),
            Reversal::Restriction(e) => e.id.to_string(),
            Reversal::Protection(e) => e.id.to_string(),
            Reversal::Tombstone(e) => e.id.to_string(),
            Reversal::Decline { transition } => transition.to_string(),
        }
    }
}

// ------------------------------------------------------------------------------------------------ the traits

/// The authoritative records of governance and moderation. Every implementation passes the same tests
/// (`tests/governance.rs`). A write is refused as one of: [`StoreError::NotFound`] (a wiki the store does not hold, or an
/// appeal, a record or a proposal-log position that is not there), [`StoreError::AccountNotFound`], [`StoreError::Backend`]
/// (a record that is malformed, or a chain, appeal or decision that breaks a rule the database holds, said in its words),
/// [`StoreError::AlreadyRecorded`] (an id used twice, an event that already has a follower, a second open appeal) and
/// [`StoreError::StateChanged`] (a grant whose rung before is not the one held now; an appeal already decided).
pub trait GovernanceStore: Send + Sync {
    /// Record a grant or a revocation **and apply it** to the wiki's role grants in one step: `decision.before` must be
    /// the rung the account holds through a grant now ([`StoreError::StateChanged`] otherwise, nothing changed), and
    /// afterwards it holds `decision.after`, or none. `role_grants.granted_by` is the decision's account, `granted_at` its
    /// time.
    fn record_grant_decision(&self, decision: &GrantDecisionRecord) -> Result<()>;
    /// The decision with this id.
    fn grant_decision(&self, id: &GrantDecisionId) -> Result<Option<GrantDecisionRecord>>;
    /// At most `limit` decisions (never more than [`MAX_PAGE`], at least one) of the wiki, newest first, about `target`
    /// when one is named.
    fn grant_decisions(
        &self,
        wiki: &WikiId,
        target: Option<&AccountId>,
        limit: usize,
    ) -> Result<Vec<GrantDecisionRecord>>;

    /// Record one event of a restriction. An event after the first follows a `Set` or `Renewed` event of the same account
    /// in the same wiki, no later than itself, that nothing else follows; a reversal names an **open** appeal of a
    /// restriction in the wiki.
    fn record_restriction_event(&self, event: &RestrictionEvent) -> Result<()>;
    /// The event with this id.
    fn restriction_event(&self, id: &RestrictionEventId) -> Result<Option<RestrictionEvent>>;
    /// Every event of every restriction of the account in the wiki, oldest first.
    fn restriction_events(
        &self,
        wiki: &WikiId,
        account: &AccountId,
    ) -> Result<Vec<RestrictionEvent>>;
    /// The restrictions of the account in force at `at`: each a `Set` or `Renewed` event that has begun, has not ended
    /// (`expires_at > at`) and nothing follows (not renewed, lifted or reversed), the one that ends last first. **Expiry
    /// is the clock's**: nothing is written for a restriction to end.
    fn active_restrictions(
        &self,
        wiki: &WikiId,
        account: &AccountId,
        at: i64,
    ) -> Result<Vec<RestrictionEvent>>;

    /// Record one event of a temporary protection, with the rules of [`GovernanceStore::record_restriction_event`] (an
    /// event after the first follows one of the same page or namespace).
    fn record_protection_event(&self, event: &ProtectionEvent) -> Result<()>;
    /// The event with this id.
    fn protection_event(&self, id: &ProtectionEventId) -> Result<Option<ProtectionEvent>>;
    /// At most `limit` events (never more than [`MAX_PAGE`], at least one) of the wiki, newest first.
    fn protection_events(&self, wiki: &WikiId, limit: usize) -> Result<Vec<ProtectionEvent>>;
    /// The protections of the wiki in force at `at` (as [`GovernanceStore::active_restrictions`] reads them), the one that
    /// ends last first.
    fn active_protections(&self, wiki: &WikiId, at: i64) -> Result<Vec<ProtectionEvent>>;
    /// The protections in force at `at` of exactly this page or namespace, as written. Which namespace covers which path
    /// is not the store's question: ask for the namespaces a path may be under and the page.
    fn active_protections_on(
        &self,
        wiki: &WikiId,
        kind: ProtectionKind,
        target: &str,
        at: i64,
    ) -> Result<Vec<ProtectionEvent>>;

    /// Record a hide or an unhide. An unhide follows one hide of the same item and parts, once; a hide that names a
    /// declined transition names one of a proposal of the wiki.
    fn record_tombstone_event(&self, event: &TombstoneEvent) -> Result<()>;
    /// The event with this id.
    fn tombstone_event(&self, id: &TombstoneEventId) -> Result<Option<TombstoneEvent>>;
    /// Every event of the item, oldest first; [`hidden_parts`] reads what is hidden now from them.
    fn tombstone_events_of(&self, wiki: &WikiId, item: &HiddenItem) -> Result<Vec<TombstoneEvent>>;
    /// The wiki's moderation log: at most `limit` events (never more than [`MAX_PAGE`], at least one), newest first,
    /// older than the event `before` when one is named ([`StoreError::NotFound`] when that is not an event of the wiki).
    fn tombstone_log(
        &self,
        wiki: &WikiId,
        before: Option<&TombstoneEventId>,
        limit: usize,
    ) -> Result<Vec<TombstoneEvent>>;

    /// File an appeal. **One is open per action per person**: a second is [`StoreError::AlreadyRecorded`].
    fn file_appeal(&self, appeal: &AppealRecord) -> Result<()>;
    /// The appeal with this id.
    fn appeal(&self, id: &AppealId) -> Result<Option<AppealRecord>>;
    /// Decide an open appeal, once ([`StoreError::StateChanged`] when it is not open any more), and, when it is overturned,
    /// write the record that reverses the action **in the same step**. The decider is neither the appellant nor the one who
    /// took the action; the reversal is of the appealed kind, in the appeal's wiki, names this appeal and is made by the
    /// decider.
    fn decide_appeal(&self, id: &AppealId, decision: &AppealDecision) -> Result<()>;
    /// The wiki's open appeals, oldest first, at most `limit` (never more than [`MAX_PAGE`], at least one).
    fn open_appeals(&self, wiki: &WikiId, limit: usize) -> Result<Vec<AppealRecord>>;
    /// Every appeal the person made in the wiki, newest first.
    fn appeals_by(&self, wiki: &WikiId, appellant: &AccountId) -> Result<Vec<AppealRecord>>;
    /// Every appeal against the action, oldest first.
    fn appeals_about(&self, wiki: &WikiId, action: &AppealedAction) -> Result<Vec<AppealRecord>>;
    /// How many appeals the person has open in the wiki: a quota's current value.
    fn open_appeal_count(&self, wiki: &WikiId, appellant: &AccountId) -> Result<u64>;
    /// When each appeal was filed at or after `since`, oldest first, by one person or, with `None`, by everyone in the
    /// wiki: a rate quota's current value and the time it will reset, as proposals are counted (RFC 004 D-11).
    fn appeals_since(
        &self,
        wiki: &WikiId,
        appellant: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>>;
}

/// The two governance files of a tip as they were read, and what was wrong with them: **derived**, never authoritative
/// (CM-7). One row per wiki and tip: all of it or none.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GovernanceSnapshot {
    /// The block that last set the governance state; `None` when the wiki has no governance files (the defaults).
    pub policy_block: Option<String>,
    /// `governance/constitution.yaml` as read (normalized), when the file is there and could be read.
    pub constitution: Option<String>,
    /// `governance/policy.yaml` as read (normalized), when the file is there and could be read.
    pub policy: Option<String>,
    /// What makes the wiki refuse writes, when something does.
    pub fault: Option<GovernanceFault>,
}

/// A governance file that cannot be read: where, and which rule it broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceFault {
    /// The file's repository path (1 - 256 bytes).
    pub file: String,
    /// The line, from 1, when the rule is about a line.
    pub line: Option<u32>,
    /// The stable name of the rule (1 - 128 bytes).
    pub rule: String,
}

impl GovernanceSnapshot {
    /// Is the snapshot well formed? A file longer than [`MAX_SNAPSHOT_FILE_BYTES`] is a fault, not a text kept.
    pub fn validate(&self) -> Result<()> {
        check_block(self.policy_block.as_deref())?;
        for text in [&self.constitution, &self.policy].into_iter().flatten() {
            if text.len() > MAX_SNAPSHOT_FILE_BYTES {
                return refuse("a governance file kept in a snapshot is at most 1 MiB");
            }
        }
        if let Some(fault) = &self.fault {
            check_text(&fault.file, 256, "a fault's file")?;
            check_text(&fault.rule, 128, "a fault's rule")?;
            if fault.line == Some(0) {
                return refuse("a fault's line is counted from 1");
            }
        }
        Ok(())
    }
}

/// The derived governance snapshot (RFC 006 D-5). It is a trait of its own beside [`crate::DerivedStore`], so that a
/// store that holds the other derived rows need not change to hold it; both stores' `clear` and `retain_tips` drop it
/// with the rest.
pub trait GovernanceSnapshotStore: Send + Sync {
    /// The snapshot of `wiki` at `tip`, if one was stored. **A row that cannot be read back is a miss**, as every derived
    /// row is: the right answer to a damaged row is to derive it again.
    fn governance_snapshot(&self, wiki: &WikiId, tip: &str) -> Result<Option<GovernanceSnapshot>>;
    /// Store the snapshot of `wiki` at `tip`, replacing what was there. A wiki with no governance files is a snapshot of
    /// nothing, stored all the same, so that it is not derived again on every read.
    fn put_governance_snapshot(
        &self,
        wiki: &WikiId,
        tip: &str,
        snapshot: &GovernanceSnapshot,
        built_at: i64,
    ) -> Result<()>;
}
