//! The inbox (`/-/inbox`; RFC 004 D-9; handoff 004 T6): what has happened to a person's proposals and to the
//! pages they follow, newest first, and the pages they follow with a way to stop.
//!
//! **The inbox is account-wide**, so it lives outside any wiki's address, like the list of wikis, and is
//! registered the same way (guarded by the read action: every line it shows is read through the same rule as
//! the page it is about, as the person who opens it). It is the one page a visitor with no account is told, in
//! words, that an account is needed.
//!
//! **An event is ids and a kind** ([`inklkunn_core::notify`]); this page is where it becomes words, and
//! [`inklkunn_core::Inbox`] reads what it is about as the person looking, now: an event about a wiki that
//! stopped being theirs to read is one sentence that names nothing ([`Key::InboxEventUnreadable`]). Names and
//! summaries are other people's text and are escaped like any other.
//!
//! **Marking as read** is a `POST` from this site and goes back to the inbox; opening the page changes nothing.

use askama::Template;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::inbox::{About, Entry, Followed};
use inklkunn_markup::path::path_to_title;
use inklkunn_store::{InboxEventKind, WatchTarget};

use crate::catalog::{Key, Rich, T};
use crate::pages::{Page, PageState, answer, render_form_refused};
use crate::time::{rfc3339, utc_minutes};
use crate::url::encode_title;
use crate::wiki::{blocking, redirect, render_failed, services_of};

/// The address of the inbox.
pub(crate) const PATH: &str = "/-/inbox";

struct EventView {
    sentence: Rich,
    unread: bool,
    iso: String,
    when: String,
}

struct FollowView {
    label: Rich,
    action: String,
    scope: &'static str,
    button: String,
}

#[derive(Template)]
#[template(path = "inbox.html")]
struct InboxTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    /// Set when the visitor has no account: the page says so and shows nothing else.
    account_needed: Option<String>,
    intro: String,
    none: String,
    new_mark: String,
    mark_all: String,
    any_unread: bool,
    events: Vec<EventView>,
    following_heading: String,
    following_none: String,
    following: Vec<FollowView>,
    wikis_link: String,
}

/// The sentence of an event: the kind's message with the person, the proposal and the wiki filled in, or the
/// bare fact when the reader may not be told what it was about.
fn sentence(t: T, entry: &Entry) -> Rich {
    let About::Readable {
        slug,
        wiki,
        proposal,
        actor,
    } = &entry.about
    else {
        return t.msg(Key::InboxEventUnreadable).html();
    };
    let key = match entry.kind {
        InboxEventKind::Reviewed => Key::InboxEventReviewed,
        InboxEventKind::Accepted => Key::InboxEventAccepted,
        InboxEventKind::Declined => Key::InboxEventDeclined,
        InboxEventKind::NeedsRevision => Key::InboxEventNeedsRevision,
        InboxEventKind::Withdrawn => Key::InboxEventWithdrawn,
        InboxEventKind::Superseded => Key::InboxEventSuperseded,
        InboxEventKind::Replied => Key::InboxEventReplied,
        InboxEventKind::Mentioned => Key::InboxEventMentioned,
        InboxEventKind::WatchedPageChanged => Key::InboxEventWatched,
    };
    let person = actor.clone().unwrap_or_else(|| t.get(Key::InboxSomeone));
    // The proposal is named by its summary, linked to its own page; an event with none names the wiki.
    let (label, href) = match proposal {
        Some(p) => (p.summary.clone(), format!("/{slug}/-/proposals/{}", p.id)),
        None => (wiki.clone(), format!("/{slug}")),
    };
    t.msg(key)
        .with("person", person)
        .link("proposal", label, href)
        .link("wiki", wiki, format!("/{slug}"))
        .html()
}

/// The title a followed thing is about: a page's, or the title the followed pages are under.
fn followed_title(target: &WatchTarget) -> Option<String> {
    match target {
        WatchTarget::Page(path) => path_to_title(path),
        WatchTarget::Namespace(prefix) => path_to_title(&format!("{prefix}x.md"))
            .and_then(|t| t.strip_suffix("/x").map(str::to_owned)),
    }
}

fn follow_view(t: T, f: &Followed) -> Option<FollowView> {
    let title = followed_title(&f.target)?;
    let (key, scope) = match f.target {
        WatchTarget::Page(_) => (Key::InboxFollowingPage, "page"),
        WatchTarget::Namespace(_) => (Key::InboxFollowingNamespace, "namespace"),
    };
    let href = format!("/{}/{}", f.slug, encode_title(&title));
    let in_words = t.msg(key).with("page", &title).with("wiki", &f.wiki).text();
    Some(FollowView {
        label: t
            .msg(key)
            .link("page", &title, href)
            .link("wiki", &f.wiki, format!("/{}", f.slug))
            .html(),
        action: format!("/{}/-/watch/{}", f.slug, encode_title(&title)),
        scope,
        button: t.msg(Key::InboxUnfollow).with("what", in_words).text(),
    })
}

fn render(page: &Page, status: StatusCode, entries: &[Entry], followed: &[Followed]) -> Response {
    let t = page.t();
    let events: Vec<EventView> = entries
        .iter()
        .map(|e| EventView {
            sentence: sentence(t, e),
            unread: e.unread,
            iso: rfc3339(e.at),
            when: utc_minutes(e.at),
        })
        .collect();
    let rendered = InboxTpl {
        page,
        title: page.title(Key::TitleInbox),
        heading: page.get(Key::InboxHeading),
        account_needed: None,
        intro: page.get(Key::InboxIntro),
        none: page.get(Key::InboxNone),
        new_mark: page.get(Key::InboxNew),
        mark_all: page.get(Key::InboxMarkAllRead),
        any_unread: events.iter().any(|e| e.unread),
        events,
        following_heading: page.get(Key::InboxFollowingHeading),
        following_none: page.get(Key::InboxFollowingNone),
        following: followed.iter().filter_map(|f| follow_view(t, f)).collect(),
        wikis_link: page.get(Key::WikiNavWikis),
    }
    .render();
    answer(status, page, rendered)
}

/// The answer to a visitor with no account: it says so, and shows nothing of anyone's.
pub(crate) fn needs_account(page: &Page) -> Response {
    let rendered = InboxTpl {
        page,
        title: page.title(Key::TitleInbox),
        heading: page.get(Key::InboxAccountHeading),
        account_needed: Some(page.get(Key::InboxAccountBody)),
        intro: String::new(),
        none: String::new(),
        new_mark: String::new(),
        mark_all: String::new(),
        any_unread: false,
        events: Vec::new(),
        following_heading: String::new(),
        following_none: String::new(),
        following: Vec::new(),
        wikis_link: page.get(Key::WikiNavWikis),
    }
    .render();
    answer(StatusCode::FORBIDDEN, page, rendered)
}

/// `GET /-/inbox`.
pub(crate) async fn inbox_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
) -> Response {
    let Some(services) = services_of(&state) else {
        return needs_account(&page);
    };
    let loaded = blocking(move || {
        let principal = services.identity.principal(&headers);
        let entries = services.inbox.entries(&principal, false)?;
        let followed = services.inbox.followed(&principal)?;
        Ok::<_, inklkunn_core::inbox::InboxError>((entries, followed))
    })
    .await;
    match loaded {
        Some(Ok((entries, followed))) => render(&page, StatusCode::OK, &entries, &followed),
        Some(Err(inklkunn_core::inbox::InboxError::NeedsAccount)) => needs_account(&page),
        Some(Err(_)) | None => render_failed(&page),
    }
}

/// `POST /-/inbox`: mark everything read, and go back to the inbox. A form from another site is refused.
pub(crate) async fn mark_read(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
) -> Response {
    let page = page.without_switcher();
    if !crate::lang::from_this_site(&headers) {
        return answer(StatusCode::FORBIDDEN, &page, render_form_refused(&page));
    }
    let Some(services) = services_of(&state) else {
        return needs_account(&page);
    };
    let done = blocking(move || {
        let principal = services.identity.principal(&headers);
        services.inbox.mark_all_read(&principal)
    })
    .await;
    match done {
        Some(Ok(_)) => redirect(PATH),
        Some(Err(inklkunn_core::inbox::InboxError::NeedsAccount)) => needs_account(&page),
        Some(Err(_)) | None => render_failed(&page),
    }
}
