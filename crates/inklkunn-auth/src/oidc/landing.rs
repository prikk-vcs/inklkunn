//! The page the OpenID Connect callback answers with (RFC 002 D-1; handoff 002 C9).
//!
//! **Why a page and not a redirect.** The session cookie is `SameSite=Strict`. The callback is a navigation the
//! identity provider started on another site, so the browser treats everything that follows it in the same
//! navigation as cross-site: a `302` to the landing page would be requested **without** the Strict cookie the
//! callback's own response has just set, and the person would see the site signed out. A `200` page that moves
//! the browser on by itself (`<meta http-equiv="refresh">`) is a new navigation started by *this* site's page, so
//! the browser sends the Strict cookie with it. [`Landing`] is what that answer is made of: the status, the
//! headers, the cookies and the path to move on to. **The page itself is the web layer's** (handoff 002's F4):
//! its words are catalog messages in the reader's language, so this crate hands over the target, the cookies
//! and the headers and writes no sentence (see [`Landing::status`], [`Landing::headers`],
//! [`Landing::set_cookies`], [`Landing::target`]).
//!
//! A `Landing` has no public constructor: the only one in existence comes out of a successful
//! [`crate::Authenticator::finish_oidc`], together with the session it belongs to, so a handler cannot sign
//! somebody in and then redirect.

/// The longest landing path the page will carry; a longer one is replaced by `/`.
const MAX_TARGET_BYTES: usize = 2048;

/// The headers the page is sent with. The page needs no script, no style and no sub-resource, so its policy
/// allows none (the web layer's header layer sets the policy of the whole site on every response, which allows
/// no more); it must never be cached (it sits behind a one-time sign-in) and must not leak the callback's
/// address, whose query holds the provider's code, as a referrer (the page also says so in a `<meta
/// name="referrer">`, because the site's own header layer sends `same-origin`).
const HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "text/html; charset=utf-8"),
    ("Cache-Control", "no-store"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Content-Type-Options", "nosniff"),
    (
        "Content-Security-Policy",
        "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
];

/// What a successful OpenID Connect callback must be answered with: a `200` page (the web layer draws it) that
/// moves the browser to [`Landing::target`] with a request of this site's own, plus the headers and the cookies
/// to set. **Send all of it as it is**: answering with a redirect instead loses the session cookie on the next
/// request (see the module documentation). Not constructible outside this crate.
#[derive(Clone, PartialEq, Eq)]
#[must_use = "the callback must be answered with this page; a redirect would lose the Strict session cookie"]
pub struct Landing {
    target: String,
    set_cookies: Vec<String>,
}

impl std::fmt::Debug for Landing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The cookies hold the session secret.
        f.debug_struct("Landing")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl Landing {
    /// The page for `next`, a path on this site (`/`, `/handbook/Home?x=1`), with the cookies to set.
    /// Anything that is not such a path (a full URL, `//host`, a backslash, a control character, a path
    /// that is too long, nothing) is replaced by `/`: the person is signed in either way, and where to go
    /// is never worth refusing a sign-in for.
    pub(crate) fn new(next: Option<&str>, set_cookies: Vec<String>) -> Self {
        let target = next.filter(|p| is_local_path(p)).unwrap_or("/").to_owned();
        Landing {
            target,
            set_cookies,
        }
    }

    /// The status to answer with: always `200`. (Never a `3xx`.)
    pub const fn status(&self) -> u16 {
        200
    }

    /// The headers to answer with (name, value).
    pub fn headers(&self) -> &'static [(&'static str, &'static str)] {
        HEADERS
    }

    /// The `Set-Cookie` values, one header each: the new session's cookie and the removal of the one-shot
    /// login-state cookie.
    pub fn set_cookies(&self) -> &[String] {
        &self.set_cookies
    }

    /// The path the page moves the browser to: a path on this site, always (`/` when the one asked for was
    /// not).
    pub fn target(&self) -> &str {
        &self.target
    }
}

/// Is `p` a path on this site that a page may send the browser to: it starts with one `/` (not `//`, which
/// names another host), and holds no backslash (some browsers read it as `/`), no control character and no
/// space.
fn is_local_path(p: &str) -> bool {
    p.starts_with('/')
        && !p.starts_with("//")
        && p.len() <= MAX_TARGET_BYTES
        && !p
            .chars()
            .any(|c| c == '\\' || c == ' ' || c.is_control() || c == '\u{2028}' || c == '\u{2029}')
}

#[cfg(test)]
mod tests;
