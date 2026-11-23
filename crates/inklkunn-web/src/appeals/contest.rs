//! *Contest this* (RFC 006 D-9; handoff G-T5): the form that files an appeal against one action, and the act. **The form opens only
//! for the person the action affects**, while the action is in force: to anyone else, and for an action that is not there, it says
//! what is said of an action that is not there. The act is the core's, which authorizes it (reading and being the person affected,
//! **never capped by a restriction**), checks the limits and records it; this module holds no record and no key.
//!
//! No script. A problem with the text shows the form again with what was typed; every other end is a page of its own.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::appeal::{AppealError, ContestRef, Contestable};

use super::paths::{self, fields};
use super::words;
use crate::catalog::Key;
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, from_this_site, see, serve};
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

#[derive(Template)]
#[template(path = "appeal_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    what: Vec<String>,
    proposal: Option<LinkView>,
    problems: Vec<String>,
    action: String,
    kind_field: &'static str,
    kind: &'static str,
    id_field: &'static str,
    id: String,
    text_field: &'static str,
    text_label: String,
    text_hint: String,
    text: String,
    submit: String,
    back: LinkView,
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    contesting: &Contestable,
    typed: &str,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let slug = ctx.slug();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let (what, proposal) = words::what_lines(t, slug, &contesting.what);
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleContest)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::ContestHeading),
        intro: t.get(Key::ContestIntro),
        what,
        proposal,
        problems,
        action: paths::contest_action(slug),
        kind_field: fields::KIND,
        kind: contesting.contest.kind.as_str(),
        id_field: fields::ID,
        id: contesting.contest.id.clone(),
        text_field: fields::TEXT,
        text_label: t.get(Key::ContestTextLabel),
        text_hint: t
            .msg(Key::ContestTextHint)
            .with("limit", inklkunn_core::appeal::MAX_TEXT_BYTES / 1024)
            .text(),
        text: typed.to_owned(),
        submit: t.get(Key::ContestSubmit),
        back: LinkView {
            href: paths::mine(slug),
            label: t.get(Key::ContestBack),
        },
    }
    .render();
    answer(status, &head, rendered)
}

fn back(page: &Page, ctx: &Ctx) -> Vec<Choice> {
    vec![Choice {
        label: page.get(Key::ContestBack),
        href: paths::mine(ctx.slug()),
    }]
}

/// `GET /{slug}/-/appeals/new?kind=…&id=…`.
pub(crate) async fn contest_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: PathName,
    params: Params,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let what = paths::contest_of(
            params.get(fields::KIND).map_or("", String::as_str),
            params.get(fields::ID).map_or("", String::as_str),
        );
        let Some(what) = what else {
            return super::refused(page, &ctx, &AppealError::NotAffected, back(page, &ctx));
        };
        match services
            .appeals
            .contesting(&ctx.principal, &ctx.wiki, &what)
        {
            Ok(contesting) if contesting.filed.is_some() => super::refused(
                page,
                &ctx,
                &AppealError::AlreadyOpen {
                    appeal: contesting.filed.unwrap_or_default(),
                },
                back(page, &ctx),
            ),
            Ok(contesting) => form_page(page, &ctx, &contesting, "", Vec::new(), StatusCode::OK),
            Err(error) => super::refused(page, &ctx, &error, back(page, &ctx)),
        }
    })
    .await
}

/// `POST /{slug}/-/appeals/new`.
pub(crate) async fn contest_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        let t = page.t();
        let given = match crate::proposals::urlencoded::fields(&headers, form) {
            Ok(fields) => fields,
            Err(status) => {
                let key = if status == StatusCode::PAYLOAD_TOO_LARGE {
                    Key::InvalidFormTooLarge
                } else {
                    Key::InvalidIncomplete
                };
                return crate::wiki::notice(
                    page,
                    &ctx.wiki,
                    status,
                    Key::RefusedInvalidHeading,
                    vec![t.get(key)],
                    back(page, &ctx),
                    None,
                );
            }
        };
        let field = |name: &str| given.get(name).cloned().unwrap_or_default();
        let Some(what): Option<ContestRef> =
            paths::contest_of(&field(fields::KIND), &field(fields::ID))
        else {
            return super::refused(page, &ctx, &AppealError::NotAffected, back(page, &ctx));
        };
        let text = field(fields::TEXT);
        match services
            .appeals
            .file(&ctx.principal, &ctx.wiki, &what, &text)
        {
            Ok(_) => see(&paths::mine_done(ctx.slug())),
            // A problem with the text shows the form again, with what was typed.
            Err(AppealError::Text(fault)) => {
                match services
                    .appeals
                    .contesting(&ctx.principal, &ctx.wiki, &what)
                {
                    Ok(contesting) => form_page(
                        page,
                        &ctx,
                        &contesting,
                        &text,
                        vec![words::text_problem(t, fault)],
                        StatusCode::UNPROCESSABLE_ENTITY,
                    ),
                    Err(error) => super::refused(page, &ctx, &error, back(page, &ctx)),
                }
            }
            Err(error) => super::refused(page, &ctx, &error, back(page, &ctx)),
        }
    })
    .await
}
