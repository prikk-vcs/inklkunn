//! Browser sessions and CSRF tokens (RFC 002 D-1; AUTH-01).
//!
//! A signed-in browser holds one **opaque, random session id** in a cookie that is `Secure`, `HttpOnly` and
//! `SameSite=Strict`, with the `__Host-` prefix (so no sibling host can set or shadow it, and it cannot be
//! set over plain HTTP). The server keeps the rest: the session record holds the SHA-256 of the id, the
//! account, the end time and the session's own CSRF token. A copy of the database opens no session.
//!
//! Every write made with a session carries that session's CSRF token (the *synchronizer token* pattern),
//! compared in constant time. The sign-in form itself has no session yet, so it uses a *double-submit*
//! token: a random value in a `__Host-` cookie and the same value in the form.

use std::fmt;
use std::sync::Arc;

use inklkunn_store::{AccountId, SessionRecord, SessionStore};

use crate::error::AuthError;
use crate::random;
use crate::stored::{same_secret, stored_form};

/// The session cookie's name.
pub const SESSION_COOKIE: &str = "__Host-inklkunn-session";
/// The sign-in form's double-submit cookie.
pub const CSRF_COOKIE: &str = "__Host-inklkunn-csrf";
/// A session lasts seven days from sign-in.
pub const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;
/// A sign-in form's token is good for an hour.
const FORM_TOKEN_TTL_SECS: i64 = 3600;

/// The value of the session cookie. A secret: it is shown to nobody and logged nowhere.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionSecret(String);

impl SessionSecret {
    /// The cookie value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionSecret(…)")
    }
}

/// A CSRF token: put in a form's hidden field (or a header), compared on the write.
#[derive(Clone, PartialEq, Eq)]
pub struct CsrfToken(String);

impl CsrfToken {
    /// A fresh token.
    pub fn mint() -> Result<Self, AuthError> {
        random::token().map(CsrfToken)
    }

    /// The text for a hidden field or a header.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Does what the request presented equal this token? Constant-time; an empty or missing value never
    /// matches.
    pub fn matches(&self, presented: &str) -> bool {
        !presented.is_empty() && same_secret(&self.0, presented)
    }

    /// The `Set-Cookie` value for a sign-in form's double-submit cookie.
    pub fn form_cookie(&self) -> String {
        format!(
            "{CSRF_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={FORM_TOKEN_TTL_SECS}",
            self.0
        )
    }
}

impl fmt::Debug for CsrfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CsrfToken(…)")
    }
}

/// Is the sign-in form's token the one in the cookie? `cookie_header` is the request's `Cookie` header.
pub fn form_token_matches(cookie_header: Option<&str>, field: &str) -> bool {
    cookie_header
        .and_then(|h| cookie_value(h, CSRF_COOKIE))
        .is_some_and(|c| !field.is_empty() && same_secret(c, field))
}

/// A session just made: what to put in the cookie and in the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    secret: SessionSecret,
    csrf: CsrfToken,
    expires_at: i64,
}

impl NewSession {
    /// The secret (for the cookie).
    pub fn secret(&self) -> &SessionSecret {
        &self.secret
    }

    /// The CSRF token for this session's forms.
    pub fn csrf(&self) -> &CsrfToken {
        &self.csrf
    }

    /// When the session ends, in Unix seconds.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// The `Set-Cookie` value: `Secure`, `HttpOnly`, `SameSite=Strict`, `__Host-`, ending with the session.
    pub fn set_cookie(&self, now: i64) -> String {
        let max_age = (self.expires_at - now).max(0);
        format!(
            "{SESSION_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={max_age}",
            self.secret.0
        )
    }
}

/// The `Set-Cookie` value that removes the session cookie (sign-out).
pub fn clear_cookie() -> String {
    format!("{SESSION_COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")
}

/// The value of cookie `name` in a `Cookie` header. A name that appears twice is ambiguous and yields
/// nothing (a stray copy of a cookie must not pick which one is believed).
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    let mut found = None;
    for pair in header.split(';') {
        let Some((n, v)) = pair.split_once('=') else {
            continue;
        };
        if n.trim() == name {
            if found.is_some() {
                return None;
            }
            found = Some(v.trim());
        }
    }
    found
}

/// The session secret in a `Cookie` header, if there is exactly one well-formed one.
pub fn secret_from_cookie_header(header: &str) -> Option<&str> {
    cookie_value(header, SESSION_COOKIE).filter(|v| looks_like_a_secret(v))
}

/// 43 base64url characters: what [`random::token`] makes. Anything else is not worth a lookup.
fn looks_like_a_secret(v: &str) -> bool {
    v.len() == 43
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A session found by its secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedSession {
    /// The account.
    pub(crate) account: AccountId,
    /// The session's CSRF token.
    pub(crate) csrf: CsrfToken,
    /// When it ends.
    pub(crate) expires_at: i64,
}

/// Starting, finding and ending sessions over a store.
pub(crate) struct Sessions {
    store: Arc<dyn SessionStore>,
    ttl_secs: i64,
}

impl Sessions {
    /// Sessions that last `ttl_secs`.
    pub(crate) fn new(store: Arc<dyn SessionStore>, ttl_secs: i64) -> Self {
        Sessions { store, ttl_secs }
    }

    /// Start a session for `account`: a new secret every time, so a session id never survives a sign-in
    /// (no fixation). Also removes the sessions that have ended.
    pub(crate) fn start(&self, account: &AccountId, now: i64) -> Result<NewSession, AuthError> {
        let secret = SessionSecret(random::token()?);
        let csrf = CsrfToken::mint()?;
        let expires_at = now.saturating_add(self.ttl_secs);
        self.store.purge_expired_sessions(now)?;
        self.store.add_session(&SessionRecord {
            id_hash: stored_form(&secret.0),
            account: account.clone(),
            csrf: csrf.0.clone(),
            created_at: now,
            expires_at,
        })?;
        Ok(NewSession {
            secret,
            csrf,
            expires_at,
        })
    }

    /// The session this secret names, unless it is unknown or over.
    pub(crate) fn resolve(
        &self,
        secret: &str,
        now: i64,
    ) -> Result<Option<ResolvedSession>, AuthError> {
        if !looks_like_a_secret(secret) {
            return Ok(None);
        }
        Ok(self
            .store
            .session(&stored_form(secret), now)?
            .map(|s| ResolvedSession {
                account: s.account,
                csrf: CsrfToken(s.csrf),
                expires_at: s.expires_at,
            }))
    }

    /// End one session (sign-out); `true` when it existed.
    pub(crate) fn end(&self, secret: &str) -> Result<bool, AuthError> {
        Ok(self.store.end_session(&stored_form(secret))?)
    }

    /// End every session of an account (a new password, a disabling); how many there were.
    pub(crate) fn end_all(&self, account: &AccountId) -> Result<u64, AuthError> {
        Ok(self.store.end_sessions_of(account)?)
    }
}

#[cfg(test)]
mod tests;
