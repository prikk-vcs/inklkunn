//! A page's stored file, byte for byte, **on the content origin only** (RFC 003 D-3, D-10; threat model T-6;
//! requirements RD-10, API-2): `/{slug}/-/raw/{title}` answers on the content origin and is refused on the
//! application origin.
//!
//! Wiki-authored bytes never come from the origin that holds the sign-in cookie: a file that is served as it
//! was written can be anything its author wrote, and the content origin is where that is harmless. The answer
//! is `text/markdown; charset=UTF-8` with the stored bytes untouched, `nosniff` and the content policy
//! (`sandbox`, `default-src 'none'`) from the headers layer, `noindex` (the page is the thing to index, not
//! its source), `ETag` and `Last-Modified` (see [`inklkunn_core::read::RawPage`]) with conditional requests
//! answered `304`, and a `Link` to the page's canonical address.
//!
//! **Access is the page's.** The same read check as the page view, existence-neutral: a wiki that is not there,
//! one the principal may not read, and a title that leads to no page all answer the same plain-text
//! not-found. The content origin sees no session cookie on a deployment with its own host (cookies do not
//! cross hosts), so there it serves the wikis anyone may read; reading a restricted wiki's source needs a way
//! to carry the reader across, which is a decision of its own (reported with this task).

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use inklkunn_core::read::ReadError;

use crate::catalog::{Key, T};
use crate::discover::query_escape;
use crate::lang::negotiate_from_headers;
use crate::pages::{self, Page, PageState};
use crate::time::http_date;
use crate::url::encode_title;
use crate::wiki::{blocking, generic_not_found, open, with_retry_after};

/// The same sentence for every reason there is nothing to serve.
fn nothing(headers: &HeaderMap) -> Response {
    let language = negotiate_from_headers(headers);
    pages::plain(
        StatusCode::NOT_FOUND,
        language,
        T::new(language).get(Key::NotFoundBody),
    )
}

/// Does an `If-None-Match` header list `etag` (or `*`)? Weak comparison, as RFC 9110 prescribes here.
fn none_match(header_value: &str, etag: &str) -> bool {
    header_value.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

/// `GET /{slug}/-/raw/{*title}` on the content origin.
pub(crate) async fn raw_content(
    State(state): State<PageState>,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Response {
    let Ok(Path((slug, title))) = path else {
        return nothing(&headers);
    };
    let Some(services) = state.services.clone() else {
        return nothing(&headers);
    };
    let language = negotiate_from_headers(&headers);
    let request_headers = headers.clone();
    let read = blocking(move || {
        let wiki = open(&services, &request_headers, &slug)?;
        let raw = services.read.raw(&wiki, None, &title);
        Some((wiki, raw))
    })
    .await
    .flatten();
    let (wiki, raw) = match read {
        Some((wiki, Ok(Some(raw)))) => (wiki, raw),
        Some((
            _,
            Ok(None)
            | Err(
                ReadError::InvalidTitle(_)
                | ReadError::NoSuchView(_)
                | ReadError::PageUnreadable(_),
            ),
        ))
        | None => {
            return nothing(&headers);
        }
        Some((_, Err(ReadError::CouldNotBeReadNow(_)))) => {
            return with_retry_after(pages::plain(
                StatusCode::SERVICE_UNAVAILABLE,
                language,
                T::new(language).get(Key::UnavailableBody),
            ));
        }
    };
    let last_modified = http_date(raw.last_modified);
    let not_modified = match headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        Some(candidates) => none_match(candidates, &raw.etag),
        None => headers
            .get(header::IF_MODIFIED_SINCE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|since| since.trim() == last_modified),
    };
    let mut res = if not_modified {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let mut res = (StatusCode::OK, raw.bytes).into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/markdown; charset=UTF-8"),
        );
        res
    };
    let h = res.headers_mut();
    let set = |h: &mut HeaderMap, name: HeaderName, value: &str| {
        if let Ok(v) = HeaderValue::from_str(value) {
            h.insert(name, v);
        }
    };
    set(h, header::ETAG, &raw.etag);
    set(h, header::LAST_MODIFIED, &last_modified);
    set(
        h,
        HeaderName::from_static("x-robots-tag"),
        "noindex, nofollow",
    );
    if !not_modified {
        set(
            h,
            header::CONTENT_DISPOSITION,
            &format!("inline; filename*=UTF-8''{}.md", query_escape(&raw.title)),
        );
    }
    set(
        h,
        header::LINK,
        &format!(
            "<{}>; rel=\"canonical\"",
            state
                .origins
                .app(&format!("/{}/{}", wiki.slug, encode_title(&raw.title)))
        ),
    );
    set(h, header::CONTENT_LANGUAGE, language.tag());
    res
}

/// `GET /{slug}/-/raw/{*title}` on the application origin: refused, with the page every missing address gets.
/// The source is on the content origin (the page's head says where); this origin serves no file bytes.
pub(crate) async fn raw_refused(page: Page) -> Response {
    generic_not_found(&page)
}
