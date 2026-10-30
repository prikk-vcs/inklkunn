//! The pages about a page's past (RFC 003 D-4; handoff 003 T5): its history, one change, what changed between
//! two changes, and a page as it was after a change (a permalink).
//!
//! All four are under `/{slug}/-/…`, so no title can be shadowed (RFC 003 D-1), and all four begin as every
//! handler of a wiki does ([`open`]): a wiki that does not exist and a wiki the principal may not read are one
//! generic not-found page. A view other than the default is named by `?view=`.
//!
//! **What these pages say about people is what a change's message says** (RFC 002 D-5), and the words are the
//! glossary's: a change page says *Signed into the history* and, for a change that came from a proposal,
//! *signed by the wiki service on behalf of …, accepted by …*; it never says that a person signed. A change
//! whose message is not in the service's form is said to name nobody. There is no time on any of them: prikk's
//! log carries none.
//!
//! **A diff is the page's own and nothing else** (the read service issues one `diff`, always with the page's
//! path). Its lines are prikk's; the marked words are inklkunn's, and the page says so. Every line of the
//! difference is text from the repository: it is printed by the template, which escapes it, and never
//! assembled into markup here.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::read::{
    AtOutcome, ChangeDetail, ChangeInfo, ChangeOutcome, Did, DiffOutcome, DiffStatus,
    HistoryChangeKind, HistoryOutcome, HistoryRow, HunkView, LineKind, MAX_DIFF_LINES, PageDiff,
    PageEvent, PageHistory, Presentation, Provenance, ReadError, Subject, Who,
};
use inklkunn_markup::path::path_to_title;
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, Rich, T};
use crate::pages::{Page, PageState, answer};
use crate::present::WikiLinks;
use crate::services::Services;
use crate::url::{at_path, change_path, diff_path, history_path, view_ref};
use crate::wiki::{
    Banner, Choice, Shell, Shown, Variant, blocking, found_with, generic_not_found, notice, open,
    present, render_failed, services_of,
};

// ------------------------------------------------------------------------------------ the start

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// Why a request ends before its page: a page the wiki already has a word for, or no such change.
enum Early {
    Shown(Shown),
    NoSuchChange(Box<WikiRecord>),
}

/// The wiki and the view an address names, if the principal may read the wiki.
struct Start {
    wiki: Box<WikiRecord>,
    /// The view as the address spells it (`draft`), `None` for the default view.
    view: Option<String>,
    /// The view as the repository does (`heads/draft`).
    full: Option<String>,
    /// Where in a history the address asks to go on from.
    after: Option<u64>,
}

fn start(
    services: &Services,
    headers: &HeaderMap,
    slug: &str,
    params: &HashMap<String, String>,
) -> Result<Start, Early> {
    let Some(wiki) = open(services, headers, slug) else {
        return Err(Early::Shown(Shown::NotFound));
    };
    let wiki = Box::new(wiki);
    let (view, full) = match params.get("view") {
        None => (None, None),
        Some(name) => match view_ref(name) {
            Some(full) => (Some(name.clone()), Some(full)),
            None => {
                return Err(Early::Shown(Shown::NoSuchView {
                    wiki,
                    view: name.clone(),
                }));
            }
        },
    };
    let after = params.get("after").and_then(|a| a.parse::<u64>().ok());
    Ok(Start {
        wiki,
        view,
        full,
        after,
    })
}

/// What went wrong in a read, as the page that says so. The cause goes to the operator's log.
fn failed(services: &Services, wiki: Box<WikiRecord>, what: &str, e: ReadError) -> Early {
    Early::Shown(match e {
        ReadError::InvalidTitle(_) => Shown::InvalidTitle { wiki },
        ReadError::NoSuchView(view) => Shown::NoSuchView {
            wiki,
            view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
        },
        ReadError::PageUnreadable(problem) => Shown::Unreadable { wiki, problem },
        ReadError::CouldNotBeReadNow(cause) => {
            (services.log)(&format!(
                "{what} of wiki `{}` could not be read now: {cause}",
                wiki.slug
            ));
            Shown::Unavailable { wiki }
        }
    })
}

/// The pages that match a title and none is nearer, as the choices the page view offers.
fn ambiguous(
    t: T,
    wiki: Box<WikiRecord>,
    view: Option<&str>,
    title: &str,
    candidates: Vec<inklkunn_store::IndexEntry>,
) -> Early {
    let links = WikiLinks::new(t, wiki.slug.as_str(), view);
    Early::Shown(Shown::Ambiguous {
        candidates: candidates
            .iter()
            .map(|c| (c.title.clone(), links.page_href(&c.title)))
            .collect(),
        wiki,
        title: title.to_owned(),
    })
}

fn no_such_change(page: &Page, wiki: &WikiRecord) -> Response {
    notice(
        page,
        wiki,
        StatusCode::NOT_FOUND,
        Key::ChangeNoneHeading,
        vec![page.get(Key::ChangeNoneBody)],
        Vec::<Choice>::new(),
        None,
    )
}

/// What a handler's work came to.
fn settle<V>(
    page: &Page,
    done: Option<Result<V, Early>>,
    show: impl FnOnce(V) -> Response,
) -> Response {
    match done {
        None => render_failed(page),
        Some(Err(Early::Shown(shown))) => present(page, shown),
        Some(Err(Early::NoSuchChange(wiki))) => no_such_change(page, &wiki),
        Some(Ok(value)) => show(value),
    }
}

/// A title in a wiki's own words: `view=` is carried so that every link stays in the view.
fn view_note(t: T, view: Option<&str>) -> Option<String> {
    view.map(|v| t.msg(Key::WikiViewInUse).with("view", v).text())
}

// ------------------------------------------------------------------------------------ small view models

struct LinkView {
    href: String,
    label: String,
}

fn who_text(t: T, who: &Who) -> String {
    match who {
        Who::Person { name } => name.clone(),
        Who::Anonymous { token } => t.msg(Key::PeopleAnonymous).with("token", token).text(),
        Who::Service => t.get(Key::PeopleService),
    }
}

/// The sentences about the people of a change, from what its message says and no more. (The read API gives
/// these very sentences beside the structured fields.)
pub(crate) fn people(t: T, provenance: &Provenance) -> Vec<String> {
    match provenance {
        Provenance::Recorded {
            kind: HistoryChangeKind::Proposal | HistoryChangeKind::Rollback,
            proposed_by,
            accepted_by,
        } => vec![
            t.msg(Key::PeopleProposedBy)
                .with("name", who_text(t, proposed_by))
                .text(),
            t.msg(Key::PeopleAcceptedBy)
                .with("name", who_text(t, accepted_by))
                .text(),
        ],
        Provenance::Recorded {
            proposed_by: made_by,
            ..
        } => vec![
            t.msg(Key::PeopleMadeBy)
                .with("name", who_text(t, made_by))
                .text(),
        ],
        Provenance::NotRecorded => vec![t.get(Key::PeopleUnrecorded)],
    }
}

/// What a change is marked as, beyond what its people say. One list for the pages (which say each in words) and
/// the read API (which gives each as a code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flag {
    /// It undoes an earlier change.
    Undo,
    /// It is an import.
    Import,
    /// The wiki service made it itself.
    Service,
    /// It is the first change of the view.
    First,
}

impl Flag {
    /// The marks of a change, in the order the pages say them.
    pub(crate) fn of(info: &ChangeInfo) -> Vec<Flag> {
        let mut out = Vec::new();
        let kind = match &info.provenance {
            Provenance::Recorded { kind, .. } => Some(*kind),
            Provenance::NotRecorded => None,
        };
        if info.undoes || kind == Some(HistoryChangeKind::Rollback) {
            out.push(Flag::Undo);
        }
        match kind {
            Some(HistoryChangeKind::Import) => out.push(Flag::Import),
            Some(HistoryChangeKind::System) => out.push(Flag::Service),
            _ => {}
        }
        if info.first {
            out.push(Flag::First);
        }
        out
    }

    /// The stable code the API gives.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Flag::Undo => "undo",
            Flag::Import => "import",
            Flag::Service => "service",
            Flag::First => "first",
        }
    }

    const fn key(self) -> Key {
        match self {
            Flag::Undo => Key::FlagUndo,
            Flag::Import => Key::FlagImport,
            Flag::Service => Key::FlagService,
            Flag::First => Key::FlagFirst,
        }
    }
}

fn flags(t: T, info: &ChangeInfo) -> Vec<String> {
    Flag::of(info).into_iter().map(|f| t.get(f.key())).collect()
}

fn summary_of(info: &ChangeInfo) -> Option<String> {
    (!info.summary.is_empty()).then(|| info.summary.clone())
}

fn change_label(t: T, number: u64) -> String {
    t.msg(Key::HistoryChangeLabel).with("number", number).text()
}

// ------------------------------------------------------------------------------------ a page's history

struct RowView {
    href: String,
    label: String,
    summary: Option<String>,
    facts: Vec<String>,
    diff: Option<LinkView>,
    version: Option<LinkView>,
}

#[derive(Template)]
#[template(path = "history.html")]
struct HistoryTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    empty: String,
    rows: Vec<RowView>,
    older: Option<LinkView>,
    beyond: Option<String>,
    current: LinkView,
}

fn event_text(t: T, event: &PageEvent) -> String {
    match event {
        PageEvent::Created => t.get(Key::EventCreated),
        PageEvent::Edited => t.get(Key::EventEdited),
        PageEvent::Deleted => t.get(Key::EventDeleted),
        PageEvent::MovedFrom { title } => t.msg(Key::EventMovedFrom).with("title", title).text(),
        PageEvent::MovedTo { title } => t.msg(Key::EventMovedTo).with("title", title).text(),
        PageEvent::NotInspected => t.get(Key::EventNotInspected),
    }
}

fn row_view(
    t: T,
    slug: &str,
    view: Option<&str>,
    fallback_title: &str,
    row: &HistoryRow,
) -> RowView {
    // The title the page had after this change: before a move it is the old title.
    let title = path_to_title(&row.path).unwrap_or_else(|| fallback_title.to_owned());
    let info = &row.change;
    let number = info.number;
    let mut facts: Vec<String> = row.events.iter().map(|e| event_text(t, e)).collect();
    facts.extend(people(t, &info.provenance));
    facts.extend(flags(t, info));
    let edited = row.events.contains(&PageEvent::Edited);
    let gone = row.events.last() == Some(&PageEvent::Deleted)
        || row.events.last() == Some(&PageEvent::NotInspected);
    RowView {
        href: change_path(slug, view, &info.block_id),
        label: change_label(t, number),
        summary: summary_of(info),
        facts,
        diff: info
            .parent
            .as_deref()
            .filter(|_| edited)
            .map(|parent| LinkView {
                href: diff_path(slug, view, parent, &info.block_id, &title),
                label: t.msg(Key::HistoryLinkDiff).with("number", number).text(),
            }),
        version: (!gone).then(|| LinkView {
            href: at_path(slug, view, &info.block_id, &title),
            label: t.msg(Key::HistoryLinkVersion).with("number", number).text(),
        }),
    }
}

/// `GET /{slug}/-/history/{title}`.
pub(crate) async fn history(
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
    let t = page.t();
    let done = blocking(move || {
        let start = start(&services, &headers, &slug, &params)?;
        let outcome =
            services
                .read
                .page_history(&start.wiki, start.full.as_deref(), &title, start.after);
        match outcome {
            Ok(HistoryOutcome::Page(history)) => Ok((start, history)),
            Ok(HistoryOutcome::Ambiguous { candidates }) => Err(ambiguous(
                t,
                start.wiki,
                start.view.as_deref(),
                &title,
                candidates,
            )),
            Err(e) => Err(failed(&services, start.wiki, "a page's history", e)),
        }
    })
    .await;
    settle(&page, done, |(start, history)| {
        render_history(&page, &start, &history)
    })
}

fn render_history(page: &Page, start: &Start, history: &PageHistory) -> Response {
    let t = page.t();
    let wiki = &start.wiki;
    let slug = wiki.slug.as_str();
    let view = start.view.as_deref();
    let shell = Shell::of(wiki);
    let rows: Vec<RowView> = history
        .rows
        .iter()
        .map(|row| row_view(t, slug, view, &history.title, row))
        .collect();
    let empty = page.get(if history.older.is_some() {
        Key::HistoryNoneSoFar
    } else {
        Key::HistoryNone
    });
    let rendered = HistoryTpl {
        page,
        shell: &shell,
        title: t
            .msg(Key::TitleHistory)
            .with("page", &history.title)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: t
            .msg(Key::HistoryHeading)
            .with("title", &history.title)
            .text(),
        view_note: view_note(t, view),
        empty,
        rows,
        older: history.older.map(|after| LinkView {
            href: history_path(slug, view, &history.title, Some(after)),
            label: page.get(Key::HistoryOlder),
        }),
        beyond: history.beyond_window.then(|| page.get(Key::HistoryBeyond)),
        current: LinkView {
            href: WikiLinks::new(t, slug, view).page_href(&history.title),
            label: t
                .msg(Key::HistoryCurrent)
                .with("title", &history.title)
                .text(),
        },
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

// ------------------------------------------------------------------------------------ one change

struct DidView {
    text: Rich,
    diff: Option<LinkView>,
}

#[derive(Template)]
#[template(path = "change.html")]
struct ChangeTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    signed: String,
    statement: String,
    summary: Option<String>,
    facts: Vec<String>,
    did: Vec<DidView>,
    too_large: Option<String>,
    more: Option<String>,
}

/// What the change page says of who signed, and for whom: the service signs; the people are those the message
/// names. A person is never said to have signed.
fn statement(t: T, provenance: &Provenance) -> String {
    match provenance {
        Provenance::Recorded {
            kind: HistoryChangeKind::Proposal | HistoryChangeKind::Rollback,
            proposed_by,
            accepted_by,
        } => t
            .msg(Key::ChangeSignedBy)
            .with("proposer", who_text(t, proposed_by))
            .with("acceptor", who_text(t, accepted_by))
            .text(),
        Provenance::Recorded { .. } => t.get(Key::ChangeSignedService),
        Provenance::NotRecorded => t.get(Key::PeopleUnrecorded),
    }
}

fn did_view(
    t: T,
    links: &WikiLinks,
    slug: &str,
    view: Option<&str>,
    info: &ChangeInfo,
    did: &Did,
) -> DidView {
    let page_link = |title: &str| (title.to_owned(), links.page_href(title));
    let sentence = |key: Key, title: &str, linked: bool| {
        let msg = t.msg(key);
        if linked {
            let (label, href) = page_link(title);
            msg.link("title", label, href).html()
        } else {
            msg.with("title", title).html()
        }
    };
    let diff_for = |title: &str| {
        info.parent.as_deref().map(|parent| LinkView {
            href: diff_path(slug, view, parent, &info.block_id, title),
            label: t.msg(Key::ChangeLinkDiff).with("title", title).text(),
        })
    };
    match did {
        Did::Added(Subject::Page { title }) => DidView {
            text: sentence(Key::ChangeDidAdded, title, true),
            diff: None,
        },
        Did::Edited(Subject::Page { title }) => DidView {
            text: sentence(Key::ChangeDidEdited, title, true),
            diff: diff_for(title),
        },
        // A page that is gone has no address to link to.
        Did::Removed(Subject::Page { title }) => DidView {
            text: sentence(Key::ChangeDidRemoved, title, false),
            diff: None,
        },
        Did::Moved {
            from: Subject::Page { title: from },
            to: Subject::Page { title: to },
        } => {
            let (label, href) = page_link(to);
            DidView {
                text: t
                    .msg(Key::ChangeDidMoved)
                    .with("from", from)
                    .link("to", label, href)
                    .html(),
                diff: None,
            }
        }
        Did::Added(Subject::File) => file(t, Key::ChangeDidFileAdded),
        Did::Edited(Subject::File) => file(t, Key::ChangeDidFileEdited),
        Did::Removed(Subject::File) => file(t, Key::ChangeDidFileRemoved),
        Did::Moved { .. } => file(t, Key::ChangeDidFileMoved),
    }
}

fn file(t: T, key: Key) -> DidView {
    DidView {
        text: t.msg(key).html(),
        diff: None,
    }
}

/// `GET /{slug}/-/change/{block}`.
pub(crate) async fn change(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, block))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let done = blocking(move || {
        let start = start(&services, &headers, &slug, &params)?;
        match services
            .read
            .change(&start.wiki, start.full.as_deref(), &block)
        {
            Ok(ChangeOutcome::Found(detail)) => Ok((start, detail)),
            Ok(ChangeOutcome::NoSuchChange) => Err(Early::NoSuchChange(start.wiki)),
            Err(e) => Err(failed(&services, start.wiki, "a change", e)),
        }
    })
    .await;
    settle(&page, done, |(start, detail)| {
        render_change(&page, &start, &detail)
    })
}

fn render_change(page: &Page, start: &Start, detail: &ChangeDetail) -> Response {
    let t = page.t();
    let wiki = &start.wiki;
    let slug = wiki.slug.as_str();
    let view = start.view.as_deref();
    let info = &detail.info;
    let links = WikiLinks::new(t, slug, view);
    let shell = Shell::of(wiki);
    let mut facts = people(t, &info.provenance);
    facts.extend(flags(t, info));
    let rendered = ChangeTpl {
        page,
        shell: &shell,
        title: t
            .msg(Key::TitleChange)
            .with("number", info.number)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.msg(Key::ChangeHeading).with("number", info.number).text(),
        view_note: view_note(t, view),
        signed: page.get(Key::ChangeSigned),
        statement: statement(t, &info.provenance),
        summary: summary_of(info),
        facts,
        did: detail
            .did
            .iter()
            .map(|d| did_view(t, &links, slug, view, info, d))
            .collect(),
        too_large: detail.too_large.then(|| page.get(Key::ChangeTooLarge)),
        more: (detail.omitted > 0)
            .then(|| t.msg(Key::ChangeDidMore).n(detail.omitted as u64).text()),
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

// ------------------------------------------------------------------------------------ a page's difference

pub(crate) struct SegmentView {
    pub(crate) text: String,
    pub(crate) marked: bool,
}

pub(crate) struct LineView {
    pub(crate) removed: bool,
    pub(crate) added: bool,
    pub(crate) note: Option<String>,
    pub(crate) segments: Vec<SegmentView>,
}

pub(crate) struct HunkTpl {
    pub(crate) heading: String,
    pub(crate) raw: Option<String>,
    pub(crate) lines: Vec<LineView>,
}

#[derive(Template)]
#[template(path = "diff.html")]
struct DiffTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    range: Rich,
    summary: Vec<String>,
    notes: Vec<String>,
    refinement: Option<String>,
    hunks: Vec<HunkTpl>,
    added_label: String,
    removed_label: String,
    current: LinkView,
}

pub(crate) fn hunk_view(t: T, hunk: &HunkView) -> HunkTpl {
    match hunk {
        HunkView::Raw(text) => HunkTpl {
            heading: t.get(Key::DiffHunkPlain),
            raw: Some(text.clone()),
            lines: Vec::new(),
        },
        HunkView::Lines(h) => HunkTpl {
            heading: match h.new_start {
                Some(line) => t.msg(Key::DiffHunkNear).with("line", line).text(),
                None => t.get(Key::DiffHunkPlain),
            },
            raw: None,
            lines: h
                .lines
                .iter()
                .map(|line| LineView {
                    removed: line.kind == LineKind::Removed,
                    added: line.kind == LineKind::Added,
                    note: (line.kind == LineKind::Marker).then(|| t.get(Key::DiffNoNewline)),
                    segments: line
                        .segments
                        .iter()
                        .map(|s| SegmentView {
                            text: s.text.clone(),
                            marked: s.marked,
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

/// `GET /{slug}/-/diff/{from}/{to}/{title}`.
pub(crate) async fn diff(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String, String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, from, to, title))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let t = page.t();
    let done = blocking(move || {
        let start = start(&services, &headers, &slug, &params)?;
        match services
            .read
            .page_diff(&start.wiki, start.full.as_deref(), &from, &to, &title)
        {
            Ok(DiffOutcome::Found(diff)) => Ok((start, diff)),
            Ok(DiffOutcome::NoSuchChange) => Err(Early::NoSuchChange(start.wiki)),
            Ok(DiffOutcome::Ambiguous { candidates }) => Err(ambiguous(
                t,
                start.wiki,
                start.view.as_deref(),
                &title,
                candidates,
            )),
            Err(e) => Err(failed(&services, start.wiki, "a difference", e)),
        }
    })
    .await;
    settle(&page, done, |(start, diff)| {
        render_diff(&page, &start, &diff)
    })
}

fn render_diff(page: &Page, start: &Start, diff: &PageDiff) -> Response {
    let t = page.t();
    let wiki = &start.wiki;
    let slug = wiki.slug.as_str();
    let view = start.view.as_deref();
    let shell = Shell::of(wiki);
    // The text summary (A11Y-5): what the difference comes to, in words.
    let mut summary = Vec::new();
    if diff.added > 0 {
        summary.push(t.msg(Key::DiffSummaryAdded).n(diff.added).text());
    }
    if diff.removed > 0 {
        summary.push(t.msg(Key::DiffSummaryRemoved).n(diff.removed).text());
    }
    if summary.is_empty() {
        summary.push(page.get(Key::DiffSummaryNone));
    }
    let mut notes = Vec::new();
    match diff.status {
        DiffStatus::Added => notes.push(page.get(Key::DiffStatusAdded)),
        DiffStatus::Removed => notes.push(page.get(Key::DiffStatusRemoved)),
        DiffStatus::Unchanged => notes.push(page.get(Key::DiffStatusUnchanged)),
        DiffStatus::Modified | DiffStatus::Other => {}
    }
    if diff.no_lines {
        notes.push(page.get(Key::DiffNoLines));
    }
    if diff.truncated {
        notes.push(
            t.msg(Key::DiffTruncated)
                .with("limit", MAX_DIFF_LINES)
                .text(),
        );
    }
    let marked = diff.hunks.iter().any(|h| match h {
        HunkView::Lines(h) => h.lines.iter().any(|l| l.segments.iter().any(|s| s.marked)),
        HunkView::Raw(_) => false,
    });
    let range = t
        .msg(Key::DiffFromTo)
        .link(
            "from",
            change_label(t, diff.from_number),
            change_path(slug, view, &diff.from),
        )
        .link(
            "to",
            change_label(t, diff.to_number),
            change_path(slug, view, &diff.to),
        )
        .html();
    let rendered = DiffTpl {
        page,
        shell: &shell,
        title: t
            .msg(Key::TitleDiff)
            .with("page", &diff.title)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.msg(Key::DiffHeading).with("title", &diff.title).text(),
        view_note: view_note(t, view),
        range,
        summary,
        notes,
        refinement: marked.then(|| page.get(Key::DiffRefinement)),
        hunks: diff.hunks.iter().map(|h| hunk_view(t, h)).collect(),
        added_label: page.get(Key::DiffAdded),
        removed_label: page.get(Key::DiffRemoved),
        current: LinkView {
            href: WikiLinks::new(t, slug, view).page_href(&diff.title),
            label: t.msg(Key::HistoryCurrent).with("title", &diff.title).text(),
        },
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

// ------------------------------------------------------------------------------------ a permalink

/// What a permalink request came to, past the early ends.
enum Then {
    Page {
        start: Start,
        view: Box<inklkunn_core::read::PageView>,
        change: Box<ChangeInfo>,
        asked: String,
    },
    Missing {
        start: Start,
        title: String,
        change: Box<ChangeInfo>,
    },
}

/// `GET /{slug}/-/at/{block}/{title}`.
pub(crate) async fn at(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, block, title))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let t = page.t();
    let done = blocking(move || {
        let start = start(&services, &headers, &slug, &params)?;
        let links = WikiLinks::new(t, start.wiki.slug.as_str(), start.view.as_deref());
        match services
            .read
            .page_at(&start.wiki, start.full.as_deref(), &block, &title, &links)
        {
            Ok(AtOutcome::Found { view, change }) => Ok(Then::Page {
                start,
                view,
                change,
                asked: title,
            }),
            Ok(AtOutcome::PageMissing { title, change }) => Ok(Then::Missing {
                start,
                title,
                change,
            }),
            Ok(AtOutcome::NoSuchChange) => Err(Early::NoSuchChange(start.wiki)),
            Ok(AtOutcome::Ambiguous { candidates }) => Err(ambiguous(
                t,
                start.wiki,
                start.view.as_deref(),
                &title,
                candidates,
            )),
            Err(e) => Err(failed(&services, start.wiki, "a page at a change", e)),
        }
    })
    .await;
    settle(&page, done, |then| match then {
        Then::Page {
            start,
            view,
            change,
            asked,
        } => render_at(&page, &start, &view, &change, &asked),
        Then::Missing {
            start,
            title,
            change,
        } => {
            let t = page.t();
            let link = change_link(t, &start, &change);
            notice(
                &page,
                &start.wiki,
                StatusCode::NOT_FOUND,
                Key::AtMissingHeading,
                vec![
                    t.msg(Key::AtMissingBody)
                        .with("title", &title)
                        .with("change", &link.label)
                        .text(),
                ],
                vec![Choice {
                    label: link.label,
                    href: link.href,
                }],
                None,
            )
        }
    })
}

fn change_link(t: T, start: &Start, change: &ChangeInfo) -> LinkView {
    LinkView {
        href: change_path(
            start.wiki.slug.as_str(),
            start.view.as_deref(),
            &change.block_id,
        ),
        label: change_label(t, change.number),
    }
}

fn render_at(
    page: &Page,
    start: &Start,
    view: &inklkunn_core::read::PageView,
    change: &ChangeInfo,
    asked: &str,
) -> Response {
    let t = page.t();
    let wiki = &start.wiki;
    let links = WikiLinks::new(t, wiki.slug.as_str(), start.view.as_deref());
    let link = change_link(t, start, change);
    let current = links.page_href(&view.meta.title);
    let banner = Banner {
        lines: vec![
            t.msg(Key::AtBanner)
                .link("change", link.label, link.href)
                .html(),
            t.msg(Key::AtCurrent)
                .link("current", page.get(Key::AtCurrentLabel), current)
                .html(),
            // The page's links are drawn against today's titles, not those of the point it shows.
            t.msg(Key::AtLinks).html(),
        ],
    };
    found_with(
        page,
        wiki,
        view,
        asked,
        start.view.as_deref(),
        Variant {
            title: Some(
                t.msg(Key::TitleAt)
                    .with("page", &view.meta.title)
                    .with("number", change.number)
                    .with("wiki", wiki.display_name.as_str())
                    .product()
                    .text(),
            ),
            banner: Some(banner),
        },
    )
}
