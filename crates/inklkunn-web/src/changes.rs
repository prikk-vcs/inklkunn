//! Recent changes and their feeds (RFC 003 D-7; requirements RD-7, IX-5): `-/changes` as a page and
//! `-/changes.atom` as an Atom feed, over the changes of all the wiki's views, filtered by what the reader may
//! read and by what the reader asked for (a namespace, a view, the wiki service's own changes, one page), and
//! **saying so when the list is filtered**.
//!
//! **Access first.** Every change is checked against [`Action::Read`] on its view and on each page it
//! touched ([`restrict`]); a change none of whose pages the reader may read is left out and counted, and the
//! page says that changes were left out without saying which. A wiki the reader may not read is the generic
//! not-found page, byte for byte, whether or not it exists; its feed too. A feed of a wiki that is not public
//! answers like any other page of it: `Cache-Control: private`, and nothing in a public feed names it.
//!
//! **Times** are when this server first saw each change (the history carries none, see
//! [`inklkunn_core::read`]); the page and the feed say so.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use inklkunn_core::read::{
    ChangeFilter, ChangeKind, ChangeList, ChangeRow, ChangedPage, Person, Tri,
};
use inklkunn_core::{Action, Principal, Resource};
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};
use crate::discover::{Subject, query_escape, resolve};
use crate::feed::{Entry, Feed};
use crate::pages::{Page, PageState, answer};
use crate::services::Services;
use crate::time::utc_minutes;
use crate::url::encode_title;
use crate::wiki::{Shell, blocking, generic_not_found, render_failed, with_retry_after};

/// How many changes a page or a feed lists.
const LIMIT: usize = 50;
/// The longest namespace or title taken from a query.
const QUERY_CHARS: usize = 200;

/// The filters a query asks for.
pub(crate) fn parse_filter(query: &HashMap<String, String>) -> ChangeFilter {
    let text = |name: &str| {
        query
            .get(name)
            .map(|v| v.trim().chars().take(QUERY_CHARS).collect::<String>())
            .filter(|v| !v.is_empty())
    };
    let tri = |name: &str| match query.get(name).map(String::as_str) {
        Some("hide") => Tri::Hide,
        Some("only") => Tri::Only,
        _ => Tri::Any,
    };
    ChangeFilter {
        view: text("view"),
        namespace: text("namespace"),
        page: text("page"),
        automated: tri("automated"),
        minor: tri("minor"),
    }
}

/// The query string that asks for `filter` (empty for none), for the feed's address and the clear link.
fn query_of(filter: &ChangeFilter) -> String {
    let mut parts = Vec::new();
    if let Some(v) = &filter.view {
        parts.push(format!("view={}", query_escape(v)));
    }
    if let Some(v) = &filter.namespace {
        parts.push(format!("namespace={}", query_escape(v)));
    }
    if let Some(v) = &filter.page {
        parts.push(format!("page={}", query_escape(v)));
    }
    match filter.automated {
        Tri::Any => {}
        Tri::Hide => parts.push("automated=hide".to_owned()),
        Tri::Only => parts.push("automated=only".to_owned()),
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// The changes of `rows` that the reader may see, and how many were left out. `can_view` answers for a view's
/// full name, `can_page` for a page's path. A change that touched pages and not one of them is readable is
/// left out; so is a change of a view that is not readable; a change that touched no page is shown (it names
/// nothing the reader may not read). A change whose pages are not known is shown without them.
pub(crate) fn restrict(
    rows: Vec<ChangeRow>,
    can_view: impl Fn(&str) -> bool,
    can_page: impl Fn(&str) -> bool,
) -> (Vec<ChangeRow>, usize) {
    let mut kept = Vec::with_capacity(rows.len());
    let mut hidden = 0;
    for mut row in rows {
        if !can_view(&row.view) {
            hidden += 1;
            continue;
        }
        if let Some(pages) = row.pages.take() {
            let before = pages.len();
            let readable: Vec<ChangedPage> =
                pages.into_iter().filter(|p| can_page(&p.path)).collect();
            if before > 0 && readable.is_empty() {
                hidden += 1;
                continue;
            }
            row.pages = Some(readable);
        }
        kept.push(row);
    }
    (kept, hidden)
}

/// The changes a request lists: the list itself, restricted to what the principal may read.
struct Listed {
    wiki: WikiRecord,
    list: ChangeList,
    rows: Vec<ChangeRow>,
    hidden: usize,
}

fn list_changes(
    services: &Services,
    headers: &HeaderMap,
    wiki: WikiRecord,
    filter: &ChangeFilter,
) -> Result<Listed, inklkunn_core::read::ReadError> {
    let principal: Principal = services.identity.principal(headers);
    let list = services.read.changes(&wiki, filter, LIMIT)?;
    let allowed = |resource: Resource| {
        services
            .authorizer
            .authorize(&principal, Action::Read, &resource)
            .is_allowed()
    };
    let (rows, hidden) = restrict(
        list.rows.clone(),
        |view| {
            allowed(Resource::View {
                wiki: wiki.id.clone(),
                name: view.to_owned(),
            })
        },
        |path| {
            allowed(Resource::Page {
                wiki: wiki.id.clone(),
                path: path.to_owned(),
            })
        },
    );
    Ok(Listed {
        wiki,
        list,
        rows,
        hidden,
    })
}

// ------------------------------------------------------------------------------------ words

fn person_text(t: T, person: &Person) -> String {
    match person {
        Person::Account(name) => name.clone(),
        Person::Anonymous(token) => t
            .msg(Key::ChangesPersonAnonymous)
            .with("token", token)
            .text(),
        Person::Service(label) => t.msg(Key::ChangesPersonService).with("label", label).text(),
    }
}

/// Who made a change, as lines of words.
fn people_lines(t: T, row: &ChangeRow) -> Vec<String> {
    if row.kind == ChangeKind::Foreign {
        return vec![t.get(Key::ChangesForeign)];
    }
    match (&row.proposed_by, &row.accepted_by) {
        (Some(p), Some(a)) if p == a => vec![
            t.msg(Key::ChangesMadeBy)
                .with("person", person_text(t, a))
                .text(),
        ],
        (p, a) => {
            let mut lines = Vec::new();
            if let Some(p) = p {
                lines.push(
                    t.msg(Key::ChangesProposedBy)
                        .with("person", person_text(t, p))
                        .text(),
                );
            }
            if let Some(a) = a {
                lines.push(
                    t.msg(Key::ChangesAcceptedBy)
                        .with("person", person_text(t, a))
                        .text(),
                );
            }
            lines
        }
    }
}

fn view_name(full: &str) -> &str {
    full.strip_prefix("heads/").unwrap_or(full)
}

/// The address of a page in the view a change was listed from (the default view has no `~`).
fn page_href(slug: &str, default_view: &str, view: &str, title: &str) -> String {
    if view == default_view {
        format!("/{slug}/{}", encode_title(title))
    } else {
        format!("/{slug}/~{}/{}", view_name(view), encode_title(title))
    }
}

// ------------------------------------------------------------------------------------ the page

struct PageLink {
    label: String,
    href: Option<String>,
}

struct RowView {
    summary: String,
    people: Vec<String>,
    rollback: Option<String>,
    pages: Vec<PageLink>,
    pages_note: Option<String>,
    view: String,
    iso: String,
    when: String,
}

struct Opt {
    value: String,
    label: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "changes.html")]
struct ChangesTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    seen_note: String,
    action: String,
    namespace_value: String,
    views: Vec<Opt>,
    automated: Vec<Opt>,
    active: Vec<String>,
    clear_href: String,
    rows: Vec<RowView>,
    notes: Vec<String>,
    feed_href: String,
}

fn row_view(t: T, page: &Page, wiki: &WikiRecord, row: &ChangeRow) -> RowView {
    let slug = wiki.slug.as_str();
    let pages: Vec<PageLink> = row
        .pages
        .iter()
        .flatten()
        .map(|p| {
            if p.exists {
                PageLink {
                    label: p.title.clone(),
                    href: Some(page_href(slug, &wiki.default_view, &row.view, &p.title)),
                }
            } else {
                PageLink {
                    label: t.msg(Key::ChangesPageGone).with("title", &p.title).text(),
                    href: None,
                }
            }
        })
        .collect();
    let pages_note = match &row.pages {
        None => Some(page.get(Key::ChangesPagesUnavailable)),
        Some(p) if p.is_empty() => Some(page.get(Key::ChangesNoPages)),
        Some(_) => None,
    };
    RowView {
        summary: if row.summary.is_empty() {
            page.get(Key::ChangesColChange)
        } else {
            row.summary.clone()
        },
        people: people_lines(t, row),
        rollback: row.rollback.then(|| page.get(Key::ChangesRollback)),
        pages,
        pages_note,
        view: view_name(&row.view).to_owned(),
        iso: crate::time::rfc3339(row.first_seen),
        when: utc_minutes(row.first_seen),
    }
}

fn active_filters(t: T, filter: &ChangeFilter) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ns) = &filter.namespace {
        out.push(
            t.msg(Key::ChangesActiveNamespace)
                .with("namespace", ns)
                .text(),
        );
    }
    if let Some(p) = &filter.page {
        out.push(t.msg(Key::ChangesActivePage).with("page", p).text());
    }
    if let Some(v) = &filter.view {
        out.push(
            t.msg(Key::ChangesActiveView)
                .with("view", view_name(v))
                .text(),
        );
    }
    match filter.automated {
        Tri::Any => {}
        Tri::Hide => out.push(t.get(Key::ChangesActiveHide)),
        Tri::Only => out.push(t.get(Key::ChangesActiveOnly)),
    }
    out
}

fn render_changes(page: &Page, listed: &Listed, filter: &ChangeFilter) -> Response {
    let t = page.t();
    let wiki = &listed.wiki;
    let shell = Shell::of(wiki);
    let page = &page.wiki_head(wiki, Subject::Surface("-/changes"));
    let name = wiki.display_name.as_str();
    let mut notes = Vec::new();
    if listed.list.more {
        notes.push(page.get(Key::ChangesMore));
    }
    if listed.list.pages_missing {
        notes.push(page.get(Key::ChangesPagesMissing));
    }
    if listed.hidden > 0 {
        notes.push(page.get(Key::ChangesHidden));
    }
    let selected_view = filter.view.as_deref().map(view_name);
    let mut views = vec![Opt {
        value: String::new(),
        label: page.get(Key::ChangesAllViews),
        selected: selected_view.is_none(),
    }];
    views.extend(listed.list.views.iter().map(|v| Opt {
        value: v.clone(),
        label: v.clone(),
        selected: selected_view == Some(v.as_str()),
    }));
    let automated = [
        ("show", Key::ChangesOptShow, Tri::Any),
        ("hide", Key::ChangesOptHide, Tri::Hide),
        ("only", Key::ChangesOptOnly, Tri::Only),
    ]
    .into_iter()
    .map(|(value, label, tri)| Opt {
        value: value.to_owned(),
        label: page.get(label),
        selected: filter.automated == tri,
    })
    .collect();
    let rendered = ChangesTpl {
        page,
        shell: &shell,
        title: t.msg(Key::TitleChanges).with("wiki", name).product().text(),
        heading: page.get(Key::ChangesHeading),
        intro: t.msg(Key::ChangesIntro).with("wiki", name).text(),
        seen_note: page.get(Key::ChangesSeenNote),
        action: format!("/{}/-/changes", wiki.slug),
        namespace_value: filter.namespace.clone().unwrap_or_default(),
        views,
        automated,
        active: active_filters(t, filter),
        clear_href: format!("/{}/-/changes", wiki.slug),
        rows: listed
            .rows
            .iter()
            .map(|r| row_view(t, page, wiki, r))
            .collect(),
        notes,
        feed_href: format!("/{}/-/changes.atom{}", wiki.slug, query_of(filter)),
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

/// `GET /{slug}/-/changes`.
pub(crate) async fn changes_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let filter = parse_filter(&query);
    let Some((services, wiki)) = resolve(&state, headers.clone(), slug).await else {
        return generic_not_found(&page);
    };
    let for_work = filter.clone();
    let listed = blocking(move || list_changes(&services, &headers, wiki, &for_work)).await;
    match listed {
        Some(Ok(listed)) => render_changes(&page, &listed, &filter),
        Some(Err(_)) => with_retry_after(crate::pages::plain(
            StatusCode::SERVICE_UNAVAILABLE,
            page.t().lang(),
            page.get(Key::UnavailableBody),
        )),
        None => render_failed(&page),
    }
}

// ------------------------------------------------------------------------------------ the feed

fn feed_of(
    t: T,
    state: &PageState,
    services: &Services,
    listed: &Listed,
    filter: &ChangeFilter,
) -> Feed {
    let wiki = &listed.wiki;
    let origins = &state.origins;
    let name = wiki.display_name.as_str();
    let base = format!("/{}", wiki.slug);
    let title = if let Some(p) = &filter.page {
        t.msg(Key::FeedTitlePage)
            .with("page", p)
            .with("wiki", name)
            .text()
    } else if let Some(ns) = &filter.namespace {
        t.msg(Key::FeedTitleNamespace)
            .with("namespace", ns)
            .with("wiki", name)
            .text()
    } else {
        t.msg(Key::FeedTitleWiki).with("wiki", name).text()
    };
    let scope = match (&filter.page, &filter.namespace) {
        (Some(p), _) => format!(":page:{}", encode_title(p)),
        (None, Some(ns)) => format!(":namespace:{}", encode_title(ns)),
        (None, None) => String::new(),
    };
    let entries: Vec<Entry> = listed
        .rows
        .iter()
        .map(|row| {
            let first = row.pages.iter().flatten().find(|p| p.exists);
            let href = origins.app(&match first {
                Some(p) => page_href(wiki.slug.as_str(), &wiki.default_view, &row.view, &p.title),
                None => format!("{base}/-/changes"),
            });
            let related = first.zip(row.parent_block.as_ref()).map(|(p, parent)| {
                origins.app(&format!(
                    "{base}/-/diff/{parent}/{}/{}",
                    row.block_id,
                    encode_title(&p.title)
                ))
            });
            let titles: Vec<&str> = row
                .pages
                .iter()
                .flatten()
                .map(|p| p.title.as_str())
                .collect();
            Entry {
                id: format!("urn:inklkunn:{}:{}", wiki.id, row.block_id),
                title: if row.summary.is_empty() {
                    t.get(Key::ChangesColChange)
                } else {
                    row.summary.clone()
                },
                updated: row.first_seen,
                href,
                related,
                author: row
                    .accepted_by
                    .as_ref()
                    .map_or_else(|| name.to_owned(), |p| person_text(t, p)),
                summary: (!titles.is_empty()).then(|| {
                    t.msg(Key::FeedPages)
                        .with("pages", titles.join(", "))
                        .text()
                }),
            }
        })
        .collect();
    Feed {
        lang: t.lang().tag(),
        id: format!("urn:inklkunn:{}:changes{scope}", wiki.id),
        title,
        updated: listed
            .rows
            .iter()
            .map(|r| r.first_seen)
            .max()
            .unwrap_or_else(|| (services.clock)()),
        self_href: origins.app(&format!("{base}/-/changes.atom{}", query_of(filter))),
        alternate_href: origins.app(&format!("{base}/-/changes{}", query_of(filter))),
        author: name.to_owned(),
        version: env!("CARGO_PKG_VERSION"),
        entries,
    }
}

/// `GET /{slug}/-/changes.atom`.
pub(crate) async fn changes_atom(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let filter = parse_filter(&query);
    let Some((services, wiki)) = resolve(&state, headers.clone(), slug).await else {
        return generic_not_found(&page);
    };
    let (for_work, for_feed) = (filter.clone(), services.clone());
    let listed = blocking(move || list_changes(&services, &headers, wiki, &for_work)).await;
    match listed {
        Some(Ok(listed)) => {
            let body = feed_of(page.t(), &state, &for_feed, &listed, &filter).render();
            let mut res = (StatusCode::OK, body).into_response();
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/atom+xml; charset=utf-8"),
            );
            if listed.wiki.visibility != inklkunn_store::Visibility::Public {
                res.headers_mut().insert(
                    axum::http::HeaderName::from_static("x-robots-tag"),
                    HeaderValue::from_static("noindex, nofollow"),
                );
            }
            res
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
