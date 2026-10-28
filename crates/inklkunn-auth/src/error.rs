//! What authenticating can answer, kept coarse on purpose (threat model T-7): a caller learns *that* a
//! credential did not authenticate and nothing about which part was wrong.

use std::fmt;

/// Why a credential did not authenticate, or why authenticating could not be attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthError {
    /// The credential did not authenticate: no such account, a wrong password, an unknown, expired,
    /// revoked or out-of-scope token, a disabled account. **These are one answer**, so the response never
    /// says which.
    BadCredentials,
    /// Too many failures for this sign-in name or this client address: no attempt is made (a correct
    /// password is refused too) until the instant, in Unix seconds.
    Locked {
        /// When the lock ends.
        until: i64,
    },
    /// The service is hashing as many passwords as it allows at once and could not take this one in time.
    /// Nothing was decided about the credential; trying again shortly is right.
    Busy,
    /// The authenticator was set up with a value that cannot work (a zero session lifetime, no permits); it
    /// refuses to start rather than run in a way nobody asked for.
    Misconfigured(&'static str),
    /// A store, or the operating system's random source, failed. The text is for operators and carries no
    /// credential.
    Backend(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::BadCredentials => f.write_str("authentication failed"),
            AuthError::Locked { .. } => f.write_str("too many failed attempts; try again later"),
            AuthError::Busy => f.write_str("the service is busy; try again shortly"),
            AuthError::Misconfigured(why) => write!(f, "authentication is misconfigured: {why}"),
            AuthError::Backend(e) => write!(f, "authentication backend error: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<inklkunn_store::StoreError> for AuthError {
    fn from(e: inklkunn_store::StoreError) -> Self {
        AuthError::Backend(e.to_string())
    }
}

/// The crate's result type.
pub type Result<T> = std::result::Result<T, AuthError>;
