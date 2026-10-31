//! Invitations (RFC 002 D-1; handoff "serving sign-in", U2; the owner's ruling DR-11): **how an account comes
//! into being.** An invitation is a one-time, expiring link the operator makes on the command line and shows
//! once. The service keeps the SHA-256 of the link's secret (a copy of the database opens no invitation), the
//! time it was made and the time it ends, and, when the operator named one, the role in a wiki the account
//! starts with. The person who opens it makes the account in the same step that uses the invitation up.
//!
//! **Written once.** An invitation is never changed and never removed, and its use is a second record that
//! can exist once: in SQLite the database itself aborts an `UPDATE`, a `DELETE` and an `INSERT` over a key
//! already taken (the acceptance records' device). Redeeming is one store step, [`InvitationStore::redeem_invitation`]:
//! the invitation is checked, the account is added, the role is granted and the use is recorded, all or none,
//! so a name that is taken or a password that was refused does not burn the link, and two people opening one
//! link cannot both get an account.

use std::fmt;

use crate::Result;
use crate::access::{Role, RoleGrant};
use crate::identity::{Account, ExternalIdentity};
use crate::record::{AccountId, InvitationId, WikiId};

/// When and by whom an invitation was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitationUse {
    /// The account the person made with it.
    pub account: AccountId,
    /// Unix seconds.
    pub at: i64,
}

/// An invitation as stored.
#[derive(Clone, PartialEq, Eq)]
pub struct InvitationRecord {
    /// For the audit trail and the operator's records; not the link.
    pub id: InvitationId,
    /// `sha256$<hex>` of the link's secret. The secret itself is never stored.
    pub hash: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds; the invitation is over at this instant.
    pub expires_at: i64,
    /// The role in a wiki the new account starts with, if the operator named one. Both or neither.
    pub grant: Option<(WikiId, Role)>,
    /// Set once, when the invitation was used.
    pub used: Option<InvitationUse>,
}

impl fmt::Debug for InvitationRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvitationRecord")
            .field("id", &self.id)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("grant", &self.grant)
            .field("used", &self.used)
            .finish_non_exhaustive()
    }
}

impl InvitationRecord {
    /// Can this invitation still be used at `now`: not used, not over?
    pub fn usable_at(&self, now: i64) -> bool {
        self.used.is_none() && self.expires_at > now
    }
}

/// The invitation records. Every implementation passes the same invariant tests (`tests/invitations.rs`).
pub trait InvitationStore: Send + Sync {
    /// Store a new invitation (one that has not been used). [`crate::StoreError::IdTaken`] when the id or the
    /// hash exists, [`crate::StoreError::NotFound`] when it names a wiki that is not there; nothing is stored
    /// then.
    fn add_invitation(&self, invitation: &InvitationRecord) -> Result<()>;
    /// The invitation with this hash, used or not, over or not.
    fn invitation_by_hash(&self, hash: &str) -> Result<Option<InvitationRecord>>;
    /// Use the invitation with this hash to make `account`, in one step: the invitation exists, has not been
    /// used and is not over at `now`; the account is added (as [`crate::AccountStore::add_account`] does, with
    /// its refusals); the role the invitation names, if any, is granted to it (granted by no account: the
    /// operator did); the use is recorded. Returns the grant made. **All or none**: any refusal leaves the
    /// invitation as it was. [`crate::StoreError::InvitationUnusable`] is the one answer for an invitation
    /// that is unknown, used or over.
    fn redeem_invitation(
        &self,
        hash: &str,
        account: &Account,
        now: i64,
    ) -> Result<Option<RoleGrant>>;
    /// As [`InvitationStore::redeem_invitation`], and the account is linked to `identity` in the same step: a
    /// person who opened the link and signed in with a provider has an account that provider identity reaches,
    /// and the link is spent exactly then. [`crate::StoreError::IdentityTaken`] when another account holds the
    /// identity (the invitation is then not spent: the person should sign in with it instead). An invitation
    /// that is unusable is [`crate::StoreError::InvitationUnusable`] before the identity is looked at.
    fn redeem_invitation_with_identity(
        &self,
        hash: &str,
        account: &Account,
        identity: &ExternalIdentity,
        now: i64,
    ) -> Result<Option<RoleGrant>>;
}
