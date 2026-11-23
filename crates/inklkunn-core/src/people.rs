//! Who holds which rung, and the restrictions that hold a person down (RFC 006 D-6 and D-7's restriction half; handoff G-T2;
//! threat model C-3g, C-14f, C-14h).
//!
//! **The act is a recorded decision.** A grant, a revocation, a restriction, a renewal and a lifting are each written
//! through the store's own record ([`GovernanceStore`](inklkunn_store::GovernanceStore)): who acted, the rung they held, the
//! block of the rules they held it under, and the reason. A grant is applied to the wiki's role grants in the same step as
//! it is recorded, so the two cannot disagree. Each is also an **audited consequential act**: the authorizer's own
//! decision record (`admin` for an admin's grant, `moderate` for a maintainer's grant and for every restriction), which
//! names the rung and the block, and then one event of its own that names the act (`grant`, `revoke`, `restrict`,
//! `renew-restriction`, `lift-restriction`) and the account it is about, written **before** the store is changed: nothing
//! consequential happens unrecorded.
//!
//! **Who may do what** is the rules' own ([`check_grant`] and [`check_restrict`]), pure functions: a maintainer changes a rung only within the wiki's `grant_up_to` on
//! both ends of the change; nobody changes their own; the last admin who can sign in stays; a restriction is set by the
//! `moderate` rung on an account strictly below them, never on an admin, never on oneself.
//!
//! **The authorization of the act.** No new [`Action`](crate::Action): RFC 002 D-2's set carries it. An admin's grant is
//! `admin` (RFC 006 D-2: roles are an admin action), a maintainer's grant within reach and every restriction are
//! `moderate` (a person who may moderate may be asked to act on people), and the rule decides the rest. **These acts are
//! web-only** (RFC 006 D-12): the service is built for a person's session, and nothing in the read API reaches it.
//!
//! **Grants stay out of the repository and out of the export** (RFC 006 D-6): this module reads and writes the service's
//! store and never the repository. `inklkunn account grant` stays the operator's way in, and records a decision with no one
//! acting.

mod rule;
mod service;
mod text;
mod view;

pub use rule::{
    GrantCheck, GrantRefusal, RestrictRefusal, check_grant, check_restrict, grants_at_all,
    highest_grantable,
};
pub use service::{GrantMade, PeopleError, PeopleService, RestrictionMade, RestrictionOutcome};
pub use text::{MAX_REASON_CHARS, ReasonFault, clean_reason};
pub use view::{Holders, PeopleView, Person, RestrictedRow, RungRow};

/// Seconds in a day: a restriction's length is counted in whole days.
pub const DAY_SECS: i64 = 86_400;

#[cfg(test)]
mod tests;
