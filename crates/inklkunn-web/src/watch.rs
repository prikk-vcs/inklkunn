//! Following a page, or the pages under a title (`POST /<slug>/-/watch/<Title>`; RFC 004 D-9, RD-8; handoff 004
//! T6): the handler a "follow this page" button on a page posts to. The button itself is the page view's, and
//! arrives with the proposal page's wiring; this is the other half.
//!
//! The form has two fields: `scope` (`page`, the default, or `namespace`: the pages under the title) and `do`
//! (`watch`, the default, or `unwatch`). The answer is a page that says what is now true, with a way back and a
//! way to the inbox. Nothing is changed by a `GET`.
//!
//! **The order of the checks is the order of what may be told.** The form must come from this site. A wiki the
//! visitor may not read, or that is not there, is the generic not-found page, identically. Only then is an
//! account asked for: a visitor with none, in a wiki they may read, is told in words that an account is needed.
//! A title that cannot name a page is the same not-found page, and so is a page the account may not read (the
//! rule that decides is the one every read goes through), so following is no way to find out what exists.

use std::collections::HashMap;

use axum::extract::rejection::{FormRejection, PathRejection};
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::inbox::{InboxError, namespace_target, page_target};
use inklkunn_store::WatchTarget;

use crate::catalog::Key;
use crate::discover::resolve;
use crate::pages::{Page, PageState, answer, render_form_refused};
use crate::url::encode_title;
use crate::wiki::{Choice, blocking, generic_not_found, notice, render_failed};

/// What was done.
enum Done {
    Followed(WatchTarget),
    Stopped(WatchTarget),
    NeedsAccount,
    Full,
    NotFound,
}

/// `POST /{slug}/-/watch/{*title}`.
pub(crate) async fn watch(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let page = page.without_switcher();
    let Ok(Path((slug, title))) = path else {
        return generic_not_found(&page);
    };
    if !crate::lang::from_this_site(&headers) {
        return answer(StatusCode::FORBIDDEN, &page, render_form_refused(&page));
    }
    let Some((services, wiki)) = resolve(&state, headers.clone(), slug).await else {
        return generic_not_found(&page);
    };
    let Ok(Form(form)) = form else {
        return generic_not_found(&page);
    };
    let namespace = form.get("scope").is_some_and(|v| v == "namespace");
    let stop = form.get("do").is_some_and(|v| v == "unwatch");
    let (for_work, record, wanted) = (services.clone(), wiki.clone(), title.clone());
    let done = blocking(move || {
        let principal = for_work.identity.principal(&headers);
        let target = if namespace {
            namespace_target(&wanted)
        } else {
            page_target(&wanted)
        };
        let Ok(target) = target else {
            return Done::NotFound;
        };
        let result = if stop {
            for_work
                .inbox
                .unwatch(&principal, &record, &target)
                .map(|_| Done::Stopped(target.clone()))
        } else {
            for_work
                .inbox
                .watch(&principal, &record, target.clone())
                .map(|_| Done::Followed(target.clone()))
        };
        match result {
            Ok(done) => done,
            Err(InboxError::NeedsAccount) => Done::NeedsAccount,
            Err(InboxError::TooManyWatches) => Done::Full,
            Err(InboxError::NotAllowed | InboxError::BadTitle(_)) => Done::NotFound,
            Err(InboxError::Store(e)) => {
                (for_work.log)(&format!("a watch could not be recorded: {e}"));
                Done::NotFound
            }
        }
    })
    .await;
    let t = page.t();
    let back = Choice {
        label: t.msg(Key::WatchBack).with("title", &title).text(),
        href: format!("/{}/{}", wiki.slug, encode_title(&title)),
    };
    let inbox = Choice {
        label: page.get(Key::WatchToInbox),
        href: crate::inbox::PATH.to_owned(),
    };
    let (status, heading, text) = match done {
        None => return render_failed(&page),
        Some(Done::NotFound) => return generic_not_found(&page),
        Some(Done::NeedsAccount) => (
            StatusCode::FORBIDDEN,
            Key::InboxAccountHeading,
            page.get(Key::WatchAccountBody),
        ),
        Some(Done::Full) => (
            StatusCode::CONFLICT,
            Key::WatchHeadingFull,
            page.get(Key::WatchFullBody),
        ),
        Some(Done::Followed(target)) => (
            StatusCode::OK,
            Key::WatchHeadingFollowed,
            t.msg(match target {
                WatchTarget::Page(_) => Key::WatchPageFollowed,
                WatchTarget::Namespace(_) => Key::WatchNamespaceFollowed,
            })
            .with("title", &title)
            .text(),
        ),
        Some(Done::Stopped(target)) => (
            StatusCode::OK,
            Key::WatchHeadingStopped,
            t.msg(match target {
                WatchTarget::Page(_) => Key::WatchPageStopped,
                WatchTarget::Namespace(_) => Key::WatchNamespaceStopped,
            })
            .with("title", &title)
            .text(),
        ),
    };
    notice(
        &page,
        &wiki,
        status,
        heading,
        vec![text],
        vec![back, inbox],
        false,
    )
}
