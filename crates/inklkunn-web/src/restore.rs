//! The form that proposes restoring a removed page (RFC 006 D-10; handoff G-T6): `/{slug}/-/restore/{title}`.
//!
//! The page a removed page answers with ([`crate::removed`]) offers it. It asks nothing of the person but a click: **what
//! the proposal is, is the wiki's to say** ([`inklkunn_core::read::ReadService::removed_page`]), read again at every request,
//! never taken from the form. The form carries only the way it was offered (`mode`), so that a click on what has since
//! become another offer is refused and shown again (409) instead of making a proposal the person did not read about.
//!
//! **The way back is worked out here and nowhere else** ([`inklkunn_core::read::ReadService::way_back`], G-T6 correction S-1):
//! it costs a process for each page the removing change wrote, so the page of the removal, the raw 410 and the API's
//! `page-gone` offer only the link, by the `propose` rung, and this form (behind that rung) says which way there is and why
//! there is none (the text is not text, the revision that wrote it is beyond the history this wiki reads, or it was hidden).
//!
//! * **A rollback** of the change that removed the page (RFC 004 D-8), when it merges cleanly over the tip.
//! * **The page again, with the text it had last**, a proposal like any new page, when later changes block that.
//!
//! * **Never hidden text** (INV-13; RFC 006 D-8): when moderation hid the text that either would write again (the page's last
//!   revision, and for a rollback every page it writes), the form offers neither: it says so, names the pages by their titles,
//!   and offers the empty new page. A request for either is refused (`409`) whatever mode it names, so a draft that carries the
//!   hidden text is never made. A hidden summary or author of that revision leaves the text free to come back.
//!
//! Either way the result is a **draft**: the person's own until they send it for review, editable, with every check of a
//! proposal (the `propose` rung on the page, the protections, the quotas) made by the service when it is made. **The order of
//! a request is the order of every write form:** a form from this site, the session's token, a wiki the visitor may read, an
//! account, the `propose` rung (a person who may not sees why, not the form), and only then the fields.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::proposal::rollback::restore_edit;
use inklkunn_core::read::{Lookup, Presentation};
use inklkunn_core::read::{ReadError, RemovedPage, Restore, RollbackOutcome};

use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::present::WikiLinks;
use crate::proposals::{
    Ctx, SMALL_FORM_LIMIT, fields_of, from_this_site, needs_account, refused, see, serve,
};
use crate::url::view_ref;
use crate::wiki::{Choice, Shell, generic_not_found, missing_page, notice, with_retry_after};

mod fields;
mod paths;

use fields::{MODE, MODE_LAST_TEXT, MODE_ROLLBACK, VIEW};
pub(crate) use paths::path;

type PathTitle = Result<Path<(String, String)>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;
type Raw = crate::proposals::urlencoded::Raw;

/// The body bound of the form: a word and a view's name.
pub(crate) const FORM_LIMIT: usize = SMALL_FORM_LIMIT;

/// What the form is about once the request is checked: the wiki's context, the view and the removed page.
struct Subject_ {
    view_full: Option<String>,
    view_name: Option<String>,
    removed: RemovedPage,
    /// The way back, worked out here and nowhere else (G-T6 correction S-1): a visit to the removed page costs none of it.
    restore: Restore,
}

/// How reading the removed page came out, before anything is drawn.
enum Found {
    Removed(Box<Subject_>),
    /// The title leads to a page now: the way to it.
    Page(String),
    /// No page, and none that was removed.
    Never,
    /// The history could not be asked now.
    Unavailable,
}

fn find(ctx: &Ctx, t: T, title: &str, view: Option<&str>) -> Result<Found, Box<Response>> {
    let (view_full, view_name) = match view.filter(|v| !v.is_empty()) {
        None => (None, None),
        Some(name) => match view_ref(name) {
            Some(full) => (Some(full), Some(name.to_owned())),
            None => return Ok(Found::Never),
        },
    };
    let read = &ctx.services.read;
    match read.lookup(&ctx.wiki, view_full.as_deref(), title) {
        Ok(Lookup::Missing) => {}
        Ok(_) => {
            return Ok(Found::Page(
                WikiLinks::new(t, ctx.slug(), view_name.as_deref()).page_href(title),
            ));
        }
        Err(ReadError::CouldNotBeReadNow(cause)) => {
            ctx.log(&format!("a page could not be read now: {cause}"));
            return Ok(Found::Unavailable);
        }
        Err(_) => return Ok(Found::Never),
    }
    let removed = match read.removed_page(&ctx.wiki, view_full.as_deref(), title) {
        Ok(Some(removed)) => removed,
        Ok(None) => return Ok(Found::Never),
        Err(ReadError::CouldNotBeReadNow(cause)) => {
            ctx.log(&format!("a history could not be read now: {cause}"));
            return Ok(Found::Unavailable);
        }
        Err(_) => return Ok(Found::Never),
    };
    match read.way_back(&ctx.wiki, view_full.as_deref(), &removed) {
        Ok(restore) => Ok(Found::Removed(Box::new(Subject_ {
            view_full,
            view_name,
            removed,
            restore,
        }))),
        Err(ReadError::CouldNotBeReadNow(cause)) => {
            ctx.log(&format!("a history could not be read now: {cause}"));
            Ok(Found::Unavailable)
        }
        Err(_) => Ok(Found::Never),
    }
}

/// The answer for every case but a removed page that can be restored.
fn other(page: &Page, ctx: &Ctx, title: &str, found: Found) -> Response {
    match found {
        Found::Page(href) => see(&href),
        Found::Never => missing_page(page, &ctx.wiki, title),
        Found::Unavailable => with_retry_after(notice(
            page,
            &ctx.wiki,
            StatusCode::SERVICE_UNAVAILABLE,
            Key::UnavailableHeading,
            vec![page.get(Key::UnavailableBody)],
            Vec::new(),
            None,
        )),
        Found::Removed(_) => generic_not_found(page),
    }
}

/// The checks every request of the form makes before it reads a field: an account that may propose here. A person who may
/// not is told why, with the refusal a proposal's own would give (the reason and the end of a restriction).
fn may_propose(page: &Page, ctx: &Ctx) -> Option<Response> {
    if let Some(response) = needs_account(page, ctx) {
        return Some(response);
    }
    if ctx.may_wiki(Action::Propose) {
        return None;
    }
    Some(refused(
        page,
        ctx,
        &ctx.refusal_wiki(Action::Propose),
        Vec::new(),
    ))
}

fn mode_of(restore: &Restore) -> Option<&'static str> {
    match restore {
        Restore::Rollback { .. } => Some(MODE_ROLLBACK),
        Restore::LastText { .. } => Some(MODE_LAST_TEXT),
        Restore::NotText | Restore::BeyondHistory | Restore::TextHidden { .. } => None,
    }
}

/// The sentence that says the text to be restored was hidden by moderation, with the titles of the pages it is the text of
/// (titles only, never any of the text).
pub(crate) fn text_hidden_words(t: T, titles: &[String]) -> String {
    t.msg(Key::RestoreTextHidden)
        .with("pages", titles.join(", "))
        .text()
}

/// The answer when no mode can be offered: the page that says why (`status`): the text was hidden, is not text, or was written
/// beyond the history this wiki reads; and the way on is the empty new page.
fn no_mode(page: &Page, ctx: &Ctx, subject: &Subject_, status: StatusCode) -> Response {
    let t = page.t();
    let removed = &subject.removed;
    let words = match &subject.restore {
        Restore::TextHidden { titles } => text_hidden_words(t, titles),
        Restore::NotText => t.get(Key::RemovedRestoreNotText),
        Restore::BeyondHistory => t.get(Key::RemovedRestoreBeyondHistory),
        // An offer is a form, not a notice: the caller asks this only when there is no way.
        Restore::Rollback { .. } | Restore::LastText { .. } => {
            return see(&crate::proposals::paths::new_page(
                ctx.slug(),
                Some(&removed.title),
            ));
        }
    };
    notice(
        page,
        &ctx.wiki,
        status,
        Key::RemovedHeading,
        vec![words],
        vec![Choice {
            label: t.get(Key::RestoreBack),
            href: WikiLinks::new(t, ctx.slug(), subject.view_name.as_deref())
                .page_href(&removed.title),
        }],
        Some(Choice {
            label: t.get(Key::ProposeNewPageLink),
            href: crate::proposals::paths::new_page(ctx.slug(), Some(&removed.title)),
        }),
    )
}

#[derive(Template)]
#[template(path = "restore.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    problems: Vec<String>,
    intro: String,
    action: String,
    mode_field: &'static str,
    mode: &'static str,
    view_field: &'static str,
    view: Option<String>,
    button: String,
    back: Choice,
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    subject: &Subject_,
    mode: &'static str,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let removed = &subject.removed;
    let number = removed.removal.number;
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitlePage)
            .with(
                "page",
                t.msg(Key::RestoreHeading)
                    .with("title", &removed.title)
                    .text(),
            )
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t
            .msg(Key::RestoreHeading)
            .with("title", &removed.title)
            .text(),
        problems,
        intro: t
            .msg(if mode == MODE_ROLLBACK {
                Key::RestoreIntroRollback
            } else {
                Key::RestoreIntroLastText
            })
            .with("number", number)
            .with("title", &removed.title)
            .text(),
        action: path(ctx.slug(), &removed.title, subject.view_name.as_deref()),
        mode_field: MODE,
        mode,
        view_field: VIEW,
        view: subject.view_name.clone(),
        button: t.get(Key::RestoreOpenDraft),
        back: Choice {
            label: t.get(Key::RestoreBack),
            href: WikiLinks::new(t, ctx.slug(), subject.view_name.as_deref())
                .page_href(&removed.title),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// `GET /{slug}/-/restore/{title}`.
pub(crate) async fn restore_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: PathTitle,
    params: Params,
) -> Response {
    let (Ok(Path((slug, title))), Some(services)) = (path, crate::wiki::services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = may_propose(page, &ctx) {
            return response;
        }
        match find(&ctx, page.t(), &title, params.get(VIEW).map(String::as_str)) {
            Ok(Found::Removed(subject)) => match mode_of(&subject.restore) {
                Some(mode) => form_page(page, &ctx, &subject, mode, Vec::new(), StatusCode::OK),
                // Nothing can restore it: the way on is the new page (and, for hidden text, the words that say why).
                None => no_mode(page, &ctx, &subject, StatusCode::OK),
            },
            Ok(found) => other(page, &ctx, &title, found),
            Err(response) => *response,
        }
    })
    .await
}

/// `POST /{slug}/-/restore/{title}`: the draft.
pub(crate) async fn restore_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathTitle,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path((slug, title))), Some(services)) = (path, crate::wiki::services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = may_propose(page, &ctx) {
            return response;
        }
        let given = match fields_of(page, &ctx, &headers, form) {
            Ok(given) => given,
            Err(response) => return *response,
        };
        let asked = given.get(MODE).map_or("", String::as_str);
        let subject = match find(&ctx, page.t(), &title, given.get(VIEW).map(String::as_str)) {
            Ok(Found::Removed(subject)) => subject,
            Ok(found) => return other(page, &ctx, &title, found),
            Err(response) => return *response,
        };
        // Whatever mode the form names: hidden text is refused before the mode is looked at.
        let Some(mode) = mode_of(&subject.restore) else {
            return no_mode(page, &ctx, &subject, StatusCode::CONFLICT);
        };
        if asked != mode {
            return form_page(
                page,
                &ctx,
                &subject,
                mode,
                vec![page.get(Key::RestoreChanged)],
                StatusCode::CONFLICT,
            );
        }
        make(page, &ctx, &subject)
    })
    .await
}

/// Make the draft the removed page's way back is, and send the person to it.
fn make(page: &Page, ctx: &Ctx, subject: &Subject_) -> Response {
    let removed = &subject.removed;
    let view = subject.view_full.as_deref();
    let services = &ctx.services;
    let unavailable = |cause: String| {
        ctx.log(&format!("a restoring proposal could not be made: {cause}"));
        with_retry_after(notice(
            page,
            &ctx.wiki,
            StatusCode::SERVICE_UNAVAILABLE,
            Key::UnavailableHeading,
            vec![page.get(Key::UnavailableBody)],
            Vec::new(),
            None,
        ))
    };
    let made = match &subject.restore {
        Restore::Rollback { undoes } => {
            match services.read.rollback_plan(&ctx.wiki, view, undoes) {
                Ok(RollbackOutcome::Plan(plan)) => {
                    services
                        .proposals
                        .create_rollback(&ctx.principal, &ctx.wiki, view, &plan)
                }
                // The plan went away between the offer and the click: the offer is not what it was.
                Ok(RollbackOutcome::No(_)) => return changed(page, ctx, subject),
                Err(e) => return unavailable(e.to_string()),
            }
        }
        Restore::LastText { from } => {
            let text = match services.read.source_at(&ctx.wiki, from, &removed.path) {
                Ok(Some(text)) => text,
                Ok(None) => return changed(page, ctx, subject),
                Err(e) => return unavailable(e.to_string()),
            };
            let Some(edit) = restore_edit(&removed.title, &removed.path, text, &removed.tip) else {
                return changed(page, ctx, subject);
            };
            services
                .proposals
                .create(&ctx.principal, &ctx.wiki, view, edit)
        }
        Restore::NotText | Restore::BeyondHistory | Restore::TextHidden { .. } => {
            return changed(page, ctx, subject);
        }
    };
    match made {
        Ok(record) => see(&crate::proposals::paths::proposal(
            ctx.slug(),
            record.id.as_str(),
        )),
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}

fn changed(page: &Page, ctx: &Ctx, subject: &Subject_) -> Response {
    match mode_of(&subject.restore) {
        Some(mode) => form_page(
            page,
            ctx,
            subject,
            mode,
            vec![page.get(Key::RestoreChanged)],
            StatusCode::CONFLICT,
        ),
        None => no_mode(page, ctx, subject, StatusCode::CONFLICT),
    }
}
