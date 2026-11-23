//! Appeals (RFC 006 D-9; handoff G-T5; requirement GV-8; threat model C-14f, C-14g): **a person contests what was done to them,
//! and someone who did not do it decides.**
//!
//! # What may be appealed, by whom, and what an overturn does
//!
//! Five kinds of action ([`AppealedKind`]), each appealable by the person it **affects** and by no one else, while it is **in
//! force**. A person is never told whether an action exists that is not about them: *no such action* and *not about you* are one
//! answer ([`AppealError::NotAffected`]).
//!
//! | Kind | The action's id | Affects (may appeal) | Who took it | An overturn |
//! |---|---|---|---|---|
//! | **decline** | the log position (`proposal_transitions.seq`, decimal) of the decline that is the proposal's last step | the proposal's author | the one who declined | the proposal is **reopened** ([`ProposalOp::Reopen`](inklkunn_store::ProposalOp)), held for review again |
//! | **tombstone** | a hide in force | the author of what was hidden | the one who hid it | an **unhide** of it, of every hide **of the same act** (one act id, A-2), and the reopening of the proposal that act declined (if it is still as that act left it) |
//! | **restriction** | the restriction in force | the restricted account | the one who set (or renewed) it | a **reversed** event of it |
//! | **protection** | a temporary protection in force | the author of an open proposal it holds back (a proposal that touches a page it covers) | the one who set (or renewed) it | a **reversed** event of it |
//! | **grant** | a decision that took a rung away or lowered it, **while it is the person's latest decision in the wiki** (A-3) | the account it was about | the one who decided (none: the operator's command line) | a decision **restoring the rung before**, if the decision is still the person's latest and they still hold the rung it gave them, and the decider may grant it |
//!
//! A protection is appealable by someone it **affected**, never a bystander: the *affected* are the people whose proposals it
//! holds, which is a fact of the records. An acceptance a protection refused leaves no record that names who tried, so the rule
//! does not pretend to know them. A grant decision that **raised** a rung is no one's grievance and is not appealable; nor is one that a later decision followed (D1 admin to maintainer, D2 to observer, D3 to maintainer: restoring D1's admin would undo D3).
//!
//! # What an appeal is
//!
//! One bounded text ([`MAX_TEXT_BYTES`], a comment's limit; line breaks allowed, control and bidirectional characters not),
//! recorded, **visible to the appellant and to the rung that decides appeals** and to no one else: the *appeals* page is that
//! rung's, the appellant has *my appeals*, and the moderation log says only that an action was appealed and how it ended, with no
//! text and no name ([`AppealMark`]). It is decided **once**, *upheld* or *overturned*, with a reason, by someone who did not take
//! the action and is not the appellant (the service says so by name, and the database refuses it too).
//!
//! # The order of every act
//!
//! The order of every write: the wiki must be one the person may read (an absent one and a forbidden one are one answer), they
//! must be an account, the rules must be readable (C-3d), the act is **authorized** (which records the decision with the rung
//! and the block), the pure rules say whether it may be done, **the audit event** is written, and only then the record; a
//! record that fails after its allowed event gets a `failed` follow-up (F-4), so the trail never names an act that did not happen.
//!
//! **Filing is authorized by reading and by being the person affected, never by `propose` or `comment`, and never capped by a
//! restriction** ([`Authorizer::authorize_appeal_filing`](crate::Authorizer)). The path cannot be used to comment: it records one
//! text, in the appeals' own table.
//!
//! **These acts are web-only** (RFC 006 D-12): the read API gets the appellant's own appeals (G-T7) and nothing writes.
//!
//! # How an overturn is made atomic
//!
//! **Every record of an overturn is written in the one transaction of the decision** (A-1): the record that reverses a grant, a
//! restriction, a protection or a tombstone, the unhides of the rest of a hiding's act, and the reopening of a proposal (the
//! proposal log is the same database). All or none: a moderator's unhide of one page while the decision is made makes the decision
//! fail whole, with nothing left written. Every check that can refuse is made before, and the two deciders that read more than
//! their own record (a decline's, a hiding's) go one at a time in this process, so that what is checked is what the next decision finds.

mod find;
mod limits;
mod model;
mod reverse;
mod service;
mod text;
mod views;

pub use limits::{
    AppealLimits, OPEN_PER_WIKI_DEFAULT, OPEN_PER_WIKI_MAX, PER_DAY_DEFAULT, PER_DAY_MAX,
};
pub use model::{AppealMark, AppealRow, ContestRef, DecisionRow, Offer, Outcome};
pub use service::{
    AppealError, AppealFiled, AppealService, DeciderRefusal, OverturnMade, OverturnRefusal,
};
pub use text::{MAX_TEXT_BYTES, TextFault, clean_text};
pub use views::{AppealsView, ContestWhat, Contestable, MineView, Queued};

pub use inklkunn_store::{AppealId, AppealedAction, AppealedKind};

#[cfg(test)]
mod tests;
