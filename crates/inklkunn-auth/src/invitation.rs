//! Invitations and joining (RFC 002 D-1; the sign-in handoff's U2; the owner's ruling DR-11): **how an account
//! comes into being.** You are invited, or, where the server is open, you join; then you choose how you sign in.
//!
//! An invitation is a one-time, expiring link the operator makes on the command line. Its secret is 256 random
//! bits behind the prefix `inv_`; it is shown once and kept only as its SHA-256 (a copy of the database opens no
//! invitation). The person who opens it makes the account and the invitation is used up in **one store step**
//! (`InvitationStore::redeem_invitation`), so a link spends exactly when an account exists and two people
//! opening one link cannot both get one. No password is ever sent or printed: the person types theirs into the
//! page, and the operator passes the link on as they like.
//!
//! The credential the account is bound to is the person's choice. A password is built here; a provider's
//! identity is the next task's, and the invitation record already leaves room for it: it names no credential,
//! so redeeming with a verified provider identity is the same store step with an account that has no password
//! and a link made afterwards.
//!
//! **Joining** is the server's setting, [`JoinPolicy`], **off by default**. It never matches or links an
//! account by e-mail address: one person, one account, and a second credential is the person's own act while
//! signed in or the operator's.

use std::fmt;
use std::sync::Arc;

use inklkunn_store::{InvitationId, InvitationRecord, InvitationStore, Role, StoreError, WikiId};

use crate::error::AuthError;
use crate::random;
use crate::stored::stored_form;

/// What every invitation's secret starts with (a leaked one is recognisable to a secret scanner).
pub const INVITATION_PREFIX: &str = "inv_";
/// How long an invitation lasts unless the operator says otherwise: seven days.
pub const DEFAULT_LIFETIME_SECS: i64 = 7 * 24 * 3600;
/// The longest an invitation may last. A link is a bearer secret that someone is going to paste somewhere.
pub const MAX_LIFETIME_SECS: i64 = 30 * 24 * 3600;
/// `inv_` and 43 base64url characters.
const SECRET_LEN: usize = INVITATION_PREFIX.len() + 43;

/// Who may make an account for themselves (`INKLKUNN_JOIN`). **The default is `Off`.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JoinPolicy {
    /// Nobody joins: accounts come from invitations (and the operator).
    #[default]
    Off,
    /// A first sign-in through the configured provider makes an account. For an organization's own provider; a
    /// public provider makes this open registration.
    Provider,
    /// Anyone may make an account: through the provider if there is one, and with a password.
    Anyone,
}

impl JoinPolicy {
    /// The setting's value.
    pub const fn as_str(self) -> &'static str {
        match self {
            JoinPolicy::Off => "off",
            JoinPolicy::Provider => "provider",
            JoinPolicy::Anyone => "anyone",
        }
    }

    /// Parse the setting's value.
    pub fn parse(s: &str) -> Option<Self> {
        [JoinPolicy::Off, JoinPolicy::Provider, JoinPolicy::Anyone]
            .into_iter()
            .find(|p| p.as_str() == s)
    }

    /// May a first sign-in through the configured provider make an account, with no invitation?
    pub const fn lets_a_provider_sign_in_join(self) -> bool {
        matches!(self, JoinPolicy::Provider | JoinPolicy::Anyone)
    }

    /// May a person make an account with a password of their own choosing, with no invitation?
    pub const fn lets_anyone_join_with_a_password(self) -> bool {
        matches!(self, JoinPolicy::Anyone)
    }
}

/// An invitation's link secret. Shown to the operator once; its `Debug` shows nothing of it.
#[derive(Clone, PartialEq, Eq)]
pub struct InvitationSecret(String);

impl InvitationSecret {
    /// The text that goes in the link.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InvitationSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvitationSecret(…)")
    }
}

/// An invitation just made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedInvitation {
    /// For the audit trail.
    pub id: InvitationId,
    /// To show the operator now and never again.
    pub secret: InvitationSecret,
    /// When it ends, in Unix seconds.
    pub expires_at: i64,
}

/// Why an invitation was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteError {
    /// The lifetime is not between one second and [`MAX_LIFETIME_SECS`].
    Lifetime,
    /// The wiki the invitation names is not there.
    NoSuchWiki,
    /// The store or the random source failed.
    Auth(AuthError),
}

impl fmt::Display for InviteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InviteError::Lifetime => write!(
                f,
                "an invitation lives from one second to {} days",
                MAX_LIFETIME_SECS / 86_400
            ),
            InviteError::NoSuchWiki => f.write_str("the wiki is not there"),
            InviteError::Auth(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for InviteError {}

impl From<AuthError> for InviteError {
    fn from(e: AuthError) -> Self {
        InviteError::Auth(e)
    }
}

/// Is this text shaped like an invitation secret? Anything else is not worth a lookup.
pub(crate) fn looks_like_an_invitation(presented: &str) -> bool {
    presented.len() == SECRET_LEN
        && presented.starts_with(INVITATION_PREFIX)
        && presented
            .bytes()
            .skip(INVITATION_PREFIX.len())
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Making and checking invitations over a store.
pub(crate) struct Invitations {
    store: Arc<dyn InvitationStore>,
}

impl Invitations {
    pub(crate) fn new(store: Arc<dyn InvitationStore>) -> Self {
        Invitations { store }
    }

    /// Make an invitation that lasts `lifetime_secs`, naming `grant` if the operator did.
    pub(crate) fn issue(
        &self,
        grant: Option<(WikiId, Role)>,
        lifetime_secs: i64,
        now: i64,
    ) -> Result<IssuedInvitation, InviteError> {
        if !(1..=MAX_LIFETIME_SECS).contains(&lifetime_secs) {
            return Err(InviteError::Lifetime);
        }
        let secret = InvitationSecret(format!("{INVITATION_PREFIX}{}", random::token()?));
        let id = InvitationId::new();
        let expires_at = now.saturating_add(lifetime_secs);
        self.store
            .add_invitation(&InvitationRecord {
                id: id.clone(),
                hash: stored_form(&secret.0),
                created_at: now,
                expires_at,
                grant,
                used: None,
            })
            .map_err(|e| match e {
                StoreError::NotFound => InviteError::NoSuchWiki,
                other => InviteError::Auth(other.into()),
            })?;
        Ok(IssuedInvitation {
            id,
            secret,
            expires_at,
        })
    }

    /// Can this presented secret still be used at `now`? A secret that is malformed, unknown, used or over is
    /// one answer.
    pub(crate) fn usable(&self, presented: &str, now: i64) -> Result<bool, AuthError> {
        if !looks_like_an_invitation(presented) {
            return Ok(false);
        }
        Ok(self
            .store
            .invitation_by_hash(&stored_form(presented))?
            .is_some_and(|i| i.usable_at(now)))
    }

    /// The stored form of a presented secret, for the redeeming step.
    pub(crate) fn hash_of(presented: &str) -> Option<String> {
        looks_like_an_invitation(presented).then(|| stored_form(presented))
    }
}

#[cfg(test)]
mod tests;
