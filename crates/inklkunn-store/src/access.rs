//! The authorization vocabulary the store persists (RFC 002 D-1, D-2; requirements GV-2, ID-7): the **role**
//! an account holds in a wiki, the **scope** a token is limited to, and the grants that record who was given
//! what. The *meaning* of a role (which actions it allows) is `inklkunn-core`'s one function; this module
//! only names the rungs and keeps the grants.
//!
//! M2's role set is fixed (RFC 002 D-2): the participation ladder of GV-2, granted directly. RFC 006 turns it
//! into policy as data without changing these types' contract.

use serde::{Deserialize, Serialize};

use crate::record::{AccountId, WikiId};

/// What an account may do in one wiki: the rungs of the participation ladder (GV-2), lowest first. The
/// derived order *is* the ladder, so "at least a reviewer" is `role >= Role::Reviewer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Reads, and follows what happens. Observation is a legitimate first step.
    Observer,
    /// Asks and comments.
    Questioner,
    /// Proposes changes.
    Contributor,
    /// Reviews proposals.
    Reviewer,
    /// Accepts proposals, creates views, moderates.
    Maintainer,
    /// Administers the wiki: roles, keys, exports and imports.
    Admin,
}

impl Role {
    /// Every role, lowest first.
    pub const ALL: [Role; 6] = [
        Role::Observer,
        Role::Questioner,
        Role::Contributor,
        Role::Reviewer,
        Role::Maintainer,
        Role::Admin,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Observer => "observer",
            Role::Questioner => "questioner",
            Role::Contributor => "contributor",
            Role::Reviewer => "reviewer",
            Role::Maintainer => "maintainer",
            Role::Admin => "admin",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// How much of an account's authority a credential carries (RFC 002 D-1): `read ⊂ propose ⊂ review ⊂
/// accept ⊂ admin`. A browser session carries all of it ([`Scope::Admin`], the top of the ladder); an API
/// token carries what it was issued with, and is **intersected** with the account's roles: a token never
/// does more than its scope or its account allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Read pages, history, proposals.
    Read,
    /// Also comment and propose.
    Propose,
    /// Also review.
    Review,
    /// Also accept, create views and moderate.
    Accept,
    /// Everything the account may do.
    Admin,
}

impl Scope {
    /// Every scope, narrowest first.
    pub const ALL: [Scope; 5] = [
        Scope::Read,
        Scope::Propose,
        Scope::Review,
        Scope::Accept,
        Scope::Admin,
    ];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Propose => "propose",
            Scope::Review => "review",
            Scope::Accept => "accept",
            Scope::Admin => "admin",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        Scope::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// One account's role in one wiki, with who gave it and when. An account holds at most one role per wiki:
/// a promotion or a demotion replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleGrant {
    /// The account.
    pub account: AccountId,
    /// The wiki.
    pub wiki: WikiId,
    /// The role.
    pub role: Role,
    /// The account that granted it; `None` when the operator did, from the command line (the first
    /// administrator of a wiki can only be named that way).
    pub granted_by: Option<AccountId>,
    /// Unix seconds.
    pub granted_at: i64,
}

/// The role grants. Every implementation passes the same invariant tests (`tests/invariants.rs`).
pub trait RoleStore: Send + Sync {
    /// Give `grant.account` the role in the wiki, replacing any it held there. The wiki must exist
    /// ([`crate::StoreError::NotFound`] otherwise): a grant for a wiki that is not there is a mistake or an
    /// attack, never data to keep.
    fn grant(&self, grant: RoleGrant) -> crate::Result<()>;
    /// Take the account's role in the wiki away; `true` when it held one.
    fn revoke(&self, account: &AccountId, wiki: &WikiId) -> crate::Result<bool>;
    /// The role the account holds in the wiki, if any.
    fn role(&self, account: &AccountId, wiki: &WikiId) -> crate::Result<Option<Role>>;
    /// Every grant in the wiki, oldest first.
    fn grants_in(&self, wiki: &WikiId) -> crate::Result<Vec<RoleGrant>>;
}

#[cfg(test)]
mod tests;
