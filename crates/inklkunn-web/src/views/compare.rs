//! Comparing two views (RFC 005 part 2 D-9; handoff 005 K-T8): **where two views differ, claim by claim and page by
//! page, and nothing about which is right.** The page lists the claims that differ (in wording, status or sources;
//! a claim only one view has is said so) and the pages that differ, each page with a link to how it differs, which
//! is RFC 003's diff between the two views' tips, with its word marks and its bounds.
//!
//! Both views must be views the wiki has (an address that names one it has not is the page for a view that is not
//! there), and the wiki must be one the visitor may read: the handler begins as every handler of a wiki does.
//! **Existence-neutral otherwise**: nothing here says which wikis, views or claims exist beyond what the visitor may
//! read. The lists are bounded by the core, and what is left out is counted in words. The two views are called by
//! their names in the order the address gives them, and are treated alike: no colour, no icon, no order says one is
//! nearer the truth.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::knowledge::Standing;
use inklkunn_core::knowledge::compare::Presence;
use inklkunn_core::read::{
    ComparedClaim, DiffStatus, MAX_DIFF_LINES, PageDifference, Presentation, ReadError,
    ViewComparison, ViewPageOutcome,
};
use inklkunn_core::views::{ViewList, list as list_views, short_name};
use inklkunn_store::WikiRecord;

use super::{address_of, fields, named, paths};
use crate::catalog::{Key, T};
use crate::discover::Subject;
use crate::history::{DiffTpl, LinkView as DiffLink, hunk_view};
use crate::knowledge::marks::distinct;
use crate::knowledge::paths as claim_paths;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::services::Services;
use crate::url::view_ref;
use crate::wiki::{
    Shell, Shown, blocking, generic_not_found, open, present, render_failed, services_of,
};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

// ------------------------------------------------------------------------------------ the page

struct Selection {
    value: String,
    label: String,
    selected: bool,
}

struct ClaimItem {
    href: String,
    label: String,
    facts: Vec<String>,
}

struct PageItem {
    href: String,
    label: String,
    facts: Vec<String>,
}

struct Report {
    same: Option<String>,
    claims_heading: String,
    claims_none: Option<String>,
    claims: Vec<ClaimItem>,
    claims_omitted: Option<String>,
    pages_heading: String,
    pages_none: Option<String>,
    pages: Vec<PageItem>,
    pages_omitted: Option<String>,
}

#[derive(Template)]
#[template(path = "compare.html")]
struct CompareTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    action: String,
    a_field: &'static str,
    b_field: &'static str,
    a_label: String,
    b_label: String,
    a_options: Vec<Selection>,
    b_options: Vec<Selection>,
    go: String,
    pick_two: Option<String>,
    report: Option<Report>,
    back: LinkView,
}

/// What a request for a comparison came to, before any template.
enum Loaded {
    Shown(Shown),
    Chooser {
        wiki: Box<WikiRecord>,
        list: ViewList,
        a: String,
        b: Option<String>,
        pick_two: bool,
    },
    Compared {
        wiki: Box<WikiRecord>,
        list: ViewList,
        a: String,
        b: String,
        comparison: Box<ViewComparison>,
    },
}

/// A view named in the address, as the repository names it; `Err` is the page for a view that is not there.
fn named_view(wiki: &WikiRecord, raw: &str) -> Result<(String, String), Shown> {
    match view_ref(raw) {
        Some(full) => Ok((raw.to_owned(), full)),
        None => Err(Shown::NoSuchView {
            wiki: Box::new(wiki.clone()),
            view: raw.to_owned(),
        }),
    }
}

fn unavailable(
    services: &Services,
    wiki: WikiRecord,
    what: &str,
    e: &dyn std::fmt::Display,
) -> Loaded {
    (services.log)(&format!(
        "{what} of wiki `{}` could not be read now: {e}",
        wiki.slug
    ));
    Loaded::Shown(Shown::Unavailable {
        wiki: Box::new(wiki),
    })
}

fn no_such_view(wiki: WikiRecord, view: &str) -> Loaded {
    Loaded::Shown(Shown::NoSuchView {
        wiki: Box::new(wiki),
        view: view.strip_prefix("heads/").unwrap_or(view).to_owned(),
    })
}

fn load(
    services: &Services,
    headers: &HeaderMap,
    slug: &str,
    params: &HashMap<String, String>,
) -> Loaded {
    let Some(wiki) = open(services, headers, slug) else {
        return Loaded::Shown(Shown::NotFound);
    };
    let list = match list_views(&services.read, &*services.acts, &wiki) {
        Ok(list) => list,
        Err(e) => return unavailable(services, wiki, "the views", &e),
    };
    let default = short_name(&wiki.default_view).to_owned();
    let (a, a_full) = match named_view(
        &wiki,
        params
            .get(fields::A)
            .map_or(default.as_str(), String::as_str),
    ) {
        Ok(found) => found,
        Err(shown) => return Loaded::Shown(shown),
    };
    let Some(raw_b) = params.get(fields::B) else {
        return Loaded::Chooser {
            wiki: Box::new(wiki),
            list,
            a,
            b: None,
            pick_two: false,
        };
    };
    let (b, b_full) = match named_view(&wiki, raw_b) {
        Ok(found) => found,
        Err(shown) => return Loaded::Shown(shown),
    };
    if a == b {
        return Loaded::Chooser {
            wiki: Box::new(wiki),
            list,
            a,
            b: Some(b),
            pick_two: true,
        };
    }
    match services
        .read
        .compare_views(&wiki, Some(&a_full), Some(&b_full))
    {
        Ok(comparison) => Loaded::Compared {
            wiki: Box::new(wiki),
            list,
            a,
            b,
            comparison: Box::new(comparison),
        },
        Err(ReadError::NoSuchView(view)) => no_such_view(wiki, &view),
        Err(e) => unavailable(services, wiki, "a comparison of views", &e),
    }
}

/// `GET /{slug}/-/views/compare`.
pub(crate) async fn compare(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    match blocking(move || load(&services, &headers, &slug, &params)).await {
        None => render_failed(&page),
        Some(Loaded::Shown(shown)) => present(&page, shown),
        Some(Loaded::Chooser {
            wiki,
            list,
            a,
            b,
            pick_two,
        }) => render(&page, &wiki, &list, &a, b.as_deref(), pick_two, None),
        Some(Loaded::Compared {
            wiki,
            list,
            a,
            b,
            comparison,
        }) => {
            let report = report(page.t(), &wiki, &a, &b, &comparison);
            render(&page, &wiki, &list, &a, Some(&b), false, Some(report))
        }
    }
}

/// The two choices of a view, each with the one chosen selected.
fn selections(t: T, wiki: &WikiRecord, list: &ViewList, chosen: Option<&str>) -> Vec<Selection> {
    list.rows
        .iter()
        .map(|row| Selection {
            value: row.name.clone(),
            label: if row.is_default {
                named(t, wiki, &row.name)
            } else {
                row.name.clone()
            },
            selected: chosen == Some(row.name.as_str()),
        })
        .collect()
}

fn render(
    page: &Page,
    wiki: &WikiRecord,
    list: &ViewList,
    a: &str,
    b: Option<&str>,
    pick_two: bool,
    report: Option<Report>,
) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let head = page.wiki_head(wiki, Subject::Wiki);
    let heading = match b.filter(|_| report.is_some()) {
        Some(b) => t
            .msg(Key::ComparePair)
            .with("a", named(t, wiki, a))
            .with("b", named(t, wiki, b))
            .text(),
        None => head.get(Key::CompareHeading),
    };
    let rendered = CompareTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleCompare)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading,
        intro: head.get(Key::CompareIntro),
        action: paths::compare(slug, None, None),
        a_field: fields::A,
        b_field: fields::B,
        a_label: head.get(Key::CompareALabel),
        b_label: head.get(Key::CompareBLabel),
        a_options: selections(t, wiki, list, Some(a)),
        b_options: selections(t, wiki, list, b),
        go: head.get(Key::CompareGo),
        pick_two: pick_two.then(|| head.get(Key::ComparePickTwo)),
        report,
        back: LinkView {
            href: paths::list(slug),
            label: head.get(Key::CompareBack),
        },
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}

// ------------------------------------------------------------------------------------ the report

/// The word for a status, in the shape a comparison gives it.
fn status_word(word: &str) -> Key {
    match Standing::from_word(word) {
        Some(Standing::Accepted) => Key::CompareStatusAccepted,
        Some(Standing::Contested) => Key::CompareStatusContested,
        Some(Standing::Unresolved) => Key::CompareStatusUnresolved,
        Some(Standing::Unknown) | None => Key::CompareStatusUnknown,
    }
}

/// What differs about one claim, in sentences.
pub(crate) fn claim_facts(
    t: T,
    wiki: &WikiRecord,
    a: &str,
    b: &str,
    claim: &ComparedClaim,
) -> Vec<String> {
    let d = &claim.difference;
    let (va, vb) = (named(t, wiki, a), named(t, wiki, b));
    match d.presence {
        Presence::OnlyA => vec![t.msg(Key::CompareOnlyIn).with("view", va).text()],
        Presence::OnlyB => vec![t.msg(Key::CompareOnlyIn).with("view", vb).text()],
        Presence::UnreadableInA => vec![t.msg(Key::CompareUnreadableIn).with("view", va).text()],
        Presence::UnreadableInB => vec![t.msg(Key::CompareUnreadableIn).with("view", vb).text()],
        Presence::Both => {
            let mut facts = Vec::new();
            if d.wording {
                facts.push(t.get(Key::CompareDiffWording));
            }
            if let Some((sa, sb)) = &d.status {
                facts.push(
                    t.msg(Key::CompareDiffStatus)
                        .with("a", t.get(status_word(sa)))
                        .with("va", va)
                        .with("b", t.get(status_word(sb)))
                        .with("vb", vb)
                        .text(),
                );
            }
            if d.sources {
                facts.push(t.get(Key::CompareDiffSources));
            }
            facts
        }
    }
}

fn claim_items(
    t: T,
    wiki: &WikiRecord,
    a: &str,
    b: &str,
    comparison: &ViewComparison,
) -> Vec<ClaimItem> {
    let mut used = HashMap::new();
    comparison
        .claims
        .iter()
        .map(|claim| {
            // The claim's page is read in a view that has the claim: the first, unless only the second does.
            let in_b = matches!(
                claim.difference.presence,
                Presence::OnlyB | Presence::UnreadableInA
            );
            let view = if in_b { b } else { a };
            let label = match &claim.said_in {
                Some(page) => t.msg(Key::CompareClaimSaidIn).with("page", page).text(),
                None => t.get(Key::CompareClaimUnplaced),
            };
            ClaimItem {
                href: claim_paths::claim(
                    wiki.slug.as_str(),
                    &claim.difference.id,
                    address_of(wiki, view),
                ),
                label: distinct(&mut used, label),
                facts: claim_facts(t, wiki, a, b, claim),
            }
        })
        .collect()
}

fn page_item(t: T, wiki: &WikiRecord, a: &str, b: &str, page: &PageDifference) -> PageItem {
    let mut facts = Vec::new();
    match page.status {
        DiffStatus::Added => facts.push(
            t.msg(Key::CompareOnlyIn)
                .with("view", named(t, wiki, b))
                .text(),
        ),
        DiffStatus::Removed => facts.push(
            t.msg(Key::CompareOnlyIn)
                .with("view", named(t, wiki, a))
                .text(),
        ),
        DiffStatus::Modified | DiffStatus::Other | DiffStatus::Unchanged => {
            facts.push(t.get(Key::ComparePageDiffers));
        }
    }
    if page.added > 0 {
        facts.push(t.msg(Key::DiffSummaryAdded).n(page.added).text());
    }
    if page.removed > 0 {
        facts.push(t.msg(Key::DiffSummaryRemoved).n(page.removed).text());
    }
    PageItem {
        href: paths::compare_page(wiki.slug.as_str(), a, b, &page.title),
        label: t.msg(Key::ComparePageLink).with("page", &page.title).text(),
        facts,
    }
}

fn report(t: T, wiki: &WikiRecord, a: &str, b: &str, comparison: &ViewComparison) -> Report {
    let omitted = |key: Key, shown: usize, omitted: usize| {
        (omitted > 0).then(|| {
            t.msg(key)
                .with("shown", shown)
                .with("omitted", omitted)
                .text()
        })
    };
    Report {
        same: comparison.same.then(|| t.get(Key::CompareSame)),
        claims_heading: t.get(Key::CompareClaimsHeading),
        // A list that leaves some out is full, so an empty one has none left out.
        claims_none: comparison
            .claims
            .is_empty()
            .then(|| t.get(Key::CompareClaimsNone)),
        claims: claim_items(t, wiki, a, b, comparison),
        claims_omitted: omitted(
            Key::CompareClaimsOmitted,
            comparison.claims.len(),
            comparison.claims_omitted,
        ),
        pages_heading: t.get(Key::ComparePagesHeading),
        pages_none: comparison
            .pages
            .is_empty()
            .then(|| t.get(Key::ComparePagesNone)),
        pages: comparison
            .pages
            .iter()
            .map(|p| page_item(t, wiki, a, b, p))
            .collect(),
        pages_omitted: omitted(
            Key::ComparePagesOmitted,
            comparison.pages.len(),
            comparison.pages_omitted,
        ),
    }
}

// ------------------------------------------------------------------------------------ one page, between two views

enum PageLoaded {
    Shown(Shown),
    Found {
        wiki: Box<WikiRecord>,
        a: String,
        b: String,
        diff: Box<inklkunn_core::read::ViewPageDiff>,
    },
    Absent(Box<WikiRecord>),
}

fn load_page(
    services: &Services,
    headers: &HeaderMap,
    slug: &str,
    title: &str,
    params: &HashMap<String, String>,
) -> PageLoaded {
    let Some(wiki) = open(services, headers, slug) else {
        return PageLoaded::Shown(Shown::NotFound);
    };
    let (Some(raw_a), Some(raw_b)) = (params.get(fields::A), params.get(fields::B)) else {
        // An address without its two views names nothing.
        return PageLoaded::Shown(Shown::NotFound);
    };
    let (a, a_full) = match named_view(&wiki, raw_a) {
        Ok(found) => found,
        Err(shown) => return PageLoaded::Shown(shown),
    };
    let (b, b_full) = match named_view(&wiki, raw_b) {
        Ok(found) => found,
        Err(shown) => return PageLoaded::Shown(shown),
    };
    match services
        .read
        .compare_page(&wiki, Some(&a_full), Some(&b_full), title)
    {
        Ok(ViewPageOutcome::Found(diff)) => PageLoaded::Found {
            wiki: Box::new(wiki),
            a,
            b,
            diff,
        },
        Ok(ViewPageOutcome::Absent) => PageLoaded::Absent(Box::new(wiki)),
        Err(ReadError::NoSuchView(view)) => PageLoaded::Shown(Shown::NoSuchView {
            wiki: Box::new(wiki),
            view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
        }),
        Err(ReadError::InvalidTitle(_)) => PageLoaded::Shown(Shown::InvalidTitle {
            wiki: Box::new(wiki),
        }),
        Err(e) => {
            (services.log)(&format!(
                "a page's difference between views of wiki `{}` could not be read now: {e}",
                wiki.slug
            ));
            PageLoaded::Shown(Shown::Unavailable {
                wiki: Box::new(wiki),
            })
        }
    }
}

/// `GET /{slug}/-/views/compare/{title}?a=…&b=…`.
pub(crate) async fn compare_page(
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
    let for_work = title.clone();
    match blocking(move || load_page(&services, &headers, &slug, &for_work, &params)).await {
        None => render_failed(&page),
        Some(PageLoaded::Shown(shown)) => present(&page, shown),
        Some(PageLoaded::Absent(wiki)) => crate::wiki::notice(
            &page,
            &wiki,
            StatusCode::NOT_FOUND,
            Key::ComparePagesHeading,
            vec![page.get(Key::ComparePageAbsent)],
            vec![crate::wiki::Choice {
                label: page.get(Key::CompareBack),
                href: paths::list(wiki.slug.as_str()),
            }],
            None,
        ),
        Some(PageLoaded::Found { wiki, a, b, diff }) => render_page(&page, &wiki, &a, &b, &diff),
    }
}

fn render_page(
    page: &Page,
    wiki: &WikiRecord,
    a: &str,
    b: &str,
    diff: &inklkunn_core::read::ViewPageDiff,
) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shape = &diff.shape;
    let shell = Shell::of(wiki);
    let mut summary = Vec::new();
    if shape.added > 0 {
        summary.push(t.msg(Key::DiffSummaryAdded).n(shape.added).text());
    }
    if shape.removed > 0 {
        summary.push(t.msg(Key::DiffSummaryRemoved).n(shape.removed).text());
    }
    if summary.is_empty() {
        summary.push(page.get(Key::DiffSummaryNone));
    }
    let mut notes = Vec::new();
    match shape.status {
        DiffStatus::Added => notes.push(
            t.msg(Key::CompareOnlyIn)
                .with("view", named(t, wiki, b))
                .text(),
        ),
        DiffStatus::Removed => notes.push(
            t.msg(Key::CompareOnlyIn)
                .with("view", named(t, wiki, a))
                .text(),
        ),
        DiffStatus::Unchanged => notes.push(page.get(Key::DiffStatusUnchanged)),
        DiffStatus::Modified | DiffStatus::Other => {}
    }
    if shape.no_lines {
        notes.push(page.get(Key::DiffNoLines));
    }
    if shape.truncated {
        notes.push(
            t.msg(Key::DiffTruncated)
                .with("limit", MAX_DIFF_LINES)
                .text(),
        );
    }
    let marked = shape.hunks.iter().any(|h| match h {
        inklkunn_core::read::HunkView::Lines(h) => {
            h.lines.iter().any(|l| l.segments.iter().any(|s| s.marked))
        }
        inklkunn_core::read::HunkView::Raw(_) => false,
    });
    // The page is read where a view has it: in both unless the diff says it was added or removed.
    let mut more = Vec::new();
    for (view, has) in [
        (a, shape.status != DiffStatus::Added),
        (b, shape.status != DiffStatus::Removed),
    ] {
        if has {
            more.push(DiffLink {
                href: crate::present::WikiLinks::new(t, slug, address_of(wiki, view))
                    .page_href(&diff.title),
                label: t
                    .msg(Key::ComparePageRead)
                    .with("page", &diff.title)
                    .with("view", named(t, wiki, view))
                    .text(),
            });
        }
    }
    let head = page.wiki_head(wiki, Subject::Wiki);
    let rendered = DiffTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleComparePage)
            .with("page", &diff.title)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: t
            .msg(Key::ComparePageHeading)
            .with("title", &diff.title)
            .text(),
        view_note: None,
        range: t
            .msg(Key::ComparePageRange)
            .with("a", named(t, wiki, a))
            .with("b", named(t, wiki, b))
            .html(),
        summary,
        notes,
        refinement: marked.then(|| page.get(Key::DiffRefinement)),
        hunks: shape.hunks.iter().map(|h| hunk_view(t, h)).collect(),
        added_label: page.get(Key::DiffAdded),
        removed_label: page.get(Key::DiffRemoved),
        current: DiffLink {
            href: paths::compare(slug, Some(a), Some(b)),
            label: page.get(Key::ComparePageBack),
        },
        more,
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}
