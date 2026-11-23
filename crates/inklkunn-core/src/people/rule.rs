//! The rules of who may change whose rung, and who may restrict whom (RFC 006 D-6, D-7; threat model C-14h), as **pure
//! functions** of the facts the service has read: no store, no clock, nothing to mock. The service asks them after it has
//! authorized the act, and every refusal is a value that names its rule.
//!
//! # A grant, a revocation, a demotion
//!
//! One rule for all of them, because they are one thing: **a change of the rung an account holds through a grant**. A
//! revocation is a change to none.
//!
//! - **An admin** may change any account's rung to any rung, **except** that nobody changes their own, and that **the last
//!   admin who can still sign in is not demoted or revoked** (the product must not strand a wiki: the operator's
//!   `inklkunn account grant` is still there, but a wiki should not need it). The second half is a guard that **cannot be
//!   reached through the service on its own**: an admin who acts is an admin who can sign in and is not the account they
//!   act on, so another admin who can sign in is always there. It stays as the rule's own statement of INV-14's spirit (and
//!   holds if a later way of acting is not an admin who can sign in); the stranding that *can* happen, a sole admin changing
//!   their own rung, is the first half's.
//! - **A maintainer** may change an account's rung **only when both the rung it holds through a grant now and the rung it
//!   would hold after are at or below `rungs.grant_up_to`** (none: a maintainer changes no one's). So a maintainer
//!   cannot promote anyone above their reach, and cannot demote or revoke someone who already stands above it: reach is
//!   a ceiling on both ends of the change.
//! - **No one else** changes a rung.
//!
//! # A restriction
//!
//! One restricts an account only when **the account holds a rung strictly below one's own** (the higher of its grant and
//! the wiki's `entry`, before any cap), and **never an admin**. So a maintainer cannot restrict a maintainer or an admin; an
//! admin can restrict a maintainer; nobody restricts an admin (the operator's authority is the command line's, not a
//! rung's); and nobody restricts themselves.

use inklkunn_store::Role;

use crate::governance::GrantUpTo;

/// Why a change of a rung is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrantRefusal {
    /// The person does not grant: their rung is below maintainer, or they are a maintainer in a wiki whose rules let a
    /// maintainer grant none.
    NotAllowed,
    /// Nobody changes their own rung.
    OwnRung,
    /// The account already holds exactly that rung through a grant (or holds none and is asked to hold none).
    NothingChanges,
    /// A maintainer's reach: the rung the account holds now, or the rung it would hold, is above `rungs.grant_up_to`.
    AboveReach {
        /// What the wiki's rules let a maintainer grant.
        up_to: GrantUpTo,
    },
    /// The change would leave the wiki without an admin who can sign in.
    LastAdmin,
}

/// The facts of a change of a rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantCheck {
    /// The rung the person who acts holds now (after a restriction).
    pub actor: Role,
    /// Is the person who acts the account whose rung changes?
    pub own: bool,
    /// The rung the account holds through a grant now.
    pub current: Option<Role>,
    /// The rung it would hold: `None` is a revocation.
    pub new: Option<Role>,
    /// `rungs.grant_up_to` of the rules in force.
    pub grant_up_to: GrantUpTo,
    /// How many accounts **other than the one whose rung changes** hold an admin grant in the wiki and are not disabled.
    pub other_admins_who_can_sign_in: usize,
}

/// Does a person who holds `actor` change anyone's rung at all, under the rules' `grant_up_to`? The gate that comes before
/// the act is authorized.
pub fn grants_at_all(actor: Role, grant_up_to: GrantUpTo) -> bool {
    match actor {
        Role::Admin => true,
        Role::Maintainer => grant_up_to.highest().is_some(),
        Role::Observer | Role::Questioner | Role::Contributor | Role::Reviewer => false,
    }
}

/// The highest rung a person who holds `actor` may give: `Some(Admin)` for an admin, the rules' `grant_up_to` for a
/// maintainer, `None` for no one else (and for a maintainer who may grant none).
pub fn highest_grantable(actor: Role, grant_up_to: GrantUpTo) -> Option<Role> {
    match actor {
        Role::Admin => Some(Role::Admin),
        Role::Maintainer => grant_up_to.highest(),
        Role::Observer | Role::Questioner | Role::Contributor | Role::Reviewer => None,
    }
}

/// May this change be made? The one rule of the module's text.
pub fn check_grant(c: &GrantCheck) -> Result<(), GrantRefusal> {
    if c.own {
        return Err(GrantRefusal::OwnRung);
    }
    if !grants_at_all(c.actor, c.grant_up_to) {
        return Err(GrantRefusal::NotAllowed);
    }
    if c.current == c.new {
        return Err(GrantRefusal::NothingChanges);
    }
    if c.actor != Role::Admin {
        // A maintainer's reach is a ceiling on both ends of the change.
        let within =
            |rung: Option<Role>| rung.is_none_or(|r| c.grant_up_to.maintainer_may_grant(r));
        if !(within(c.current) && within(c.new)) {
            return Err(GrantRefusal::AboveReach {
                up_to: c.grant_up_to,
            });
        }
    }
    let leaves_admin = c.current == Some(Role::Admin) && c.new != Some(Role::Admin);
    if leaves_admin && c.other_admins_who_can_sign_in == 0 {
        return Err(GrantRefusal::LastAdmin);
    }
    Ok(())
}

/// Why a restriction is refused (or a lifting, or a renewal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RestrictRefusal {
    /// The person's rung is below the rung the rules ask for moderating.
    NotAllowed,
    /// Nobody acts on their own restriction.
    OwnAccount,
    /// **Nobody restricts an admin.**
    AdminIsNotRestricted,
    /// The account holds a rung that is not below the person's own: a maintainer cannot restrict a maintainer.
    NotBelowYou,
    /// The account is restricted already, until this second: the restriction is renewed, not set again.
    AlreadyRestricted {
        /// When the restriction in force ends.
        until: i64,
    },
    /// The account is not restricted: there is nothing to renew or lift.
    NotRestricted,
    /// The restriction would last longer than the operator allows (`INKLKUNN_GOVERNANCE_LIMIT_DAYS`), or no day at all.
    DaysOutOfRange {
        /// The longest, in days.
        limit_days: u32,
    },
}

/// May the person who holds `actor` (when the rules ask `moderate` for moderating) restrict, renew or lift the restriction
/// of an account that holds `target` before any cap?
pub fn check_restrict(
    actor: Role,
    moderate: Role,
    target: Option<Role>,
    own: bool,
) -> Result<(), RestrictRefusal> {
    if own {
        return Err(RestrictRefusal::OwnAccount);
    }
    if actor < moderate {
        return Err(RestrictRefusal::NotAllowed);
    }
    if target == Some(Role::Admin) {
        return Err(RestrictRefusal::AdminIsNotRestricted);
    }
    if target.is_some_and(|t| t >= actor) {
        return Err(RestrictRefusal::NotBelowYou);
    }
    Ok(())
}
