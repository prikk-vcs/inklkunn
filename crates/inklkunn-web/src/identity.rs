//! The real `Identify` (the sign-in handoff's U4): a request becomes a principal here and nowhere else.
//!
//! * **A session cookie** resolves through `Authenticator::principal_for_session` for a read, and **only**
//!   through `Authenticator::principal_for_write` for a write, which checks the cookie and the CSRF token the
//!   request presented together (handoff 004 P6; RFC 002 D-1). A cookie that is live with no token, or another
//!   session's, is [`WriteIdentity::CsrfMismatch`], never an anonymous visitor and never the account.
//! * **A bearer token** resolves through `Authenticator::principal_for_token` (a hashed lookup; the principal
//!   carries the token's scope, which the authorizer intersects with the account's role). **A request that
//!   presents a bearer token is judged by the token alone**: an invalid, expired or revoked token is
//!   *invalid token* (a write) or anonymous (a read), never a fall back to a cookie the request also carries,
//!   and a valid token is not narrowed or widened by one (the reading's F7).
//! * Another scheme in `Authorization` (an operator's proxy may add Basic) is not a token of ours and does not
//!   stop a cookie from being believed.
//!
//! This module only asks the authenticator and passes along the principal it made. The `IdentityAuthority` that
//! makes principals is held by the authenticator alone, claimed once by the binary (corrections handoff C7).

use std::sync::Arc;

use axum::http::{HeaderMap, header};
use inklkunn_auth::session::{SESSION_COOKIE, secret_from_cookie_header};
use inklkunn_auth::{AuthError, Authenticator, SessionCheck};
use inklkunn_core::Principal;
use inklkunn_store::AccountStore;

use crate::services::{Identify, SessionView, WriteIdentity};
use crate::signin::names::BEARER;

/// What a request presents in `Authorization`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Presented<'a> {
    /// No bearer token (no header, or a scheme that is not ours).
    Nothing,
    /// One `Bearer <token>`.
    Token(&'a str),
    /// A bearer credential that cannot be a token: no token, a space in it, or two `Authorization` headers.
    Malformed,
}

/// Read `Authorization` as a bearer token. Any header at all that names the `Bearer` scheme means the request
/// presents a token, good or not; only then is the cookie out of the question.
pub(crate) fn presented(headers: &HeaderMap) -> Presented<'_> {
    let values: Vec<&str> = headers
        .get_all(header::AUTHORIZATION)
        .iter()
        .map(|v| v.to_str().unwrap_or(""))
        .collect();
    let names_bearer = |v: &str| {
        v.split(' ')
            .next()
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case(BEARER))
    };
    if !values.iter().any(|v| names_bearer(v)) {
        return Presented::Nothing;
    }
    match values.as_slice() {
        [one] => match one.split_once(' ') {
            Some((_, token)) if !token.is_empty() && !token.contains(' ') => {
                Presented::Token(token)
            }
            _ => Presented::Malformed,
        },
        // Two headers, one of them a bearer credential: not a token, and not a visitor either.
        _ => Presented::Malformed,
    }
}

/// The session secret the request's cookies carry, if exactly one well-formed one.
pub(crate) fn session_secret(headers: &HeaderMap) -> Option<String> {
    let joined = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join("; ");
    secret_from_cookie_header(&joined).map(str::to_owned)
}

/// Does the request carry a cookie that could be a session (its name is there)? Cheap, no lookup: a request
/// without one has nothing to forge.
pub(crate) fn carries_session_cookie(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|line| line.contains(SESSION_COOKIE))
}

/// `Identify` over the authenticator.
pub struct AuthIdentity {
    auth: Arc<Authenticator>,
    accounts: Arc<dyn AccountStore>,
    clock: fn() -> i64,
    log: Arc<dyn Fn(&str) + Send + Sync>,
}

impl std::fmt::Debug for AuthIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthIdentity")
    }
}

impl AuthIdentity {
    /// Identity over `auth`. `accounts` names the person the header shows; `clock` is Unix seconds; `log` is
    /// where a failure of the store goes (a page never shows it).
    pub fn new(
        auth: Arc<Authenticator>,
        accounts: Arc<dyn AccountStore>,
        clock: fn() -> i64,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Self {
        AuthIdentity {
            auth,
            accounts,
            clock,
            log,
        }
    }

    /// An authentication that failed for a reason worth the operator's line (not a credential that is simply
    /// not good).
    fn noted<T>(&self, result: Result<T, AuthError>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(AuthError::BadCredentials) => None,
            Err(e) => {
                (self.log)(&format!("a credential could not be checked: {e}"));
                None
            }
        }
    }
}

impl Identify for AuthIdentity {
    fn principal(&self, headers: &HeaderMap) -> Principal {
        let now = (self.clock)();
        match presented(headers) {
            Presented::Token(token) => self
                .noted(self.auth.principal_for_token(token, now))
                .unwrap_or(Principal::Anonymous),
            // Judged by the token alone, and this one is not a token.
            Presented::Malformed => Principal::Anonymous,
            Presented::Nothing => session_secret(headers)
                .and_then(|secret| self.noted(self.auth.principal_for_session(&secret, now)))
                .flatten()
                .map_or(Principal::Anonymous, |s| s.principal),
        }
    }

    fn may_be_signed_in(&self, headers: &HeaderMap) -> bool {
        carries_session_cookie(headers) && presented(headers) == Presented::Nothing
    }

    fn session(&self, headers: &HeaderMap) -> Option<SessionView> {
        if presented(headers) != Presented::Nothing {
            return None;
        }
        let secret = session_secret(headers)?;
        let found = self
            .noted(self.auth.principal_for_session(&secret, (self.clock)()))
            .flatten()?;
        let display_name = match self.accounts.account(&found.account) {
            Ok(Some(account)) => account.display_name.as_str().to_owned(),
            _ => return None,
        };
        Some(SessionView {
            account: found.account.clone(),
            display_name,
            csrf: found.csrf.as_str().to_owned(),
        })
    }

    fn for_write(&self, headers: &HeaderMap, csrf: Option<&str>) -> WriteIdentity {
        let now = (self.clock)();
        match presented(headers) {
            // Judged by the token alone. A token is not sent by a browser by itself, so it needs no CSRF token;
            // one that does not authenticate is `invalid token`, whatever cookies come with it.
            Presented::Token(token) => self
                .noted(self.auth.principal_for_token(token, now))
                .map_or(WriteIdentity::InvalidToken, WriteIdentity::Principal),
            Presented::Malformed => WriteIdentity::InvalidToken,
            Presented::Nothing => {
                let Some(secret) = session_secret(headers) else {
                    return WriteIdentity::Principal(Principal::Anonymous);
                };
                // The cookie and the token, checked together, or no session at all.
                match self
                    .auth
                    .principal_for_write(&secret, csrf.unwrap_or(""), now)
                {
                    Ok(SessionCheck::Valid(s)) => WriteIdentity::Principal(s.principal),
                    Ok(SessionCheck::CsrfMismatch) => WriteIdentity::CsrfMismatch,
                    Ok(SessionCheck::SignedOut) => WriteIdentity::Principal(Principal::Anonymous),
                    Err(e) => {
                        (self.log)(&format!("a session could not be checked: {e}"));
                        // Closed: an outage is not a licence to act.
                        WriteIdentity::CsrfMismatch
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
