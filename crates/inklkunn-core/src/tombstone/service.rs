//! The acts of the `moderate` rung over a tombstone: **hide**, **unhide**, **show hidden content**; and the moderation log
//! (RFC 006 D-8, D-12; handoff G-T4; threat model C-14f, C-15f).
//!
//! **The order of every act is the order of every write** (as the people's acts, [`crate::people`]): the wiki must be one the
//! person may read (an absent one and a forbidden one are one answer), they must be an account, the rules of the wiki must be
//! readable, the act is **authorized** (which records the decision with the rung and the block), the item is looked up, the
//! pure rules say whether the act may be done, **the audit event** (`rung=…; policy=…`, never content) is written, and only
//! then the record. A record that fails after its audit event gets a `failed` follow-up, so the trail never says an act
//! happened that did not.
//!
//! **These acts are web-only** (RFC 006 D-12): nothing in the read API reaches this service.
//!
//! - **Hide** hides parts of one item: a comment's body or author, a revision's content, summary or author. **A proposal's
//!   revision that was accepted and the page's revision are one content**, recorded once on the page's revision, so one act
//!   hides every place it is shown. **The content of a view's tip cannot be hidden** ([`ModerationError::TipContent`]: revert
//!   or remove it first, by proposal; the moderation form offers the rollback). **Hiding the content of the latest revision of
//!   an open proposal also declines it**, in the same act, with the category as its reason, because a proposal nobody can read
//!   cannot be reviewed; the tombstone event names the decline's own record.
//! - **Unhide** restores exactly what one hide hid. A proposal the hide declined stays declined: that is a decision, and
//!   reopening one is the proposer's (a new proposal).
//! - **Show hidden content** gives the hidden parts back to the `moderate` rung, after an event in the audit trail (one line,
//!   never content). Nobody else sees hidden content through inklkunn (Q-5).

use std::sync::Arc;

use inklkunn_store::{
    AccountId, AccountStore, Actor, AppealedAction, AppealedKind, AuditEvent, AuditOutcome,
    AuditSink, ClaimThreadStore, CommentId, GovernanceStore, HiddenItem, HiddenItemKind,
    HiddenParts, ProposalChange, ProposalId, ProposalOp, ProposalState, ProposalStore,
    ProposalTransition, ReasonCategory, Role, StoreError, TombstoneAct, TombstoneActId,
    TombstoneEvent, TombstoneEventId, WikiRecord,
};

use super::model::{Hidden, Mark};
use super::reader::{Tombstones, page_item};
use crate::appeal::AppealMark;
use crate::authorize::{Action, Authorizer, Holding, Principal, Resource, RestrictionNotice};
use crate::governance::{ReadFault, Rules, is_valid_reason};
use crate::notify::Notifier;
use crate::read::{ChangeInfo, LaterHolders, ReadError, ReadService, TipCheck};

/// The most characters of a moderator's note: a sentence or two, as a reason is.
pub const MAX_NOTE_CHARS: usize = crate::governance::MAX_REASON_CHARS;
/// The most files *show hidden content* lists for a change.
pub const MAX_REVEALED_FILES: usize = crate::read::MAX_REVEALED_FILES;

/// The item an act is about, **as a surface names it**: the address a person was looking at when they asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A comment on a proposal.
    ProposalComment {
        /// The proposal.
        proposal: ProposalId,
        /// The comment.
        comment: CommentId,
    },
    /// A comment in a claim's thread.
    ClaimComment {
        /// The claim's id.
        claim: String,
        /// The comment.
        comment: CommentId,
    },
    /// A revision of a proposal.
    ProposalRevision {
        /// The proposal.
        proposal: ProposalId,
        /// The revision's number, from 1.
        seq: u32,
    },
    /// A revision of a page: a change of a view's history.
    PageRevision {
        /// The change's block id.
        block: String,
        /// The view whose history it is read in (`None`: the default view).
        view: Option<String>,
    },
}

/// Why an act was refused. Every variant is one plain fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationError {
    /// No such wiki, or the person may not see it: one answer for both (RFC 002 D-2).
    NotFound,
    /// The person may not moderate: they are not an account, or their rung does not include it.
    NotAllowed,
    /// The person is restricted, and without the restriction they could have done this.
    Restricted(RestrictionNotice),
    /// A governance file of this wiki cannot be read, so every write refuses (C-3d).
    RulesUnreadable(ReadFault),
    /// There is no such item in this wiki (or the person may not see the proposal it belongs to).
    NoSuchItem,
    /// No part was chosen.
    NoPart,
    /// A part the item does not have: a comment's summary, or the summary of a revision that is not the proposal's latest
    /// (the summary is the latest revision's: the store keeps no other).
    NoSuchPart,
    /// Everything chosen is hidden already.
    AlreadyHidden,
    /// The content of a view's tip cannot be hidden: **revert or remove it first, by proposal**, and the rollback is offered.
    TipContent {
        /// The view whose tip holds it (its short name), when the check could say.
        view: Option<String>,
    },
    /// The change has an operation whose path could not be resolved, so it may have written a file that no tip can be compared
    /// with: **its content cannot be hidden** (L-2), as for a change too large to compare. Its summary and its author can be.
    TipUnresolved,
    /// The note is not one line of at most [`MAX_NOTE_CHARS`] characters.
    Note,
    /// The hiding to unhide or to show is not there, is not in force, or is another wiki's.
    NoSuchHiding,
    /// The records changed while the act was being made; look again.
    Changed,
    /// The facts or the records could not be had now (the text is for the operator): nothing was changed.
    Unavailable(String),
}

impl std::fmt::Display for ModerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModerationError::NotFound => write!(f, "no such wiki"),
            ModerationError::NotAllowed => write!(f, "not allowed"),
            ModerationError::Restricted(n) => {
                write!(f, "restricted until {}: {}", n.until, n.reason)
            }
            ModerationError::RulesUnreadable(fault) => {
                write!(f, "the rules of this wiki cannot be read: {fault}")
            }
            ModerationError::NoSuchItem => write!(f, "no such item"),
            ModerationError::NoPart => write!(f, "no part chosen"),
            ModerationError::NoSuchPart => write!(f, "the item has no such part"),
            ModerationError::AlreadyHidden => write!(f, "already hidden"),
            ModerationError::TipContent { view } => write!(
                f,
                "the content of a view's tip cannot be hidden ({}): revert or remove it first",
                view.as_deref().unwrap_or("unchecked")
            ),
            ModerationError::TipUnresolved => write!(
                f,
                "the content of a change with an operation on an unresolved path cannot be hidden: it cannot be compared with the views' tips"
            ),
            ModerationError::Note => write!(f, "the note is not one short line"),
            ModerationError::NoSuchHiding => write!(f, "no such hiding in force"),
            ModerationError::Changed => write!(f, "the records changed meanwhile"),
            ModerationError::Unavailable(why) => write!(f, "could not be done now: {why}"),
        }
    }
}

impl std::error::Error for ModerationError {}

fn unavailable(e: impl std::fmt::Display) -> ModerationError {
    ModerationError::Unavailable(e.to_string())
}

fn read_failure(e: ReadError) -> ModerationError {
    match e {
        ReadError::NoSuchView(_) | ReadError::InvalidTitle(_) => ModerationError::NoSuchItem,
        other => unavailable(other),
    }
}

fn store_failure(e: StoreError) -> ModerationError {
    match e {
        StoreError::StateChanged | StoreError::AlreadyRecorded => ModerationError::Changed,
        StoreError::NotFound => ModerationError::NoSuchHiding,
        other => unavailable(other),
    }
}

/// A decline that a hide made in the same act (H-6): **the typed mark of a decline by moderation.** The proposal log has no field
/// for it, so it is the moderation record that names the decline (`declined_transition`, the step's position in the log); this
/// is the step itself and the category of the hide, never the moderator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModerationDecline {
    /// The decline, as the proposal's log holds it. Its actor is the moderator and its reason is words written when it was made:
    /// **a surface draws neither**, it says *declined by moderation* and the category.
    pub step: ProposalTransition,
    /// Why it was hidden, which is why it was declined.
    pub category: ReasonCategory,
}

/// A hide that was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HideMade {
    /// The record of the hide (the first of the act's records).
    pub id: TombstoneEventId,
    /// The act: every record of this hide carries it, an overturn reverses exactly them, and the audit trail names it.
    pub act: TombstoneActId,
    /// The proposal the act declined with it, when it did.
    pub declined: Option<ProposalId>,
}

/// An unhide that was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnhideMade {
    /// The record of the unhide.
    pub id: TombstoneEventId,
    /// The act of the unhide, which the audit trail names.
    pub act: TombstoneActId,
    /// The hide it followed.
    pub of: TombstoneEventId,
}

/// One row of the moderation log: **the item, the category, the rung and the date, never the hidden content and never a
/// name.** The note is carried only for a reader who holds the `moderate` rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRow {
    /// The record.
    pub id: TombstoneEventId,
    /// Unix seconds.
    pub at: i64,
    /// A hide, or an unhide.
    pub act: TombstoneAct,
    /// What it is about, by identifier.
    pub item: HiddenItem,
    /// Which parts.
    pub parts: HiddenParts,
    /// Why: the hide's category (an unhide shows the one of the hide it follows).
    pub category: Option<ReasonCategory>,
    /// The rung that acted.
    pub rung: Role,
    /// The block that set the rules in force when it was done, `None` for the defaults (RFC 006 D-5: every decision shows the rung
    /// and the policy block).
    pub policy_block: Option<String>,
    /// The note, **for the `moderate` rung only**.
    pub note: Option<String>,
    /// For a hide that is in force now: it can still be unhidden.
    pub in_force: bool,
    /// The proposal this hide declined in the same act, when it did.
    pub declined: bool,
    /// For a hide: **that it was appealed, and how that ended**, and nothing else (RFC 006 D-9: an appeal names a person and a
    /// grievance, so the log carries no text and no name).
    pub appeal: Option<AppealMark>,
    /// For an unhide: it is the overturn of an appeal.
    pub by_appeal: bool,
}

/// What *show hidden content* gives back: the hidden parts of one item, and only those.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Revealed {
    /// A comment's body, or a revision's summary line.
    pub text: Option<String>,
    /// The author's name, when the author is hidden.
    pub author: Option<String>,
    /// The files of a revision, when its content is hidden: path and text.
    pub files: Vec<RevealedFile>,
    /// The note the moderator wrote, for the `moderate` rung.
    pub note: Option<String>,
}

/// One file of a revision shown by *show hidden content*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealedFile {
    /// Its path.
    pub path: String,
    /// Its text.
    pub text: String,
}

/// What every act has settled before it looks at the item.
struct Acting {
    account: AccountId,
    holding: Holding,
    rules: Arc<Rules>,
}

impl Acting {
    fn rung(&self) -> Option<Role> {
        self.holding.rung
    }
}

/// The item as found: its canonical address, the person it is about, what it has, and what the act would also do.
struct Found {
    canonical: HiddenItem,
    affected: Option<AccountId>,
    /// An open proposal whose latest revision this item is: hiding its content declines it.
    open_latest: Option<ProposalId>,
    /// The proposal's latest revision, for a proposal's revision.
    is_latest: bool,
    /// The item is a comment: it has no summary.
    comment: bool,
}

/// The moderation of the wikis.
#[derive(Clone)]
pub struct ModerationService {
    authorizer: Authorizer,
    governance: Arc<dyn GovernanceStore>,
    proposals: Arc<dyn ProposalStore>,
    claims: Arc<dyn ClaimThreadStore>,
    accounts: Arc<dyn AccountStore>,
    read: Arc<ReadService>,
    tombstones: Arc<Tombstones>,
    audit: Arc<dyn AuditSink>,
    notifier: Option<Arc<Notifier>>,
}

impl std::fmt::Debug for ModerationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ModerationService")
    }
}

/// The words of the category the decline of a hidden proposal carries as its reason.
fn category_words(category: ReasonCategory) -> &'static str {
    match category {
        ReasonCategory::PersonalData => "personal data",
        ReasonCategory::Harassment => "harassment",
        ReasonCategory::Unlawful => "unlawful",
        ReasonCategory::Copyright => "copyright",
        ReasonCategory::Spam => "spam",
        ReasonCategory::Other => "other",
    }
}

impl ModerationService {
    /// Over the moderation records, the proposal and claim-thread records the items live in, the read service (for the changes
    /// of a history and the check of a tip), the reader of what is hidden, and the audit trail. The clock is the
    /// authorizer's.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        authorizer: Authorizer,
        governance: Arc<dyn GovernanceStore>,
        proposals: Arc<dyn ProposalStore>,
        claims: Arc<dyn ClaimThreadStore>,
        accounts: Arc<dyn AccountStore>,
        read: Arc<ReadService>,
        tombstones: Arc<Tombstones>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        ModerationService {
            authorizer,
            governance,
            proposals,
            claims,
            accounts,
            read,
            tombstones,
            audit,
            notifier: None,
        }
    }

    /// The same, telling a proposer that a proposal of theirs was declined by the act that hid it.
    #[must_use]
    pub fn with_notifier(mut self, notifier: Arc<Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    fn readable(&self, principal: &Principal, wiki: &WikiRecord) -> Result<(), ModerationError> {
        let resource = Resource::Wiki {
            wiki: wiki.id.clone(),
        };
        if self
            .authorizer
            .authorize(principal, Action::Read, &resource)
            .is_allowed()
        {
            Ok(())
        } else {
            Err(ModerationError::NotFound)
        }
    }

    fn begin(&self, principal: &Principal, wiki: &WikiRecord) -> Result<Acting, ModerationError> {
        self.readable(principal, wiki)?;
        let Some(account) = principal.account_id().cloned() else {
            return Err(ModerationError::NotAllowed);
        };
        let rules = self.authorizer.rules(&wiki.id).map_err(unavailable)?;
        if let Some(fault) = rules.fault() {
            return Err(ModerationError::RulesUnreadable(fault.clone()));
        }
        let holding = self
            .authorizer
            .holding(principal, &wiki.id)
            .map_err(unavailable)?;
        Ok(Acting {
            account,
            holding,
            rules,
        })
    }

    /// Authorize `moderate` on the wiki, **recording the decision** (rung and block), or say why not.
    fn authorize(&self, principal: &Principal, wiki: &WikiRecord) -> Result<(), ModerationError> {
        let resource = Resource::Wiki {
            wiki: wiki.id.clone(),
        };
        if self
            .authorizer
            .authorize(principal, Action::Moderate, &resource)
            .is_allowed()
        {
            return Ok(());
        }
        Err(self
            .authorizer
            .restriction_refusal(principal, Action::Moderate, &resource)
            .map_or(ModerationError::NotAllowed, ModerationError::Restricted))
    }

    /// **The declines of a proposal that a hide made** (H-6): for each hide of the proposal that declined it in the same act, the
    /// decline's step in the proposal's log and the hide's category. A page that tells why a proposal was declined asks this first,
    /// and draws *declined by moderation* and the category for a step it finds here, **never the moderator** (D-8). Nothing is
    /// authorized or recorded: what it returns is what the proposal's page already says, without the name.
    pub fn declines_by_moderation(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalId,
    ) -> Result<Vec<ModerationDecline>, ModerationError> {
        let events = self
            .governance
            .tombstone_events_of_id(
                &wiki.id,
                HiddenItemKind::ProposalRevision,
                proposal.as_str(),
            )
            .map_err(unavailable)?;
        let mut out = Vec::new();
        for hide in events.iter().filter(|e| e.act == TombstoneAct::Hide) {
            let (Some(position), Some(category)) = (hide.declined_transition, hide.category) else {
                continue;
            };
            if let Some((of, step)) = self
                .proposals
                .transition_at(&wiki.id, position)
                .map_err(unavailable)?
                && of == *proposal
            {
                out.push(ModerationDecline { step, category });
            }
        }
        Ok(out)
    }

    /// May the principal moderate here? **Draws nothing and records nothing**: what a page asks to offer *hide* and *show
    /// hidden content* to the rung that may. The act asks again, as every act does.
    pub fn may_moderate(&self, principal: &Principal, wiki: &WikiRecord) -> bool {
        let (Ok(holding), Ok(rules)) = (
            self.authorizer.holding(principal, &wiki.id),
            self.authorizer.rules(&wiki.id),
        ) else {
            return false;
        };
        rules.fault().is_none()
            && holding
                .rung
                .is_some_and(|rung| rung >= rules.policy().rungs().moderate())
    }

    /// **The event of the act in the audit trail**, written before the record is: `rung=…; policy=…` as every decision is,
    /// and what the act is about, never content. The resource is the act: `tombstone:<wiki>:<act id>` (A-2).
    #[allow(clippy::too_many_arguments)]
    fn audit_act(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        acting: &Acting,
        operation: &str,
        act: &TombstoneActId,
        outcome: AuditOutcome,
        detail: &str,
    ) -> Result<(), ModerationError> {
        let event = AuditEvent::new(
            self.authorizer.now(),
            &principal.audit_name(),
            operation,
            &format!("tombstone:{}:{act}", wiki.id),
            outcome,
            &format!(
                "rung={}; policy={}; {detail}",
                acting.rung().map_or("none", Role::as_str),
                acting.rules.short_block()
            ),
        );
        self.audit.record(&event).map_err(unavailable)
    }

    // ----------------------------------------------------------------------------------------------- the item

    /// Look the item up: it is there, which record it is kept under, whom it is about, and what hiding it would also do.
    fn find(&self, wiki: &WikiRecord, item: &Item) -> Result<Found, ModerationError> {
        match item {
            Item::ProposalComment { proposal, comment } => {
                let record = self.proposal_of(wiki, proposal)?;
                let comments = self
                    .proposals
                    .comments(proposal, None)
                    .map_err(unavailable)?;
                let found = comments
                    .iter()
                    .find(|c| c.id == *comment)
                    .ok_or(ModerationError::NoSuchItem)?;
                let _ = record;
                Ok(Found {
                    canonical: super::reader::comment_item(comment),
                    affected: Some(found.author.clone()),
                    open_latest: None,
                    is_latest: false,
                    comment: true,
                })
            }
            Item::ClaimComment { claim, comment } => {
                let mut after: Option<CommentId> = None;
                loop {
                    let page = self
                        .claims
                        .claim_comments(
                            &wiki.id,
                            claim,
                            after.as_ref(),
                            inklkunn_store::claim_thread::MAX_PAGE,
                        )
                        .map_err(|e| match e {
                            StoreError::NotFound => ModerationError::NoSuchItem,
                            other => unavailable(other),
                        })?;
                    if let Some(row) = page.rows.iter().find(|r| r.comment.id == *comment) {
                        return Ok(Found {
                            canonical: super::reader::claim_comment_item(comment),
                            affected: Some(row.comment.author.clone()),
                            open_latest: None,
                            is_latest: false,
                            comment: true,
                        });
                    }
                    match page.next {
                        Some(next) => after = Some(next),
                        None => return Err(ModerationError::NoSuchItem),
                    }
                }
            }
            Item::ProposalRevision { proposal, seq } => {
                let record = self.proposal_of(wiki, proposal)?;
                let revisions = self
                    .proposals
                    .revisions(proposal, None)
                    .map_err(unavailable)?;
                let latest = revisions.last().map(|r| r.seq);
                if !revisions.iter().any(|r| r.seq == *seq) {
                    return Err(ModerationError::NoSuchItem);
                }
                let is_latest = latest == Some(*seq);
                let open = matches!(
                    record.state,
                    ProposalState::HeldForReview | ProposalState::NeedsRevision
                );
                Ok(Found {
                    canonical: self
                        .tombstones
                        .item_of_revision(proposal, *seq)
                        .map_err(unavailable)?,
                    affected: Some(record.author),
                    open_latest: (open && is_latest).then(|| proposal.clone()),
                    is_latest,
                    comment: false,
                })
            }
            Item::PageRevision { block, view } => {
                let info = self
                    .read
                    .reveal_change(wiki, view.as_deref(), block)
                    .map_err(read_failure)?
                    .ok_or(ModerationError::NoSuchItem)?
                    .info;
                let affected = self.proposer_of(wiki, &info);
                Ok(Found {
                    canonical: page_item(block),
                    affected,
                    open_latest: None,
                    is_latest: true,
                    comment: false,
                })
            }
        }
    }

    fn proposal_of(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<inklkunn_store::ProposalRecord, ModerationError> {
        self.proposals
            .proposal(id, None)
            .map_err(unavailable)?
            .filter(|p| p.wiki == wiki.id)
            .ok_or(ModerationError::NoSuchItem)
    }

    /// The account that proposed a change, when its message names a proposal of this wiki.
    fn proposer_of(&self, wiki: &WikiRecord, info: &ChangeInfo) -> Option<AccountId> {
        let id = info.proposal.as_deref().and_then(ProposalId::parse)?;
        self.proposal_of(wiki, &id).ok().map(|p| p.author)
    }

    // ------------------------------------------------------------------------------------------------- hide

    /// **Hide** parts of an item, with a reason category and an optional note (for the `moderate` rung only).
    pub fn hide(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        item: &Item,
        parts: HiddenParts,
        category: ReasonCategory,
        note: &str,
    ) -> Result<HideMade, ModerationError> {
        let acting = self.begin(principal, wiki)?;
        self.authorize(principal, wiki)?;
        let Some(rung) = acting.rung() else {
            return Err(ModerationError::NotAllowed);
        };
        if parts.is_empty() {
            return Err(ModerationError::NoPart);
        }
        let note = note.trim();
        let note = if note.is_empty() {
            None
        } else if is_valid_reason(note) {
            Some(note.to_owned())
        } else {
            return Err(ModerationError::Note);
        };
        let found = self.find(wiki, item)?;
        if parts.summary
            && (found.comment
                || (matches!(item, Item::ProposalRevision { .. }) && !found.is_latest))
        {
            return Err(ModerationError::NoSuchPart);
        }
        let hidden = self.hidden_of(wiki, &found.canonical, item)?;
        let already = hidden.parts();
        let fresh = HiddenParts {
            content: parts.content && !already.content,
            summary: parts.summary && !already.summary,
            author: parts.author && !already.author,
        };
        if fresh.is_empty() {
            return Err(ModerationError::AlreadyHidden);
        }
        // The content of a view's tip cannot be hidden, in any view (D-8).
        if parts.content && found.canonical.kind == HiddenItemKind::PageRevision {
            match self
                .read
                .tip_check(wiki, &found.canonical.id)
                .map_err(read_failure)?
            {
                // Allowed: the change is not a tip's text. (The lines of it that a tip still has are told by the form, not refused.)
                TipCheck::Clear | TipCheck::Survives { .. } => {}
                TipCheck::Holds { view } => {
                    return Err(ModerationError::TipContent { view: Some(view) });
                }
                TipCheck::Unchecked => return Err(ModerationError::TipContent { view: None }),
                TipCheck::Unresolved => return Err(ModerationError::TipUnresolved),
            }
        }
        let declines = if parts.content {
            found.open_latest.clone()
        } else {
            None
        };
        let id = TombstoneEventId::new();
        let act = TombstoneActId::new();
        let detail = format!(
            "hide; item={}:{}; parts={}; category={}{}",
            found.canonical.kind.as_str(),
            found.canonical.id,
            parts_words(parts),
            category.as_str(),
            declines
                .as_ref()
                .map_or(String::new(), |p| format!("; declines={p}"))
        );
        self.audit_act(
            principal,
            wiki,
            &acting,
            "tombstone-hide",
            &act,
            AuditOutcome::Allowed,
            &detail,
        )?;
        let done = self.record_hide(
            wiki,
            &acting,
            rung,
            &id,
            &act,
            &found,
            parts,
            category,
            note,
            declines.as_ref(),
        );
        match done {
            Ok(()) => {
                // The reads that follow a hide of a content ask who wrote each path: build that index toward the tip now, once
                // (best effort: a read that finds it short builds on, and says *hidden* for what it could not reach).
                if parts.content {
                    let _ = self.read.warm_writes(wiki);
                }
                Ok(HideMade {
                    id,
                    act,
                    declined: declines,
                })
            }
            Err(e) => {
                // The act did not happen: the trail says so, and the person is told it did not.
                let _ = self.audit_act(
                    principal,
                    wiki,
                    &acting,
                    "tombstone-hide",
                    &act,
                    AuditOutcome::Failed,
                    &format!("{detail}; the record failed"),
                );
                Err(e)
            }
        }
    }

    /// What is hidden of the item now, whichever way it was named.
    fn hidden_of(
        &self,
        wiki: &WikiRecord,
        _canonical: &HiddenItem,
        item: &Item,
    ) -> Result<Hidden, ModerationError> {
        let read = match item {
            // A proposal's revision answers with the page revision it became as well.
            Item::ProposalRevision { proposal, seq } => {
                self.tombstones.proposal_revision(&wiki.id, proposal, *seq)
            }
            Item::PageRevision { block, .. } => self.tombstones.of_change(&wiki.id, block),
            Item::ProposalComment { comment, .. } => {
                self.tombstones.proposal_comment(&wiki.id, comment)
            }
            Item::ClaimComment { comment, .. } => self.tombstones.claim_comment(&wiki.id, comment),
        };
        read.map_err(unavailable)
    }

    /// What to record for a hide: **the content of a change is recorded against each page the change wrote** (the reference is
    /// the page's repository path, which the export names the page by), and what is of the change as a whole (its summary
    /// and its author) once, with no reference. Everything else is one record.
    fn records_of(
        &self,
        wiki: &WikiRecord,
        canonical: &HiddenItem,
        parts: HiddenParts,
    ) -> Result<Vec<(HiddenItem, HiddenParts)>, ModerationError> {
        if canonical.kind != HiddenItemKind::PageRevision || !parts.content {
            return Ok(vec![(canonical.clone(), parts)]);
        }
        let paths = self
            .read
            .written_paths_of(wiki, &canonical.id)
            .map_err(read_failure)?
            .unwrap_or_default();
        let content = HiddenParts {
            content: true,
            summary: false,
            author: false,
        };
        let mut out: Vec<(HiddenItem, HiddenParts)> = if paths.is_empty() {
            vec![(canonical.clone(), content)]
        } else {
            paths
                .into_iter()
                .map(|path| {
                    (
                        HiddenItem {
                            reference: Some(path),
                            ..canonical.clone()
                        },
                        content,
                    )
                })
                .collect()
        };
        let rest = HiddenParts {
            content: false,
            ..parts
        };
        if !rest.is_empty() {
            out.push((canonical.clone(), rest));
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_hide(
        &self,
        wiki: &WikiRecord,
        acting: &Acting,
        rung: Role,
        id: &TombstoneEventId,
        act: &TombstoneActId,
        found: &Found,
        parts: HiddenParts,
        category: ReasonCategory,
        note: Option<String>,
        declines: Option<&ProposalId>,
    ) -> Result<(), ModerationError> {
        // One act, one time: every record of this hide is written at the same second, under the one act id.
        let at = self.authorizer.now();
        let mut declined_transition = None;
        if let Some(proposal) = declines {
            let current = self.proposal_of(wiki, proposal)?;
            let to = crate::proposal::machine::apply(Some(current.state), ProposalOp::Decline)
                .map_err(|_| ModerationError::Changed)?;
            let change = ProposalChange {
                transition: ProposalTransition {
                    proposal: proposal.clone(),
                    op: ProposalOp::Decline,
                    from: Some(current.state),
                    to,
                    actor: acting.account.clone(),
                    reason: Some(format!(
                        "hidden by moderation: {}",
                        category_words(category)
                    )),
                    at: self.authorizer.now(),
                },
                text: None,
                revision: None,
                superseded_by: None,
            };
            let after = self
                .proposals
                .change_proposal(&change)
                .map_err(store_failure)?;
            declined_transition = self
                .proposals
                .last_transition_position(proposal)
                .map_err(unavailable)?;
            if let Some(notifier) = &self.notifier {
                // The proposer is told that moderation declined it, and not by whom (D-8).
                notifier.declined_by_moderation(wiki, &after);
            }
        }
        let records = self.records_of(wiki, &found.canonical, parts)?;
        for (n, (item, these)) in records.into_iter().enumerate() {
            self.governance
                .record_tombstone_event(&TombstoneEvent {
                    id: if n == 0 {
                        id.clone()
                    } else {
                        TombstoneEventId::new()
                    },
                    act_id: act.clone(),
                    wiki: wiki.id.clone(),
                    at,
                    act: TombstoneAct::Hide,
                    item,
                    parts: these,
                    category: Some(category),
                    note: note.clone(),
                    affected: found.affected.clone(),
                    by: Actor {
                        account: acting.account.clone(),
                        rung,
                    },
                    policy_block: acting.rules.block().map(str::to_owned),
                    of_event: None,
                    appeal: None,
                    declined_transition: if n == 0 { declined_transition } else { None },
                })
                .map_err(store_failure)?;
        }
        Ok(())
    }

    // ----------------------------------------------------------------------------------------------- unhide

    /// The hide `hide` that is in force, with the item's other events: or why there is none.
    fn in_force(
        &self,
        wiki: &WikiRecord,
        hide: &TombstoneEventId,
    ) -> Result<TombstoneEvent, ModerationError> {
        let event = self
            .governance
            .tombstone_event(hide)
            .map_err(unavailable)?
            .filter(|e| e.wiki == wiki.id && e.act == TombstoneAct::Hide)
            .ok_or(ModerationError::NoSuchHiding)?;
        let events = self
            .governance
            .tombstone_events_of(&wiki.id, &event.item)
            .map_err(unavailable)?;
        if Hidden::of_events(&events)
            .marks()
            .iter()
            .any(|m| m.event == event.id)
        {
            Ok(event)
        } else {
            Err(ModerationError::NoSuchHiding)
        }
    }

    /// **Unhide**: restore exactly what the hide `hide` hid. The same item, the same parts, once.
    pub fn unhide(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        hide: &TombstoneEventId,
    ) -> Result<UnhideMade, ModerationError> {
        let acting = self.begin(principal, wiki)?;
        self.authorize(principal, wiki)?;
        let Some(rung) = acting.rung() else {
            return Err(ModerationError::NotAllowed);
        };
        let hidden = self.in_force(wiki, hide)?;
        let id = TombstoneEventId::new();
        let act = TombstoneActId::new();
        let detail = format!(
            "unhide; item={}:{}; parts={}; of={hide}",
            hidden.item.kind.as_str(),
            hidden.item.id,
            parts_words(hidden.parts)
        );
        self.audit_act(
            principal,
            wiki,
            &acting,
            "tombstone-unhide",
            &act,
            AuditOutcome::Allowed,
            &detail,
        )?;
        let done = self.governance.record_tombstone_event(&TombstoneEvent {
            id: id.clone(),
            act_id: act.clone(),
            wiki: wiki.id.clone(),
            at: self.authorizer.now(),
            act: TombstoneAct::Unhide,
            item: hidden.item,
            parts: hidden.parts,
            category: None,
            note: None,
            affected: hidden.affected,
            by: Actor {
                account: acting.account.clone(),
                rung,
            },
            policy_block: acting.rules.block().map(str::to_owned),
            of_event: Some(hidden.id.clone()),
            appeal: None,
            declined_transition: None,
        });
        match done {
            Ok(()) => Ok(UnhideMade {
                id,
                act,
                of: hide.clone(),
            }),
            Err(e) => {
                let _ = self.audit_act(
                    principal,
                    wiki,
                    &acting,
                    "tombstone-unhide",
                    &act,
                    AuditOutcome::Failed,
                    &format!("{detail}; the record failed"),
                );
                Err(store_failure(e))
            }
        }
    }

    // ------------------------------------------------------------------------------------- show hidden content

    /// **Show hidden content** of the item named by `item`, for the hide `hide`: the `moderate` rung's explicit act. The
    /// audit event (one line, never content) is written first; a trail that cannot be written refuses the act. Gives back
    /// the parts that hide hid, and the note.
    pub fn show_hidden(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        item: &Item,
        hide: &TombstoneEventId,
    ) -> Result<Revealed, ModerationError> {
        let acting = self.begin(principal, wiki)?;
        self.authorize(principal, wiki)?;
        let event = self.in_force(wiki, hide)?;
        let found = self.find(wiki, item)?;
        if found.canonical.kind != event.item.kind || found.canonical.id != event.item.id {
            return Err(ModerationError::NoSuchHiding);
        }
        self.audit_act(
            principal,
            wiki,
            &acting,
            "tombstone-show",
            &event.act_id,
            AuditOutcome::Allowed,
            &format!(
                "show; item={}:{}; parts={}",
                event.item.kind.as_str(),
                event.item.id,
                parts_words(event.parts)
            ),
        )?;
        let mut revealed = Revealed {
            note: event.note.clone(),
            ..Revealed::default()
        };
        match item {
            Item::ProposalComment { proposal, comment } => {
                let found = self
                    .proposals
                    .comments(proposal, None)
                    .map_err(unavailable)?
                    .into_iter()
                    .find(|c| c.id == *comment)
                    .ok_or(ModerationError::NoSuchItem)?;
                if event.parts.content {
                    revealed.text = Some(found.body);
                }
                if event.parts.author {
                    revealed.author = Some(self.name_of(&found.author));
                }
            }
            Item::ClaimComment { claim, comment } => {
                let mut after: Option<CommentId> = None;
                'pages: loop {
                    let page = self
                        .claims
                        .claim_comments(
                            &wiki.id,
                            claim,
                            after.as_ref(),
                            inklkunn_store::claim_thread::MAX_PAGE,
                        )
                        .map_err(unavailable)?;
                    if let Some(row) = page.rows.iter().find(|r| r.comment.id == *comment) {
                        if event.parts.content {
                            revealed.text = Some(row.comment.body.clone());
                        }
                        if event.parts.author {
                            revealed.author = Some(self.name_of(&row.comment.author));
                        }
                        break 'pages;
                    }
                    match page.next {
                        Some(next) => after = Some(next),
                        None => return Err(ModerationError::NoSuchItem),
                    }
                }
            }
            Item::ProposalRevision { proposal, seq } => {
                let record = self.proposal_of(wiki, proposal)?;
                if event.parts.summary {
                    revealed.text = Some(record.summary.clone());
                }
                if event.parts.author {
                    revealed.author = Some(self.name_of(&record.author));
                }
                if event.parts.content {
                    revealed.files = self
                        .proposals
                        .revisions(proposal, None)
                        .map_err(unavailable)?
                        .into_iter()
                        .find(|r| r.seq == *seq)
                        .map(|r| {
                            r.content
                                .files
                                .into_iter()
                                .take(MAX_REVEALED_FILES)
                                .map(|f| RevealedFile {
                                    path: f.path,
                                    text: f.text.unwrap_or_default(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
            Item::PageRevision { block, view } => {
                let shown = self
                    .read
                    .reveal_change(wiki, view.as_deref(), block)
                    .map_err(read_failure)?
                    .ok_or(ModerationError::NoSuchItem)?;
                if event.parts.summary {
                    revealed.text = Some(shown.info.summary.clone());
                }
                if event.parts.author {
                    revealed.author = Some(match &shown.info.provenance {
                        crate::read::Provenance::Recorded {
                            proposed_by: crate::read::Who::Person { name },
                            ..
                        } => name.clone(),
                        crate::read::Provenance::Recorded {
                            proposed_by: crate::read::Who::Anonymous { token },
                            ..
                        } => token.clone(),
                        _ => String::new(),
                    });
                }
                if event.parts.content {
                    revealed.files = shown
                        .files
                        .into_iter()
                        .map(|(path, text)| RevealedFile { path, text })
                        .collect();
                }
            }
        }
        Ok(revealed)
    }

    fn name_of(&self, account: &AccountId) -> String {
        match self.accounts.account(account) {
            Ok(Some(found)) => found.display_name.as_str().to_owned(),
            _ => account.to_string(),
        }
    }

    // -------------------------------------------------------------------------------------------------- the log

    /// The wiki's moderation log, newest first: each hide and unhide, **with the item, the category, the rung and the date,
    /// never the hidden content and never a name**, to whoever may read the wiki. The note is carried for the `moderate`
    /// rung only.
    pub fn log(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        before: Option<&TombstoneEventId>,
        limit: usize,
    ) -> Result<Vec<LogRow>, ModerationError> {
        self.readable(principal, wiki)?;
        let events = self
            .governance
            .tombstone_log(&wiki.id, before, limit)
            .map_err(|e| match e {
                StoreError::NotFound => ModerationError::NoSuchHiding,
                other => unavailable(other),
            })?;
        let may_note = self.may_moderate(principal, wiki);
        let mut rows = Vec::with_capacity(events.len());
        for e in &events {
            let hide = if e.act == TombstoneAct::Hide {
                Some(e.clone())
            } else {
                match &e.of_event {
                    Some(of) => self.governance.tombstone_event(of).map_err(unavailable)?,
                    None => None,
                }
            };
            let in_force = if e.act == TombstoneAct::Hide {
                let chain = self
                    .governance
                    .tombstone_events_of(&wiki.id, &e.item)
                    .map_err(unavailable)?;
                Hidden::of_events(&chain)
                    .marks()
                    .iter()
                    .any(|m| m.event == e.id)
            } else {
                false
            };
            rows.push(LogRow {
                id: e.id.clone(),
                at: e.at,
                act: e.act,
                item: e.item.clone(),
                parts: e.parts,
                category: hide.as_ref().and_then(|h| h.category),
                rung: e.by.rung,
                policy_block: e.policy_block.clone(),
                note: if may_note {
                    hide.as_ref().and_then(|h| h.note.clone())
                } else {
                    None
                },
                in_force,
                declined: e.declined_transition.is_some(),
                appeal: if e.act == TombstoneAct::Hide {
                    let appeals = self
                        .governance
                        .appeals_about(
                            &wiki.id,
                            &AppealedAction {
                                kind: AppealedKind::Tombstone,
                                id: e.id.to_string(),
                            },
                        )
                        .map_err(unavailable)?;
                    AppealMark::of(&appeals)
                } else {
                    None
                },
                by_appeal: e.appeal.is_some(),
            });
        }
        Ok(rows)
    }

    /// What the moderation form needs to know of an item before it is shown: what is hidden of it now (the marks, never
    /// content), whether its **content** could be hidden or is a view's tip, whether hiding its content would also decline an
    /// open proposal, and which parts it has. **Records nothing and shows no content.** Only the rung that may moderate is
    /// answered ([`ModerationError::NotAllowed`] for another reader), because the answer is a read of the repository.
    pub fn preview(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        item: &Item,
    ) -> Result<Preview, ModerationError> {
        self.readable(principal, wiki)?;
        // The preview runs the tip check and the list of later holders, which read the repository (up to a thousand reads): only
        // the rung that may hide asks it (L-7), whatever the page in front of it checked. Nothing is drawn or recorded.
        if !self.may_moderate(principal, wiki) {
            return Err(ModerationError::NotAllowed);
        }
        let found = self.find(wiki, item)?;
        let marks = self
            .hidden_of(wiki, &found.canonical, item)?
            .marks()
            .to_vec();
        let (tip, survives) = if found.canonical.kind == HiddenItemKind::PageRevision {
            match self
                .read
                .tip_check(wiki, &found.canonical.id)
                .map_err(read_failure)?
            {
                TipCheck::Clear => (None, None),
                TipCheck::Survives { lines, at_least } => {
                    (None, Some(Survives { lines, at_least }))
                }
                TipCheck::Holds { view } => {
                    (Some(ModerationError::TipContent { view: Some(view) }), None)
                }
                TipCheck::Unchecked => (Some(ModerationError::TipContent { view: None }), None),
                TipCheck::Unresolved => (Some(ModerationError::TipUnresolved), None),
            }
        } else {
            (None, None)
        };
        // The later revisions that hold the text of a change whose content may be hidden (J-4): a hide does not cover them.
        let later = if found.canonical.kind == HiddenItemKind::PageRevision && tip.is_none() {
            let view = match item {
                Item::PageRevision { view, .. } => view.as_deref(),
                _ => None,
            };
            // A list that could not be made is said so, and does not stop the form: nothing in it is needed to hide.
            Some(
                self.read
                    .later_holders(wiki, view, &found.canonical.id)
                    .unwrap_or_else(|_| LaterHolders::unsettled()),
            )
        } else {
            None
        };
        Ok(Preview {
            marks,
            tip,
            survives,
            later,
            declines: found.open_latest.is_some(),
            summary: !found.comment
                && !(matches!(item, Item::ProposalRevision { .. }) && !found.is_latest),
        })
    }
}

/// The lines of a change that a view's tip still holds (see [`TipCheck::Survives`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Survives {
    /// How many.
    pub lines: usize,
    /// There are at least this many: the count stopped at its bound.
    pub at_least: bool,
}

/// What the moderation form is told of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The hidings in force on it, oldest first.
    pub marks: Vec<Mark>,
    /// Why its content cannot be hidden, when it cannot ([`ModerationError::TipContent`], [`ModerationError::TipUnresolved`]).
    pub tip: Option<ModerationError>,
    /// Lines the change added that a tip's page still has, when its content may be hidden all the same: the hide does not hide them.
    pub survives: Option<Survives>,
    /// The later revisions of its pages that hold most of the lines the change added, when its content may be hidden: **a hide does
    /// not cover them**, and the moderator decides whether to hide them too (J-4; [`LaterHolders`] for what *holds* means and for
    /// every bound). `None` for an item that is not a change, and for one whose content may not be hidden.
    pub later: Option<LaterHolders>,
    /// Hiding its content also declines the proposal it is the latest revision of.
    pub declines: bool,
    /// It has a summary to hide.
    pub summary: bool,
}

/// The parts, as the trail and the log say them.
fn parts_words(parts: HiddenParts) -> String {
    let mut words = Vec::new();
    if parts.content {
        words.push("content");
    }
    if parts.summary {
        words.push("summary");
    }
    if parts.author {
        words.push("author");
    }
    words.join(",")
}
