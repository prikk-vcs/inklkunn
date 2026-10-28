//! The one stylesheet (RFC 003 D-3, D-6; INC-5), served from the application origin. The page policy allows
//! `style-src 'self'` and nothing else, so the sheet is a file the application itself serves: no inline
//! `style` attribute, no `<style>` element, no web font, no other origin.
//!
//! Its address carries a fingerprint of its bytes (`/-/assets/site-<fingerprint>.css`), so it can be cached
//! for a year and marked `immutable`: a changed sheet has a new address, and the pages, which are always
//! revalidated, name the current one. An older address answers 404 rather than serve a sheet its name does
//! not describe. The handler sets its own `Cache-Control` (and `Vary`); the headers layer fills in only what a
//! handler left out and never touches the security headers.

use std::sync::OnceLock;

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::pages::Page;

/// The stylesheet, embedded in the binary.
pub const CSS: &str = include_str!("../static/site.css");

/// The route pattern of the sheet (a public route: it serves no wiki data).
pub const ROUTE: &str = "/-/assets/{file}";

/// What a cache may keep the sheet for: a year, never revalidated (its name changes with its bytes).
const CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// FNV-1a over the bytes: a short name that changes when the sheet does. It is a cache key, not a security
/// measure, so a fast hash with no secret is the right size.
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The file name the sheet is served under: `site-<16 hex digits>.css`.
pub fn file_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| format!("site-{:016x}.css", fingerprint(CSS.as_bytes())))
}

/// The address the pages link to.
pub fn path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| format!("/-/assets/{}", file_name()))
}

/// `GET /-/assets/{file}`: the sheet under its current name, a 404 page under any other.
pub(crate) async fn serve(Path(file): Path<String>, page: Page) -> Response {
    if file != file_name() {
        return crate::pages::not_found(page).await;
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, CACHE_CONTROL),
            // The sheet does not depend on the language or on a cookie; a proxy may compress it.
            (header::VARY, "Accept-Encoding"),
        ],
        CSS,
    )
        .into_response()
}

#[cfg(test)]
mod tests;
