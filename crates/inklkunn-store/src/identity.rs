//! Accounts and the credentials that reach them (RFC 002 D-1): **what the service keeps** about who may sign
//! in and how. The *work* of authenticating (hashing, throttling, OpenID Connect, sessions' cookies) is
//! `inklkunn-auth`'s; this module is the records and the traits behind which they are stored, so the
//! in-memory and SQLite stores can be held to one set of invariants.
//!
//! What is stored is never a secret in the form a person or a client holds it: a password is an Argon2id
//! PHC string, and a session id or an API token is the SHA-256 of what the client holds. The store sees
//! them as opaque text.

use std::fmt;

use crate::Result;
use crate::access::Scope;
use crate::record::{AccountId, DisplayName, TokenId};

/// A local sign-in name: 3 to 64 characters of `a-z`, `0-9`, `.`, `_` and `-`, starting with a letter or a
/// digit. Sign-in names are case-insensitive, so [`LoginName::parse`] folds ASCII capitals to lower case and
/// the stored form is always the folded one. It is **not** the account's name anywhere a reader sees (that is
/// the display name) and **not** its identity in signed messages (that is the account id).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoginName(String);

/// Why a sign-in name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginNameError {
    /// Shorter than 3 or longer than 64 characters.
    Length,
    /// A character other than `a-z`, `0-9`, `.`, `_`, `-` (after folding), or a start that is not a letter
    /// or a digit.
    Characters,
}

impl fmt::Display for LoginNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoginNameError::Length => f.write_str("a sign-in name has 3 to 64 characters"),
            LoginNameError::Characters => f.write_str(
                "a sign-in name uses letters a to z, digits, `.`, `_` and `-`, and starts with a letter or a digit",
            ),
        }
    }
}

impl std::error::Error for LoginNameError {}

impl LoginName {
    /// The shortest and longest sign-in name, in characters.
    pub const LENGTH: std::ops::RangeInclusive<usize> = 3..=64;

    /// Validate (and fold) a sign-in name.
    pub fn parse(s: &str) -> std::result::Result<Self, LoginNameError> {
        if !Self::LENGTH.contains(&s.chars().count()) {
            return Err(LoginNameError::Length);
        }
        let folded = s.to_ascii_lowercase();
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c);
        let starts_well = folded
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if !starts_well || !folded.chars().all(allowed) {
            return Err(LoginNameError::Characters);
        }
        Ok(LoginName(folded))
    }

    /// The folded text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LoginName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An identity at an OpenID Connect provider: the issuer and the provider's stable subject (the `iss` and
/// `sub` claims). Linking is by this pair and by nothing else; an e-mail address a provider reports is never
/// an identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalIdentity {
    issuer: String,
    subject: String,
}

/// Why an external identity was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalIdentityError {
    /// The issuer is empty, longer than 512 bytes, or holds a control character or whitespace.
    Issuer,
    /// The subject is empty, longer than 255 bytes (OpenID Connect Core 2), or holds a control character.
    Subject,
}

impl fmt::Display for ExternalIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExternalIdentityError::Issuer => f.write_str("the issuer is not acceptable"),
            ExternalIdentityError::Subject => f.write_str("the subject is not acceptable"),
        }
    }
}

impl std::error::Error for ExternalIdentityError {}

impl ExternalIdentity {
    /// Validate an issuer and a subject.
    pub fn new(issuer: &str, subject: &str) -> std::result::Result<Self, ExternalIdentityError> {
        if issuer.is_empty()
            || issuer.len() > 512
            || issuer.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(ExternalIdentityError::Issuer);
        }
        if subject.is_empty() || subject.len() > 255 || subject.chars().any(char::is_control) {
            return Err(ExternalIdentityError::Subject);
        }
        Ok(ExternalIdentity {
            issuer: issuer.to_owned(),
            subject: subject.to_owned(),
        })
    }

    /// The provider's issuer URL.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The provider's subject identifier.
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

/// A local password: the sign-in name and the Argon2id PHC string that verifies it. A password without a
/// name, or a name without a password, cannot be stored, so neither can exist half-made.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalCredential {
    /// The sign-in name.
    pub login: LoginName,
    /// The PHC string (`$argon2id$…`). Never a password.
    pub password_hash: String,
}

impl fmt::Debug for LocalCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalCredential")
            .field("login", &self.login)
            .field("password_hash", &"…")
            .finish()
    }
}

/// An account (RFC 002 D-1): who signs in, never what they may do (that is a role grant) and never what
/// they are called in a signed message (that is the id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The opaque id.
    pub id: AccountId,
    /// The free-text name readers see.
    pub display_name: DisplayName,
    /// The local password, if the account has one. An account may have this, a linked external identity, or
    /// both.
    pub local: Option<LocalCredential>,
    /// Unix seconds.
    pub created_at: i64,
    /// A disabled account cannot sign in or use a token; its history stays attributed to it.
    pub disabled: bool,
}

/// The account records. Every implementation passes the same invariant tests (`tests/invariants.rs`).
pub trait AccountStore: Send + Sync {
    /// Store a new account. [`crate::StoreError::IdTaken`] when the id exists,
    /// [`crate::StoreError::LoginTaken`] when another account has the sign-in name; nothing is stored then.
    fn add_account(&self, account: &Account) -> Result<()>;
    /// The account with this id.
    fn account(&self, id: &AccountId) -> Result<Option<Account>>;
    /// The account that signs in with this name.
    fn account_by_login(&self, login: &LoginName) -> Result<Option<Account>>;
    /// The account linked to this external identity.
    fn account_by_identity(&self, identity: &ExternalIdentity) -> Result<Option<Account>>;
    /// Link an external identity to an account. Idempotent for the same account;
    /// [`crate::StoreError::IdentityTaken`] when another account holds it,
    /// [`crate::StoreError::AccountNotFound`] when the account is not there.
    fn link_identity(&self, id: &AccountId, identity: &ExternalIdentity, at: i64) -> Result<()>;
    /// Replace (or, with `None`, remove) the account's local password.
    /// [`crate::StoreError::LoginTaken`] when another account has the name.
    fn set_local_credential(
        &self,
        id: &AccountId,
        credential: Option<&LocalCredential>,
    ) -> Result<()>;
    /// Disable or re-enable an account.
    fn set_disabled(&self, id: &AccountId, disabled: bool) -> Result<()>;
}

/// A browser session as stored: the **hash** of the id the cookie holds, the account, the CSRF token that
/// belongs to this session, and when it was made and when it ends.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionRecord {
    /// `sha256$<hex>` of the session id. The id itself is never stored.
    pub id_hash: String,
    /// The account.
    pub account: AccountId,
    /// The synchronizer token that every write made with this session must present.
    pub csrf: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds; the session is over at this instant.
    pub expires_at: i64,
}

impl fmt::Debug for SessionRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionRecord")
            .field("account", &self.account)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// The session records. Every implementation passes the same invariant tests.
pub trait SessionStore: Send + Sync {
    /// Store a session. The hash must be new ([`crate::StoreError::IdTaken`]).
    fn add_session(&self, session: &SessionRecord) -> Result<()>;
    /// The session with this hash, unless it is unknown or over at `now`.
    fn session(&self, id_hash: &str, now: i64) -> Result<Option<SessionRecord>>;
    /// End one session; `true` when it existed.
    fn end_session(&self, id_hash: &str) -> Result<bool>;
    /// End every session of an account (a password change, a disabling); how many there were.
    fn end_sessions_of(&self, id: &AccountId) -> Result<u64>;
    /// Remove sessions that are over at `now`; how many.
    fn purge_expired_sessions(&self, now: i64) -> Result<u64>;
}

/// An API token as stored (RFC 002 D-1): hashed, scoped, expiring and revocable. The token itself is shown
/// once, when it is issued, and never again.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenRecord {
    /// The token's id, for listing and revoking; not a secret.
    pub id: TokenId,
    /// `sha256$<hex>` of the token.
    pub hash: String,
    /// The account the token acts for.
    pub account: AccountId,
    /// The most the token may do, before it is intersected with the account's role.
    pub scope: Scope,
    /// What the owner called it (one line, at most 80 characters).
    pub label: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds; `None` for a token that does not expire.
    pub expires_at: Option<i64>,
    /// Unix seconds; set once, when the token was revoked.
    pub revoked_at: Option<i64>,
}

impl fmt::Debug for TokenRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenRecord")
            .field("id", &self.id)
            .field("account", &self.account)
            .field("scope", &self.scope)
            .field("label", &self.label)
            .field("expires_at", &self.expires_at)
            .field("revoked_at", &self.revoked_at)
            .finish_non_exhaustive()
    }
}

/// A token label as stored: one line of at most 80 characters, control characters made `?`.
pub fn clean_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                '?'
            } else {
                c
            }
        })
        .take(80)
        .collect()
}

/// The token records. Every implementation passes the same invariant tests.
pub trait TokenStore: Send + Sync {
    /// Store a token. The id and the hash must be new ([`crate::StoreError::IdTaken`]).
    fn add_token(&self, token: &TokenRecord) -> Result<()>;
    /// The token with this hash, whatever its state (the caller decides about expiry and revocation).
    fn token_by_hash(&self, hash: &str) -> Result<Option<TokenRecord>>;
    /// An account's tokens, oldest first.
    fn tokens_of(&self, account: &AccountId) -> Result<Vec<TokenRecord>>;
    /// Revoke the account's token `id` at `at`; `true` when it existed, was the account's and was not
    /// already revoked. A token of another account is never touched (so a revocation can be offered a
    /// guessed id harmlessly).
    fn revoke_token(&self, account: &AccountId, id: &TokenId, at: i64) -> Result<bool>;
}

#[cfg(test)]
mod tests;
