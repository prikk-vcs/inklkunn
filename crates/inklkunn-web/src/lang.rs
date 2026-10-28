//! Language at the HTTP edge (RFC 003 D-6): which language a request wants, the one cookie that remembers a
//! visitor's choice, where a switch returns to, and whether a form came from a page of this site. Pure
//! functions over headers and addresses; the pages that use them are in [`crate::pages`].
//!
//! The cookie holds a language code and nothing else (`inklkunn_lang=ja`), is `SameSite=Lax`, `HttpOnly` (no
//! script ever reads it: there is none) and `Secure` where the site is served over `https`. It is the only
//! preference stored for an anonymous visitor.

use axum::http::{HeaderMap, Method, Uri, header};

use crate::catalog::Lang;

/// The cookie that carries the visitor's language code.
pub const LANG_COOKIE: &str = "inklkunn_lang";

/// How long the choice is kept: a year.
const COOKIE_MAX_AGE_SECS: u64 = 31_536_000;

/// The longest address a switch will carry back (anything longer returns to the start).
const MAX_BACK: usize = 1024;

/// Addresses whose query may hold something that must not be copied into another page (a sign-in's one-time
/// code), and the switcher's own page: a language switch never returns to them with their query.
const NEVER_COPIED: &[&str] = &["/-/sign-in", "/-/lang"];

/// The value of cookie `name` from the request's `Cookie` header(s), if present.
pub fn get_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then_some(v)
        })
}

/// The page language for a request: the language cookie, else `Accept-Language`, else English.
pub fn negotiate_from_headers(headers: &HeaderMap) -> Lang {
    let accept = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok());
    Lang::negotiate(accept, get_cookie(headers, LANG_COOKIE))
}

/// A `Set-Cookie` value for the language cookie. `secure` is whether the site is served over `https`
/// (plain `http` is for a loopback host in development, and some browsers refuse a `Secure` cookie there).
pub fn language_cookie(tag: &str, secure: bool) -> String {
    format!(
        "{LANG_COOKIE}={tag}; Path=/; Max-Age={COOKIE_MAX_AGE_SECS}; SameSite=Lax; HttpOnly{}",
        if secure { "; Secure" } else { "" }
    )
}

/// A return address that is safe to redirect to and to write into a page: a local path (`/…`), not
/// protocol-relative (`//host`), no backslash, no space or control character, only characters a URL may carry
/// unescaped, within the length limit. Anything else is `None`.
pub fn safe_back(s: &str) -> Option<&str> {
    let local = s.starts_with('/') && !s.starts_with("//");
    let clean = s.chars().all(|c| {
        c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\' | '^' | '`' | '{' | '|' | '}')
    });
    (local && clean && s.len() <= MAX_BACK).then_some(s)
}

/// Where a visitor viewing `method uri` is sent back to after choosing a language: the same address, with its
/// query, for a plain page view; the start page for anything else (a form answer has no address worth
/// returning to) and for the addresses whose query must not be copied into another page: the sign-in's, which may
/// hold a one-time code, and the switcher's own.
pub fn back_for(method: &Method, uri: &Uri) -> String {
    let path = uri.path();
    let copyable = *method == Method::GET
        && !NEVER_COPIED
            .iter()
            .any(|p| path == *p || path.starts_with(&format!("{p}/")));
    let target = match (copyable, uri.query()) {
        (true, Some(q)) => format!("{path}?{q}"),
        (true, None) => path.to_owned(),
        (false, _) => "/".to_owned(),
    };
    safe_back(&target).map_or_else(|| "/".to_owned(), str::to_owned)
}

/// Did this request, which changes something, come from a page of this site? Modern browsers say so with
/// `Sec-Fetch-Site`: only `same-origin` (or `none`, a navigation the person started) is accepted, so a
/// sibling subdomain, such as the content origin, is not trusted. Where it is absent the `Origin` header
/// must name the `Host` the request was addressed to. A request with neither (a command-line client, an old
/// browser) is not a cross-site form post and is accepted: the only thing a language form can do is set a
/// language cookie.
pub fn from_this_site(headers: &HeaderMap) -> bool {
    let value = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = value("sec-fetch-site") {
        return matches!(site, "same-origin" | "none");
    }
    match value("origin") {
        None => true,
        Some(origin) => match (origin.split_once("://"), value("host")) {
            (Some((_, authority)), Some(host)) => authority.eq_ignore_ascii_case(host.trim()),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests;
