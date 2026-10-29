//! The search page (RFC 003 D-8; requirements RD-9, NAV-5): titles and aliases, found by any part of them with
//! width variants, diacritics and case folded; results with the text that matched; and, when no page has the
//! title that was searched for, **the pages with similar titles first and then the offer to create it**,
//! disabled: nobody can create a page until proposals arrive (RFC 004), and a link to a form that does not
//! exist would be a dead end.
//!
//! **A snippet is the author's text, escaped.** What matched is a title or an alias, which a person wrote:
//! the segments the read service returns are plain text, each is HTML-escaped here, the matching part is
//! wrapped in `<strong>`, and the whole goes through the same sanitizer as a page body before a template sees
//! it ([`snippet`]). An alias that is a script element is shown as the letters of a script element.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Sanitizer;
use inklkunn_core::read::{SearchKind, SearchOutcome, SnippetSegment};
use inklkunn_markup::escape_html;
use inklkunn_store::WikiRecord;

use crate::catalog::Key;
use crate::discover::{Subject, resolve};
use crate::pages::{Page, PageState, answer};
use crate::url::encode_title;
use crate::wiki::{Body, Shell, blocking, generic_not_found, render_failed, with_retry_after};

/// How many pages are offered as similar titles before the offer to create.
const SIMILAR: usize = 5;

/// The segments of a matching text as HTML that has passed the sanitizer.
pub(crate) fn snippet(sanitizer: &Sanitizer, segments: &[SnippetSegment]) -> Body {
    let mut html = String::new();
    for segment in segments {
        let escaped = escape_html(&segment.text);
        if segment.matched {
            html.push_str("<strong>");
            html.push_str(&escaped);
            html.push_str("</strong>");
        } else {
            html.push_str(&escaped);
        }
    }
    Body(sanitizer.clean(&html))
}

struct HitView {
    href: String,
    title: String,
    label: String,
    snippet: Body,
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    action: String,
    query: String,
    results_heading: Option<String>,
    note: Option<String>,
    hits: Vec<HitView>,
    cut: bool,
    create: Option<CreateOffer>,
}

struct CreateOffer {
    heading: String,
    label: String,
}

fn hit_views(
    page: &Page,
    wiki: &WikiRecord,
    outcome: &SearchOutcome,
    sanitizer: &Sanitizer,
    take: usize,
) -> Vec<HitView> {
    outcome
        .hits
        .iter()
        .take(take)
        .map(|h| HitView {
            href: format!("/{}/{}", wiki.slug, encode_title(&h.entry.title)),
            title: h.entry.title.clone(),
            label: page.get(match h.kind {
                SearchKind::Title => Key::SearchMatchedTitle,
                SearchKind::Alias => Key::SearchMatchedAlias,
            }),
            snippet: snippet(sanitizer, &h.segments),
        })
        .collect()
}

fn render_search(
    page: &Page,
    wiki: &WikiRecord,
    outcome: Option<&SearchOutcome>,
    sanitizer: &Sanitizer,
) -> Response {
    let t = page.t();
    let shell = Shell::of(wiki);
    let page = &page.wiki_head(wiki, Subject::Surface("-/search"));
    let query = outcome.map_or(String::new(), |o| o.query.clone());
    let searched = outcome.is_some_and(|o| !o.query.is_empty());
    let (mut results_heading, mut note, mut hits, mut create) = (None, None, Vec::new(), None);
    let mut cut = false;
    if let Some(o) = outcome.filter(|_| searched) {
        cut = o.truncated;
        if o.names_a_page {
            results_heading = Some(
                t.msg(Key::SearchResultsHeading)
                    .with("query", &o.query)
                    .text(),
            );
            note = Some(page.get(Key::SearchExact));
            hits = hit_views(page, wiki, o, sanitizer, usize::MAX);
        } else {
            if o.hits.is_empty() {
                note = Some(t.msg(Key::SearchNone).with("query", &o.query).text());
            } else {
                results_heading = Some(page.get(Key::SearchSimilarHeading));
                hits = hit_views(page, wiki, o, sanitizer, SIMILAR);
                cut = cut || o.hits.len() > SIMILAR;
            }
            create = Some(CreateOffer {
                heading: page.get(Key::SearchCreateHeading),
                label: t.msg(Key::SearchCreate).with("title", &o.query).text(),
            });
        }
    } else {
        note = Some(page.get(Key::SearchEmpty));
    }
    let rendered = SearchTpl {
        page,
        shell: &shell,
        title: t
            .msg(Key::TitleSearch)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        action: format!("/{}/-/search", wiki.slug),
        query,
        results_heading,
        note,
        hits,
        cut,
        create,
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

/// `GET /{slug}/-/search?q=…`.
pub(crate) async fn search_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return generic_not_found(&page);
    };
    let asked = query
        .ok()
        .and_then(|q| q.0.get("q").cloned())
        .unwrap_or_default();
    let wiki_for_work = wiki.clone();
    let for_work = services.clone();
    let outcome = blocking(move || {
        if asked.trim().is_empty() {
            return Ok(None);
        }
        for_work.read.search(&wiki_for_work, None, &asked).map(Some)
    })
    .await;
    match outcome {
        Some(Ok(outcome)) => {
            render_search(&page, &wiki, outcome.as_ref(), services.read.sanitizer())
        }
        Some(Err(_)) => with_retry_after(crate::pages::plain(
            StatusCode::SERVICE_UNAVAILABLE,
            page.t().lang(),
            page.get(Key::UnavailableBody),
        )),
        None => render_failed(&page),
    }
}

#[cfg(test)]
mod tests;
