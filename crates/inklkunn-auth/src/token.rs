//! API tokens (RFC 002 D-1; AUTH-02): **hashed, scoped, expiring and revocable**.
//!
//! A token is 256 random bits behind the prefix `ink_` (so a leaked one is recognisable to a secret scanner).
//! It is shown once, when it is issued; the service keeps its SHA-256, the scope it was issued with, an
//! expiry and a revocation time. A token never does more than its scope: the principal made from it carries
//! that scope, which `authorize()` intersects with the account's role.

use std::fmt;
use std::sync::Arc;

use inklkunn_store::{AccountId, Scope, TokenId, TokenRecord, TokenStore};

use crate::error::AuthError;
use crate::random;
use crate::stored::stored_form;

/// What every token starts with.
pub const TOKEN_PREFIX: &str = "ink_";
/// The longest a token lives: a year and a day. A token is a bearer secret; the longer one lives the more
/// places it has been copied.
pub const MAX_LIFETIME_SECS: i64 = 366 * 24 * 3600;
/// `ink_` and 43 base64url characters.
const TOKEN_LEN: usize = TOKEN_PREFIX.len() + 43;

/// A token as shown to its owner, once. Its `Debug` shows nothing of it.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSecret(String);

impl TokenSecret {
    /// The text the owner copies.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenSecret(…)")
    }
}

/// A token just issued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedToken {
    /// For listing and revoking.
    pub id: TokenId,
    /// To show the owner now and never again.
    pub secret: TokenSecret,
    /// When it ends.
    pub expires_at: i64,
}

/// Why a token was not issued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueError {
    /// The lifetime is not between one second and [`MAX_LIFETIME_SECS`].
    Lifetime,
    /// The store or the random source failed.
    Auth(AuthError),
}

impl fmt::Display for IssueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueError::Lifetime => write!(
                f,
                "a token lives from one second to {} days",
                MAX_LIFETIME_SECS / 86_400
            ),
            IssueError::Auth(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for IssueError {}

impl From<AuthError> for IssueError {
    fn from(e: AuthError) -> Self {
        IssueError::Auth(e)
    }
}

/// A token that authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedToken {
    /// The account it acts for.
    pub account: AccountId,
    /// The most it may do.
    pub scope: Scope,
    /// Its id.
    pub id: TokenId,
}

/// What an owner sees of a token in a list: never the token, never its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSummary {
    /// For revoking.
    pub id: TokenId,
    /// What the owner called it.
    pub label: String,
    /// What it may do.
    pub scope: Scope,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub expires_at: Option<i64>,
    /// Unix seconds, when revoked.
    pub revoked_at: Option<i64>,
}

/// Issuing, checking and revoking tokens over a store.
pub struct Tokens {
    store: Arc<dyn TokenStore>,
}

impl Tokens {
    /// Tokens kept in `store`.
    pub fn new(store: Arc<dyn TokenStore>) -> Self {
        Tokens { store }
    }

    /// Issue a token for `account` that does at most `scope` and lasts `lifetime_secs`.
    pub fn issue(
        &self,
        account: &AccountId,
        scope: Scope,
        label: &str,
        lifetime_secs: i64,
        now: i64,
    ) -> Result<IssuedToken, IssueError> {
        if !(1..=MAX_LIFETIME_SECS).contains(&lifetime_secs) {
            return Err(IssueError::Lifetime);
        }
        let secret = TokenSecret(format!("{TOKEN_PREFIX}{}", random::token()?));
        let id = TokenId::new();
        let expires_at = now.saturating_add(lifetime_secs);
        self.store
            .add_token(&TokenRecord {
                id: id.clone(),
                hash: stored_form(&secret.0),
                account: account.clone(),
                scope,
                label: label.to_owned(),
                created_at: now,
                expires_at: Some(expires_at),
                revoked_at: None,
            })
            .map_err(AuthError::from)?;
        Ok(IssuedToken {
            id,
            secret,
            expires_at,
        })
    }

    /// Does this presented token authenticate at `now`? Unknown, malformed, expired and revoked tokens are
    /// one answer, [`AuthError::BadCredentials`].
    pub fn verify(&self, presented: &str, now: i64) -> Result<VerifiedToken, AuthError> {
        if presented.len() != TOKEN_LEN || !presented.starts_with(TOKEN_PREFIX) {
            return Err(AuthError::BadCredentials);
        }
        let record = self
            .store
            .token_by_hash(&stored_form(presented))?
            .ok_or(AuthError::BadCredentials)?;
        let expired = record.expires_at.is_some_and(|e| e <= now);
        if expired || record.revoked_at.is_some() {
            return Err(AuthError::BadCredentials);
        }
        Ok(VerifiedToken {
            account: record.account,
            scope: record.scope,
            id: record.id,
        })
    }

    /// Revoke the account's token `id`; `true` when it was theirs and live.
    pub fn revoke(&self, account: &AccountId, id: &TokenId, now: i64) -> Result<bool, AuthError> {
        Ok(self.store.revoke_token(account, id, now)?)
    }

    /// The account's tokens, oldest first.
    pub fn list(&self, account: &AccountId) -> Result<Vec<TokenSummary>, AuthError> {
        Ok(self
            .store
            .tokens_of(account)?
            .into_iter()
            .map(|t| TokenSummary {
                id: t.id,
                label: t.label,
                scope: t.scope,
                created_at: t.created_at,
                expires_at: t.expires_at,
                revoked_at: t.revoked_at,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
