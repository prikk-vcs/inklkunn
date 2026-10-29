//! The page the OpenID Connect callback answers with (RFC 002 D-1; handoff 002 C9).
//!
//! **Why a page and not a redirect.** The session cookie is `SameSite=Strict`. The callback is a navigation the
//! identity provider started on another site, so the browser treats everything that follows it in the same
//! navigation as cross-site: a `302` to the landing page would be requested **without** the Strict cookie the
//! callback's own response has just set, and the person would see the site signed out. A `200` page that moves
//! the browser on by itself (`<meta http-equiv="refresh">`) is a new navigation started by *this* site's page, so
//! the browser sends the Strict cookie with it. [`Landing`] is that page, with the status, the headers and the
//! cookies that go with it, and the web layer sends exactly what it holds (see [`Landing::status`],
//! [`Landing::headers`], [`Landing::set_cookies`], [`Landing::body`]).
//!
//! A `Landing` has no public constructor: the only one in existence comes out of a successful
//! [`crate::Authenticator::finish_oidc`], together with the session it belongs to, so a handler cannot sign
//! somebody in and then redirect.

use std::fmt::Write as _;

/// The longest landing path the page will carry; a longer one is replaced by `/`.
const MAX_TARGET_BYTES: usize = 2048;

/// The headers the page is sent with. The page needs no script, no style and no sub-resource, so its policy
/// allows none; it must never be cached (it sits behind a one-time sign-in) and must not leak the callback's
/// address, whose query holds the provider's code, as a referrer.
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

/// What a successful OpenID Connect callback must be answered with: a `200` page that moves the browser to
/// the landing path with a request of this site's own, plus the cookies to set. **Send all of it as it is**:
/// answering with a redirect instead loses the session cookie on the next request (see the module
/// documentation). Not constructible outside this crate.
#[derive(Clone, PartialEq, Eq)]
#[must_use = "the callback must be answered with this page; a redirect would lose the Strict session cookie"]
pub struct Landing {
    target: String,
    set_cookies: Vec<String>,
    body: String,
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
        let body = page(&target);
        Landing {
            target,
            set_cookies,
            body,
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

    /// The page: HTML, UTF-8.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The path the page moves the browser to.
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

/// Escape for HTML text and for a double-quoted attribute.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn page(target: &str) -> String {
    let t = escape(target);
    let mut out = String::with_capacity(400 + 2 * t.len());
    // Writing to a String cannot fail.
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <meta http-equiv=\"refresh\" content=\"0;url={t}\">\n\
         <title>Signing in</title>\n</head>\n<body>\n\
         <p>Signing you in. If this page does not move on by itself, <a href=\"{t}\">continue</a>.</p>\n\
         </body>\n</html>\n"
    );
    out
}

#[cfg(test)]
mod tests;
