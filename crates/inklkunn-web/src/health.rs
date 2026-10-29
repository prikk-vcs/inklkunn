//! The health lists and the backlinks (RFC 003 D-9; requirements RD-5, RD-6; NAV-15): what can be improved in a
//! wiki, from the link graph of the tip, **each row with what would resolve it**, framed as opportunities and
//! never as faults. `-/health` names the six lists; `-/health/{wanted,orphans,dead-ends,redirects,wants-help,
//! stale}` shows one; `-/links/<Title>` shows the pages that link to a page, each with the line that makes the
//! link.
//!
//! Nobody can create a page until proposals arrive (RFC 004), so the action beside a row is said in words and
//! the one thing that can be done now, looking at who links to a wanted title, is a link; the disabled
//! affordance is explained once, under the list, with the catalog's sentence about proposals.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::read::{
    Backlink, BacklinkKind, LinkGraph, Lookup, ReadError, RedirectIssueKind,
};
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};
use crate::discover::{Subject, resolve};
use crate::pages::{Page, PageState, answer};
use crate::time::utc_minutes;
use crate::url::encode_title;
use crate::wiki::{
    Choice, Shell, blocking, generic_not_found, notice, render_failed, with_retry_after,
};

/// The six lists, by the word the address ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Wanted,
    Orphans,
    DeadEnds,
    Redirects,
    WantsHelp,
    Stale,
}

impl Kind {
    const ALL: [Kind; 6] = [
        Kind::Wanted,
        Kind::Orphans,
        Kind::DeadEnds,
        Kind::Redirects,
        Kind::WantsHelp,
        Kind::Stale,
    ];

    const fn slug(self) -> &'static str {
        match self {
            Kind::Wanted => "wanted",
            Kind::Orphans => "orphans",
            Kind::DeadEnds => "dead-ends",
            Kind::Redirects => "redirects",
            Kind::WantsHelp => "wants-help",
            Kind::Stale => "stale",
        }
    }

    fn from_slug(slug: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.slug() == slug)
    }

    const fn name(self) -> Key {
        match self {
            Kind::Wanted => Key::HealthWantedName,
            Kind::Orphans => Key::HealthOrphansName,
            Kind::DeadEnds => Key::HealthDeadEndsName,
            Kind::Redirects => Key::HealthRedirectsName,
            Kind::WantsHelp => Key::HealthHelpName,
            Kind::Stale => Key::HealthStaleName,
        }
    }

    const fn description(self) -> Key {
        match self {
            Kind::Wanted => Key::HealthWantedDesc,
            Kind::Orphans => Key::HealthOrphansDesc,
            Kind::DeadEnds => Key::HealthDeadEndsDesc,
            Kind::Redirects => Key::HealthRedirectsDesc,
            Kind::WantsHelp => Key::HealthHelpDesc,
            Kind::Stale => Key::HealthStaleDesc,
        }
    }

    const fn none(self) -> Key {
        match self {
            Kind::Wanted => Key::HealthNoneWanted,
            Kind::Orphans => Key::HealthNoneOrphans,
            Kind::DeadEnds => Key::HealthNoneDeadEnds,
            Kind::Redirects => Key::HealthNoneRedirects,
            Kind::WantsHelp => Key::HealthNoneHelp,
            Kind::Stale => Key::HealthNoneStale,
        }
    }
}

// ------------------------------------------------------------------------------------ the index page

struct ListLink {
    href: String,
    name: String,
    description: String,
}

#[derive(Template)]
#[template(path = "health_index.html")]
struct IndexTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    lists: Vec<ListLink>,
}

/// `GET /{slug}/-/health`.
pub(crate) async fn health_index(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    let Some((_, wiki)) = resolve(&state, headers, slug).await else {
        return generic_not_found(&page);
    };
    let t = page.t();
    let shell = Shell::of(&wiki);
    let page = page.wiki_head(&wiki, Subject::Surface("-/health"));
    let name = wiki.display_name.as_str();
    let rendered = IndexTpl {
        page: &page,
        shell: &shell,
        title: t.msg(Key::TitleHealth).with("wiki", name).product().text(),
        heading: t.msg(Key::HealthHeading).with("wiki", name).text(),
        lists: Kind::ALL
            .into_iter()
            .map(|k| ListLink {
                href: format!("/{}/-/health/{}", wiki.slug, k.slug()),
                name: page.get(k.name()),
                description: page.get(k.description()),
            })
            .collect(),
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}

// ------------------------------------------------------------------------------------ one list

struct Cell {
    text: String,
    link: Option<(String, String)>,
    href: Option<String>,
}

impl Cell {
    fn text(text: impl Into<String>) -> Cell {
        Cell {
            text: text.into(),
            link: None,
            href: None,
        }
    }

    fn page(text: impl Into<String>, href: String) -> Cell {
        Cell {
            text: text.into(),
            link: None,
            href: Some(href),
        }
    }
}

#[derive(Template)]
#[template(path = "health_list.html")]
struct ListTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    description: String,
    caption: String,
    headers: Vec<String>,
    rows: Vec<Vec<Cell>>,
    none: String,
    notes: Vec<String>,
    back_href: String,
    back_text: String,
}

fn page_href(wiki: &WikiRecord, title: &str) -> String {
    format!("/{}/{}", wiki.slug, encode_title(title))
}

/// The rows of one list, from the graph (and, for the stale list, the changes).
fn rows_of(
    page: &Page,
    t: T,
    wiki: &WikiRecord,
    kind: Kind,
    graph: &LinkGraph,
    stale: Option<&inklkunn_core::read::StaleList>,
    notes: &mut Vec<String>,
) -> (Vec<String>, Vec<Vec<Cell>>) {
    let action = |key| Cell::text(page.get(key));
    match kind {
        Kind::Wanted => {
            let rows = graph
                .wanted()
                .into_iter()
                .map(|w| {
                    let mut act = action(Key::HealthWantedAction);
                    act.link = Some((
                        t.msg(Key::HealthSeeLinks).with("title", &w.title).text(),
                        format!("/{}/-/links/{}", wiki.slug, encode_title(&w.title)),
                    ));
                    vec![
                        Cell::text(w.title),
                        Cell::text(w.linked_from.to_string()),
                        act,
                    ]
                })
                .collect();
            notes.push(page.get(Key::PageProposeNote));
            (
                vec![
                    page.get(Key::HealthColWanted),
                    page.get(Key::HealthColLinked),
                    page.get(Key::HealthColAction),
                ],
                rows,
            )
        }
        Kind::Orphans | Kind::DeadEnds => {
            let (pages, key) = if kind == Kind::Orphans {
                (graph.orphans(), Key::HealthOrphansAction)
            } else {
                (graph.dead_ends(), Key::HealthDeadEndsAction)
            };
            let rows = pages
                .into_iter()
                .map(|p| {
                    vec![
                        Cell::page(p.entry.title.clone(), page_href(wiki, &p.entry.title)),
                        action(key),
                    ]
                })
                .collect();
            (
                vec![page.get(Key::HealthColPage), page.get(Key::HealthColAction)],
                rows,
            )
        }
        Kind::Redirects => {
            let rows = graph
                .redirect_issues()
                .iter()
                .map(|i| {
                    let (problem, act) = match i.kind {
                        RedirectIssueKind::Broken => {
                            (Key::HealthProblemBroken, Key::HealthActionBroken)
                        }
                        RedirectIssueKind::Loop => (Key::HealthProblemLoop, Key::HealthActionLoop),
                        RedirectIssueKind::TooLong => {
                            (Key::HealthProblemLong, Key::HealthActionLong)
                        }
                        RedirectIssueKind::Double => {
                            (Key::HealthProblemDouble, Key::HealthActionDouble)
                        }
                    };
                    vec![
                        Cell::page(i.entry.title.clone(), page_href(wiki, &i.entry.title)),
                        Cell::text(i.target.clone()),
                        action(problem),
                        action(act),
                    ]
                })
                .collect();
            (
                vec![
                    page.get(Key::HealthColPage),
                    page.get(Key::HealthColTarget),
                    page.get(Key::HealthColProblem),
                    page.get(Key::HealthColAction),
                ],
                rows,
            )
        }
        Kind::WantsHelp => {
            let rows = graph
                .wants_help()
                .into_iter()
                .map(|p| {
                    let mark = if p.status == "wip" {
                        Key::HealthMarkWip
                    } else if p.status == "draft" {
                        Key::HealthMarkDraft
                    } else {
                        Key::HealthMarkStub
                    };
                    vec![
                        Cell::page(p.entry.title.clone(), page_href(wiki, &p.entry.title)),
                        action(mark),
                        action(Key::HealthHelpAction),
                    ]
                })
                .collect();
            (
                vec![
                    page.get(Key::HealthColPage),
                    page.get(Key::HealthColMark),
                    page.get(Key::HealthColAction),
                ],
                rows,
            )
        }
        Kind::Stale => {
            let rows = stale
                .map(|s| {
                    s.pages
                        .iter()
                        .map(|p| {
                            let last = match p.last_touched {
                                Some(at) => t
                                    .msg(Key::HealthStaleSeen)
                                    .with("when", utc_minutes(at))
                                    .text(),
                                None => page.get(Key::HealthStaleNever),
                            };
                            vec![
                                Cell::page(p.entry.title.clone(), page_href(wiki, &p.entry.title)),
                                Cell::text(last),
                                action(Key::HealthStaleAction),
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(s) = stale {
                notes.push(
                    t.msg(Key::HealthStaleWindow)
                        .with("limit", inklkunn_core::read::SCAN_WINDOW)
                        .text(),
                );
                if !s.complete {
                    notes.push(page.get(Key::ChangesMore));
                }
            }
            (
                vec![
                    page.get(Key::HealthColPage),
                    page.get(Key::HealthColChanged),
                    page.get(Key::HealthColAction),
                ],
                rows,
            )
        }
    }
}

/// `GET /{slug}/-/health/{list}`.
pub(crate) async fn health_list(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path((slug, list)): Path<(String, String)>,
) -> Response {
    let Some(kind) = Kind::from_slug(&list) else {
        return generic_not_found(&page);
    };
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return generic_not_found(&page);
    };
    let for_work = wiki.clone();
    let built = blocking(move || -> Result<_, ReadError> {
        let graph = services.read.link_graph(&for_work, None)?;
        let stale = if kind == Kind::Stale {
            Some(services.read.stale_pages(&for_work)?)
        } else {
            None
        };
        Ok((graph, stale))
    })
    .await;
    let (graph, stale) = match built {
        Some(Ok(built)) => built,
        Some(Err(_)) => return unavailable(&page, &wiki),
        None => return render_failed(&page),
    };
    let t = page.t();
    let shell = Shell::of(&wiki);
    let page = page.wiki_head(
        &wiki,
        Subject::Surface(&format!("-/health/{}", kind.slug())),
    );
    let mut notes = Vec::new();
    let (headers, rows) = rows_of(&page, t, &wiki, kind, &graph, stale.as_ref(), &mut notes);
    let name = wiki.display_name.as_str();
    let rendered = ListTpl {
        page: &page,
        shell: &shell,
        title: t.msg(Key::TitleHealth).with("wiki", name).product().text(),
        heading: page.get(kind.name()),
        description: page.get(kind.description()),
        caption: page.get(kind.name()),
        headers,
        rows,
        none: page.get(kind.none()),
        notes,
        back_href: format!("/{}/-/health", wiki.slug),
        back_text: page.get(Key::HealthBack),
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}

fn unavailable(page: &Page, wiki: &WikiRecord) -> Response {
    with_retry_after(notice(
        page,
        wiki,
        StatusCode::SERVICE_UNAVAILABLE,
        Key::UnavailableHeading,
        vec![page.get(Key::UnavailableBody)],
        Vec::<Choice>::new(),
        false,
    ))
}

// ------------------------------------------------------------------------------------ backlinks

struct BacklinkView {
    href: String,
    title: String,
    kind: Option<String>,
    context: String,
}

#[derive(Template)]
#[template(path = "links.html")]
struct LinksTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    note: Option<String>,
    go_to: Option<(String, String)>,
    links: Vec<BacklinkView>,
    none: String,
    cut: Option<String>,
}

fn backlink_views(page: &Page, wiki: &WikiRecord, from: &[Backlink]) -> Vec<BacklinkView> {
    from.iter()
        .map(|b| BacklinkView {
            href: page_href(wiki, &b.from_title),
            title: b.from_title.clone(),
            kind: match b.kind {
                BacklinkKind::Link => None,
                BacklinkKind::Include => Some(page.get(Key::LinksKindInclude)),
                BacklinkKind::Redirect => Some(page.get(Key::LinksKindRedirect)),
            },
            context: b.context.clone(),
        })
        .collect()
}

/// `GET /{slug}/-/links/{*title}`: the pages that link to the page the title leads to, or, for a title that
/// leads to no page, the pages that link to it anyway (the wanted title's own list).
pub(crate) async fn links_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path((slug, title)): Path<(String, String)>,
) -> Response {
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return generic_not_found(&page);
    };
    let for_work = wiki.clone();
    let asked = title.clone();
    let built = blocking(move || -> Result<_, ReadError> {
        let lookup = services.read.lookup(&for_work, None, &asked)?;
        let graph = services.read.link_graph(&for_work, None)?;
        Ok((lookup, graph))
    })
    .await;
    let (lookup, graph) = match built {
        Some(Ok(built)) => built,
        Some(Err(ReadError::InvalidTitle(_))) => return generic_not_found(&page),
        Some(Err(_)) => return unavailable(&page, &wiki),
        None => return render_failed(&page),
    };
    let t = page.t();
    // The page, if the title leads to one; the title as asked, if it leads to none.
    let (shown_title, from, exists) = match &lookup {
        Lookup::Found(f) => (
            f.entry.title.clone(),
            graph.backlinks(&f.entry.path).to_vec(),
            true,
        ),
        Lookup::BrokenRedirect { from, .. } => (
            from.title.clone(),
            graph.backlinks(&from.path).to_vec(),
            true,
        ),
        Lookup::Ambiguous { .. } | Lookup::Missing => {
            let wanted = graph.wanted_backlinks(&title).to_vec();
            if wanted.is_empty() {
                return crate::wiki::missing_page(&page, &wiki, &title);
            }
            (title.trim().to_owned(), wanted, false)
        }
    };
    let shell = Shell::of(&wiki);
    let page = page.wiki_head(
        &wiki,
        Subject::Surface(&format!("-/links/{}", encode_title(&shown_title))),
    );
    let name = wiki.display_name.as_str();
    let cut = (from.len() >= 500).then(|| t.msg(Key::LinksCut).with("limit", from.len()).text());
    let rendered = LinksTpl {
        page: &page,
        shell: &shell,
        title: t
            .msg(Key::TitleLinks)
            .with("page", &shown_title)
            .with("wiki", name)
            .product()
            .text(),
        heading: t.msg(Key::LinksHeading).with("page", &shown_title).text(),
        note: (!exists).then(|| page.get(Key::LinksMissing)),
        go_to: exists.then(|| {
            (
                t.msg(Key::LinksGoTo).with("page", &shown_title).text(),
                page_href(&wiki, &shown_title),
            )
        }),
        links: backlink_views(&page, &wiki, &from),
        none: page.get(Key::LinksNone),
        cut,
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}

#[cfg(test)]
mod tests;
