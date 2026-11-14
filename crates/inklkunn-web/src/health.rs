//! The health lists and the backlinks (RFC 003 D-9; requirements RD-5, RD-6; NAV-15): what can be improved in a
//! wiki, from the link graph of the tip, **each row with what would resolve it**, framed as opportunities and
//! never as faults. `-/health` names the lists; `-/health/{wanted,orphans,dead-ends,redirects,wants-help,
//! stale,knowledge-files,sources-to-review}` shows one (the last two, RFC 005 part 2 D-5 and D-10, from the knowledge
//! index of the tip); `-/links/<Title>` shows the pages that link to a page, each with the line that makes the
//! link.
//!
//! The action beside a row is said in words. A wanted title's row links to two things, beside each other: the page
//! of who links to it, and *Propose this page*, the form that proposes the page (RFC 004; the default view,
//! which is the view these lists are of; the same form the missing-page notice and every red link lead to). A page
//! is made by a proposal, so no row needs a control of its own.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use std::sync::Arc;

use inklkunn_core::knowledge::Rule;
use inklkunn_core::knowledge::freshness::{LastReview, MAX_RELYING};
use inklkunn_core::knowledge::index::KnowledgeIndex;
use inklkunn_core::read::{
    ApiError, Backlink, BacklinkKind, Cursor, LinkGraph, Lookup, ReadError, RedirectIssueKind,
    SourcesToReviewPage,
};
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};
use crate::discover::{Subject, resolve};
use crate::knowledge::{
    curate as knowledge_curate, paths as knowledge_paths, show::source_name, words,
};
use crate::pages::{Page, PageState, answer};
use crate::time::utc_minutes;
use crate::url::encode_title;
use crate::wiki::{
    Choice, Shell, blocking, generic_not_found, notice, render_failed, with_retry_after,
};

/// How many rows of *Sources to review* a page shows, as the API's pages do by default (K40).
pub(crate) const SOURCES_PAGE: usize = 50;

/// The query of a list: `after` is the cursor of the page, as the API gives it.
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// The query parameter that names where a page of *Sources to review* begins.
pub(crate) const Q_AFTER: &str = "after";

/// The lists, by the word the address ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Wanted,
    Orphans,
    DeadEnds,
    Redirects,
    WantsHelp,
    Stale,
    KnowledgeFiles,
    SourcesToReview,
}

impl Kind {
    const ALL: [Kind; 8] = [
        Kind::Wanted,
        Kind::Orphans,
        Kind::DeadEnds,
        Kind::Redirects,
        Kind::WantsHelp,
        Kind::Stale,
        Kind::KnowledgeFiles,
        Kind::SourcesToReview,
    ];

    const fn slug(self) -> &'static str {
        match self {
            Kind::Wanted => "wanted",
            Kind::Orphans => "orphans",
            Kind::DeadEnds => "dead-ends",
            Kind::Redirects => "redirects",
            Kind::WantsHelp => "wants-help",
            Kind::Stale => "stale",
            Kind::KnowledgeFiles => "knowledge-files",
            Kind::SourcesToReview => "sources-to-review",
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
            Kind::KnowledgeFiles => Key::HealthKnowledgeName,
            Kind::SourcesToReview => Key::HealthSourcesName,
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
            Kind::KnowledgeFiles => Key::HealthKnowledgeDesc,
            Kind::SourcesToReview => Key::HealthSourcesDesc,
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
            Kind::KnowledgeFiles => Key::HealthNoneKnowledge,
            Kind::SourcesToReview => Key::HealthNoneSources,
        }
    }
}

/// What would resolve a row: the pages say it in words, the read API as a code (the words are the client's).
/// One list, so the two cannot disagree about which row wants which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolve {
    /// A wanted title: add a page with it, or change the links.
    WantedAdd,
    /// A page nothing links to: link to it.
    Orphan,
    /// A page that links nowhere: add links from it.
    DeadEnd,
    /// A redirect whose target is not a page.
    Broken,
    /// Redirects that lead round in a circle.
    Loop,
    /// A chain of redirects that is too long.
    Long,
    /// A redirect whose target is itself a redirect.
    Double,
    /// A page marked as needing help.
    Help,
    /// A page that has gone long without a change.
    Review,
    /// A knowledge file that cannot be read.
    KnowledgeFile,
    /// A source a person should look at again.
    SourceCheck,
}

impl Resolve {
    pub(crate) const fn key(self) -> Key {
        match self {
            Resolve::WantedAdd => Key::HealthWantedAction,
            Resolve::Orphan => Key::HealthOrphansAction,
            Resolve::DeadEnd => Key::HealthDeadEndsAction,
            Resolve::Broken => Key::HealthActionBroken,
            Resolve::Loop => Key::HealthActionLoop,
            Resolve::Long => Key::HealthActionLong,
            Resolve::Double => Key::HealthActionDouble,
            Resolve::Help => Key::HealthHelpAction,
            Resolve::Review => Key::HealthStaleAction,
            Resolve::KnowledgeFile => Key::HealthKnowledgeAction,
            Resolve::SourceCheck => Key::HealthSourcesAction,
        }
    }

    /// The stable code of the read API.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Resolve::WantedAdd => "add-page-or-fix-links",
            Resolve::Orphan => "link-to-it",
            Resolve::DeadEnd => "link-from-it",
            Resolve::Broken => "retarget-or-remove",
            Resolve::Loop => "break-the-loop",
            Resolve::Long | Resolve::Double => "point-at-final-page",
            Resolve::Help => "finish-and-unmark",
            Resolve::Review => "review",
            Resolve::KnowledgeFile => "fix-or-remove-file",
            Resolve::SourceCheck => "check-the-source",
        }
    }

    /// What is wrong with a redirect, and what resolves it.
    pub(crate) const fn of_redirect(kind: RedirectIssueKind) -> Resolve {
        match kind {
            RedirectIssueKind::Broken => Resolve::Broken,
            RedirectIssueKind::Loop => Resolve::Loop,
            RedirectIssueKind::TooLong => Resolve::Long,
            RedirectIssueKind::Double => Resolve::Double,
        }
    }

    /// The sentence that says what is wrong with the redirect (only for the redirects).
    const fn problem(self) -> Key {
        match self {
            Resolve::Loop => Key::HealthProblemLoop,
            Resolve::Long => Key::HealthProblemLong,
            Resolve::Double => Key::HealthProblemDouble,
            _ => Key::HealthProblemBroken,
        }
    }
}

/// The catalog sentence for the rule a knowledge file broke.
pub(crate) const fn rule_key(rule: Rule) -> Key {
    match rule {
        Rule::Path => Key::HealthRulePath,
        Rule::NotText => Key::HealthRuleNotText,
        Rule::TooLong => Key::HealthRuleTooLong,
        Rule::Syntax => Key::HealthRuleSyntax,
        Rule::Missing => Key::HealthRuleMissing,
        Rule::Unknown => Key::HealthRuleUnknown,
        Rule::Type => Key::HealthRuleType,
        Rule::Number => Key::HealthRuleNumber,
        Rule::Value => Key::HealthRuleValue,
        Rule::Id => Key::HealthRuleId,
        Rule::Duplicate => Key::HealthRuleDuplicate,
        Rule::Bounds => Key::HealthRuleBounds,
        Rule::Reference => Key::HealthRuleReference,
        Rule::Anchor => Key::HealthRuleAnchor,
        Rule::InUse => Key::HealthRuleInUse,
        Rule::Owned => Key::HealthRuleOwned,
        Rule::Status => Key::HealthRuleStatus,
        Rule::Judged => Key::HealthRuleJudged,
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
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
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

/// A link beside a cell's text: its words and its address, and, on the same line, another (a claim's link, and the
/// way to its curation form).
struct CellLink {
    label: String,
    href: String,
    also: Option<(String, String)>,
}

impl CellLink {
    fn to(label: String, href: String) -> CellLink {
        CellLink {
            label,
            href,
            also: None,
        }
    }
}

struct Cell {
    text: String,
    /// Links beside the text, one under another.
    links: Vec<CellLink>,
    href: Option<String>,
}

impl Cell {
    fn text(text: impl Into<String>) -> Cell {
        Cell {
            text: text.into(),
            links: Vec::new(),
            href: None,
        }
    }

    fn page(text: impl Into<String>, href: String) -> Cell {
        Cell {
            text: text.into(),
            links: Vec::new(),
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
    /// The way on and back through a list that has pages: each its words and its address.
    nav: Vec<(String, String)>,
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
    let action = |resolve: Resolve| Cell::text(page.get(resolve.key()));
    match kind {
        Kind::Wanted => {
            let rows = graph
                .wanted()
                .into_iter()
                .map(|w| {
                    let mut act = action(Resolve::WantedAdd);
                    act.links = vec![
                        CellLink::to(
                            t.msg(Key::HealthSeeLinks).with("title", &w.title).text(),
                            format!("/{}/-/links/{}", wiki.slug, encode_title(&w.title)),
                        ),
                        CellLink::to(
                            t.msg(Key::HealthProposePage).with("title", &w.title).text(),
                            crate::proposals::paths::new_page(wiki.slug.as_str(), Some(&w.title)),
                        ),
                    ];
                    vec![
                        Cell::text(w.title),
                        Cell::text(w.linked_from.to_string()),
                        act,
                    ]
                })
                .collect();
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
                (graph.orphans(), Resolve::Orphan)
            } else {
                (graph.dead_ends(), Resolve::DeadEnd)
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
                    let resolve = Resolve::of_redirect(i.kind.clone());
                    vec![
                        Cell::page(i.entry.title.clone(), page_href(wiki, &i.entry.title)),
                        Cell::text(i.target.clone()),
                        Cell::text(page.get(resolve.problem())),
                        action(resolve),
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
                        Cell::text(page.get(mark)),
                        action(Resolve::Help),
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
        // Their rows come from the knowledge index, not from the link graph: see `knowledge_rows` and `source_rows`.
        Kind::KnowledgeFiles | Kind::SourcesToReview => (Vec::new(), Vec::new()),
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
                                action(Resolve::Review),
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

/// The rows of the list of knowledge files that cannot be read: the file's place in the repository (a storage path,
/// plain text), what is wrong with it, and what would resolve it.
fn knowledge_rows(page: &Page, index: &KnowledgeIndex) -> (Vec<String>, Vec<Vec<Cell>>) {
    let rows = index
        .invalid()
        .iter()
        .map(|row| {
            let rule = Rule::from_code(&row.rule).unwrap_or(Rule::Syntax);
            vec![
                Cell::text(row.path.clone()),
                Cell::text(page.get(rule_key(rule))),
                Cell::text(page.get(Resolve::KnowledgeFile.key())),
            ]
        })
        .collect();
    (
        vec![
            page.get(Key::HealthColFile),
            page.get(Key::HealthColRule),
            page.get(Key::HealthColAction),
        ],
        rows,
    )
}

/// What a page of *Sources to review* is drawn with beside the rows: whether the reader is offered the claims'
/// curation forms (K39), and where the notes and the way on and back go.
struct SourcesDraw<'a> {
    /// The visitor may propose, so each claim named is followed by the way to its curation form.
    curate: bool,
    notes: &'a mut Vec<String>,
    nav: &'a mut Vec<(String, String)>,
}

/// The rows of *Sources to review* (RFC 005 part 2 D-10): the source by the name it has on a claim's page, why it is
/// listed (a person marked it unavailable; no one recorded looking at it; the last look was long ago, or on a day that
/// has not come), the claims that cite it, each linked to its page and said with how the source bears on it, and what
/// would resolve it. Nothing ranks: the rows are in the source ids' order, and the words say what a person has recorded
/// and nothing about the source's worth. Every text is a person's and is printed escaped. A page of the list is
/// [`SOURCES_PAGE`] rows; the way to the next and back to the first is under it (K40).
fn source_rows(
    page: &Page,
    t: T,
    wiki: &WikiRecord,
    list: &SourcesToReviewPage,
    draw: SourcesDraw<'_>,
) -> (Vec<String>, Vec<Vec<Cell>>) {
    let SourcesDraw { curate, notes, nav } = draw;
    // A claim is named by the page it is said on; two claims on one title are told apart by a count, and a claim
    // keeps the name it was given wherever else the page names it.
    let mut named: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut used = std::collections::HashMap::new();
    let rows = list
        .rows
        .iter()
        .map(|row| {
            let (name, details) = source_name(t, &row.source);
            let name = std::iter::once(name)
                .chain(details)
                .collect::<Vec<_>>()
                .join(" ");
            let mut why = Vec::new();
            if row.unavailable {
                why.push(page.get(Key::HealthSourcesUnavailable));
            }
            match &row.review {
                LastReview::Never => why.push(page.get(Key::HealthSourcesNever)),
                LastReview::On { day, overdue: true } => why.push(
                    t.msg(Key::HealthSourcesOverdue)
                        .with("day", day)
                        .with("limit", list.review_after_days)
                        .text(),
                ),
                LastReview::On {
                    day,
                    overdue: false,
                } => {
                    why.push(t.msg(Key::HealthSourcesLast).with("day", day).text());
                }
                LastReview::Ahead { day } => {
                    why.push(t.msg(Key::HealthSourcesFuture).with("day", day).text());
                }
            }
            let links: Vec<CellLink> = row
                .claims
                .iter()
                .map(|c| {
                    let said = named
                        .entry(c.claim.clone())
                        .or_insert_with(|| {
                            let title = list
                                .index
                                .anchors_of(&c.claim)
                                .filter(|a| a.state == inklkunn_core::knowledge::index::PRESENT)
                                .find_map(|a| {
                                    inklkunn_markup::path::path_to_title(a.page_path.as_deref()?)
                                });
                            match title {
                                Some(title) => crate::knowledge::marks::distinct(&mut used, title),
                                None => t
                                    .msg(Key::HealthSourcesClaimNowhere)
                                    .with("id", &c.claim)
                                    .text(),
                            }
                        })
                        .clone();
                    let label = match words::how(&c.relation, &c.bearing) {
                        Some(how) => t
                            .msg(Key::HealthSourcesClaim)
                            .with("where", said)
                            .with("how", t.get(how))
                            .text(),
                        // A phrase for a relation or a bearing this build does not know is not made up.
                        None => said,
                    };
                    CellLink {
                        label,
                        href: knowledge_paths::claim(wiki.slug.as_str(), &c.claim, None),
                        also: curate
                            .then(|| knowledge_curate::link(t, wiki.slug.as_str(), &c.claim, None)),
                    }
                })
                .collect();
            let mut claims = if row.claims_total == 0 {
                Cell::text(page.get(Key::HealthSourcesNoClaims))
            } else {
                let mut text = t
                    .msg(Key::HealthSourcesClaims)
                    .n(u64::try_from(row.claims_total).unwrap_or(u64::MAX))
                    .text();
                if row.claims_total > row.claims.len() {
                    text.push(' ');
                    text.push_str(
                        &t.msg(Key::HealthSourcesCut)
                            .with("limit", MAX_RELYING)
                            .text(),
                    );
                }
                Cell::text(text)
            };
            claims.links = links;
            vec![
                Cell::text(name),
                Cell::text(why.join(" ")),
                claims,
                Cell::text(page.get(Resolve::SourceCheck.key())),
            ]
        })
        .collect();
    notes.push(
        t.msg(Key::HealthSourcesSetting)
            .with("days", list.review_after_days)
            .text(),
    );
    if list.offset > 0 || list.next.is_some() {
        notes.push(
            t.msg(Key::HealthSourcesShowing)
                .with("from", list.offset.saturating_add(1))
                .with("to", list.offset.saturating_add(list.rows.len()))
                .with("total", list.total)
                .text(),
        );
    }
    if list.offset > 0 {
        nav.push((
            page.get(Key::HealthSourcesFirst),
            format!("/{}/-/health/{}", wiki.slug, Kind::SourcesToReview.slug()),
        ));
    }
    if let Some(next) = &list.next {
        nav.push((
            page.get(Key::HealthSourcesMore),
            format!(
                "/{}/-/health/{}?{Q_AFTER}={}",
                wiki.slug,
                Kind::SourcesToReview.slug(),
                next.as_str()
            ),
        ));
    }
    (
        vec![
            page.get(Key::HealthColSource),
            page.get(Key::HealthColWhy),
            page.get(Key::HealthColClaims),
            page.get(Key::HealthColAction),
        ],
        rows,
    )
}

/// What the page of one list is made from.
enum Built {
    /// The link graph (and, for the stale list, the changes).
    Links(Arc<LinkGraph>, Option<inklkunn_core::read::StaleList>),
    /// The knowledge index.
    Knowledge(Arc<KnowledgeIndex>),
    /// A page of the sources that want a look, read from the list kept with the knowledge index at the clock's day,
    /// and whether the visitor is offered the claims' curation forms.
    Sources(Box<SourcesToReviewPage>, bool),
}

/// `GET /{slug}/-/health/{list}`.
pub(crate) async fn health_list(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let Ok(Path((slug, list))) = path else {
        return generic_not_found(&page);
    };
    let Some(kind) = Kind::from_slug(&list) else {
        return generic_not_found(&page);
    };
    // The cursor of the page asked for; one that is not one this service gave is no place to continue from, and the
    // list begins at its first page (the note under it says which rows it shows).
    let after = params
        .ok()
        .and_then(|Query(p)| p.get(Q_AFTER).and_then(|a| Cursor::parse(a)));
    let asked_by = headers.clone();
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return generic_not_found(&page);
    };
    let for_work = wiki.clone();
    let built = blocking(move || -> Result<_, ApiError> {
        if kind == Kind::KnowledgeFiles {
            return Ok(Built::Knowledge(
                services.read.knowledge_index(&for_work, None)?,
            ));
        }
        if kind == Kind::SourcesToReview {
            let asked = |after: Option<&Cursor>| {
                services
                    .read
                    .sources_to_review_page(&for_work, None, after, SOURCES_PAGE)
            };
            let list = match asked(after.as_ref()) {
                Err(ApiError::InvalidCursor) => asked(None)?,
                other => other?,
            };
            let curate = knowledge_curate::may_propose(
                &services,
                &services.identity.principal(&asked_by),
                &for_work,
            );
            return Ok(Built::Sources(Box::new(list), curate));
        }
        let graph = services.read.link_graph(&for_work, None)?;
        let stale = if kind == Kind::Stale {
            Some(services.read.stale_pages(&for_work)?)
        } else {
            None
        };
        Ok(Built::Links(graph, stale))
    })
    .await;
    let built = match built {
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
    let mut nav = Vec::new();
    let (headers, rows) = match &built {
        Built::Links(graph, stale) => {
            rows_of(&page, t, &wiki, kind, graph, stale.as_ref(), &mut notes)
        }
        Built::Knowledge(index) => knowledge_rows(&page, index),
        Built::Sources(list, curate) => source_rows(
            &page,
            t,
            &wiki,
            list,
            SourcesDraw {
                curate: *curate,
                notes: &mut notes,
                nav: &mut nav,
            },
        ),
    };
    let name = wiki.display_name.as_str();
    let rendered = ListTpl {
        page: &page,
        shell: &shell,
        title: t
            .msg(Key::TitlePage)
            .with("page", page.get(kind.name()))
            .with("wiki", name)
            .product()
            .text(),
        heading: page.get(kind.name()),
        description: page.get(kind.description()),
        caption: page.get(kind.name()),
        headers,
        rows,
        none: page.get(kind.none()),
        notes,
        nav,
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
        None,
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
    path: Result<Path<(String, String)>, PathRejection>,
) -> Response {
    let Ok(Path((slug, title))) = path else {
        return generic_not_found(&page);
    };
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
