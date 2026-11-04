//! *Why is this here?* (RFC 005 part 2 D-8; handoff K-T7): the page every wiki page links to among its tools.
//!
//! **The page itself is K-T7's.** Until it lands, the link must not lead to a page that is not there, so this
//! route answers, for every title, one plain and honest state: *this is not available in this release*, with a
//! link back to the page. It reads nothing of the wiki beyond who may read it, so it is the same answer for a
//! page that exists and for one that does not (existence-neutral), and for a view it does not look up. K-T7
//! replaces `why_page` and keeps the address.

use std::collections::HashMap;

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::read::Presentation;

use crate::catalog::Key;
use crate::pages::{Page, PageState};
use crate::present::WikiLinks;
use crate::url::view_ref;
use crate::wiki::{
    Choice, Shown, blocking, generic_not_found, notice, open, present, render_failed, services_of,
};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// What the request came to, once the wiki is known to be the visitor's to read.
enum Asked {
    /// A page the wiki already has a word for.
    Shown(Shown),
    /// The wiki, and the view the address names.
    Wiki(Box<inklkunn_store::WikiRecord>, Option<String>),
}

/// `GET /{slug}/-/why/{*title}`.
pub(crate) async fn why_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, title))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let asked = blocking(move || {
        let Some(wiki) = open(&services, &headers, &slug) else {
            return Asked::Shown(Shown::NotFound);
        };
        match params.get("view") {
            None => Asked::Wiki(Box::new(wiki), None),
            Some(name) if view_ref(name).is_some() => {
                Asked::Wiki(Box::new(wiki), Some(name.clone()))
            }
            Some(name) => Asked::Shown(Shown::NoSuchView {
                wiki: Box::new(wiki),
                view: name.clone(),
            }),
        }
    })
    .await;
    match asked {
        None => render_failed(&page),
        Some(Asked::Shown(shown)) => present(&page, shown),
        Some(Asked::Wiki(wiki, view)) => not_available(&page, &wiki, view.as_deref(), &title),
    }
}

/// The state: not part of this release, and the way back to the page asked about.
fn not_available(
    page: &Page,
    wiki: &inklkunn_store::WikiRecord,
    view: Option<&str>,
    title: &str,
) -> Response {
    let t = page.t();
    let title = title.trim();
    let back = (!title.is_empty()).then(|| Choice {
        label: t.msg(Key::WhyBack).with("page", title).text(),
        href: WikiLinks::new(t, wiki.slug.as_str(), view).page_href(title),
    });
    notice(
        page,
        wiki,
        StatusCode::NOT_IMPLEMENTED,
        Key::WhyHeading,
        vec![page.get(Key::WhyNotAvailable)],
        back.into_iter().collect(),
        None,
    )
}
