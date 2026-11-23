//! The form of the `moderate` rung that puts a temporary protection in place on a page or on the pages below a title, renews
//! it, or lifts it (RFC 006 D-7; handoff G-T3). **Web only**: moderation is a deliberate human act, and there is no write API
//! for it (RFC 006 D-12).
//!
//! **The order of a request is the order of every write form.** A form from this site, the CSRF token of the session (decided
//! once by the layer), a wiki the visitor may read (an absent one and a forbidden one are one page), a person with an account,
//! and only then the fields. The act itself is [`Authorizer::protect`](inklkunn_core::Authorizer::protect) and its two
//! companions: they authorize `moderate` again, in the audit trail, check the limit of the platform, and record. This module
//! decides nothing about a protection: it reads fields, asks, and says what was answered.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::governance::{Require, Target};
use inklkunn_core::{ProtectError, Setting};
use inklkunn_store::Role;

use super::{NoticeView, fields, paths, rung_word, views};
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::{Ctx, SMALL_FORM_LIMIT, from_this_site, needs_account, see, serve};
use crate::wiki::{Choice, Shell, generic_not_found, notice, services_of, with_retry_after};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// The body bound of the form: a title, a reason and a few words.
pub(crate) const FORM_LIMIT: usize = SMALL_FORM_LIMIT;

/// How a request that did not end at the form ended, in the fixed cases the words are written for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// The visitor does not hold the role that moderates.
    NotAllowed,
    /// This server keeps no temporary protections.
    NotKept,
    /// The records could not be used; nothing was changed.
    Unavailable,
}

fn outcome_page(page: &Page, ctx: &Ctx, outcome: &Outcome) -> Response {
    let t = page.t();
    let (status, heading, key) = match outcome {
        Outcome::NotAllowed => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            Key::ProtectNotAllowed,
        ),
        Outcome::NotKept => (
            StatusCode::NOT_IMPLEMENTED,
            Key::ProtectHeading,
            Key::ProtectNotKept,
        ),
        Outcome::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeUnavailable,
            Key::ProtectUnavailable,
        ),
    };
    let response = notice(
        page,
        &ctx.wiki,
        status,
        heading,
        vec![t.get(key)],
        vec![Choice {
            label: t.get(Key::ProtectBack),
            href: paths::front(ctx.slug()),
        }],
        None,
    );
    match outcome {
        Outcome::Unavailable => with_retry_after(response),
        Outcome::NotAllowed | Outcome::NotKept => response,
    }
}

/// What was typed, kept to show the form again.
#[derive(Default, Clone)]
struct Typed {
    namespace: bool,
    title: String,
    review: Option<bool>,
    role: Option<Role>,
    reason: String,
    days: String,
}

struct Choosable {
    value: &'static str,
    label: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "protect_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    problems: Vec<String>,
    /// The protections that apply to the title now, in the one display.
    protections: Vec<NoticeView>,
    /// A temporary protection is in force exactly here: what the buttons are.
    in_force: Option<String>,
    action: String,
    target_legend: String,
    targets: Vec<Choosable>,
    target_field: &'static str,
    title_label: String,
    title_field: &'static str,
    typed_title: String,
    require_legend: String,
    requires: Vec<Choosable>,
    require_field: &'static str,
    rung_label: String,
    rung_field: &'static str,
    rungs: Vec<Choosable>,
    reason_label: String,
    reason_field: &'static str,
    typed_reason: String,
    reason_max: usize,
    days_label: String,
    days_field: &'static str,
    typed_days: String,
    act_field: &'static str,
    buttons: Vec<Button>,
    back: Choice,
}

struct Button {
    value: &'static str,
    label: String,
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    typed: &Typed,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let wiki = &ctx.wiki.id;
    let limit = ctx.services.authorizer.protection_limit_days().unwrap_or(0);
    let target = if typed.namespace {
        Target::namespace(&typed.title)
    } else {
        Target::path(&typed.title)
    };
    let (shown, in_force) = match &target {
        Some(target) => (
            views(
                t,
                ctx.slug(),
                &ctx.services
                    .authorizer
                    .protections_on(&ctx.principal, wiki, target.title()),
            ),
            ctx.services
                .authorizer
                .temporary_on(&ctx.principal, wiki, target)
                .is_some(),
        ),
        None => (Vec::new(), false),
    };
    let mut buttons = Vec::new();
    if in_force {
        buttons.push(Button {
            value: fields::ACT_RENEW,
            label: t.get(Key::ProtectRenew),
        });
        buttons.push(Button {
            value: fields::ACT_LIFT,
            label: t.get(Key::ProtectLift),
        });
    } else {
        buttons.push(Button {
            value: fields::ACT_SET,
            label: t.get(Key::ProtectSet),
        });
    }
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleProtect)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::ProtectHeading),
        intro: t.get(Key::ProtectIntro),
        problems,
        protections: shown,
        in_force: in_force.then(|| t.get(Key::ProtectInForce)),
        action: paths::form(ctx.slug()),
        target_legend: t.get(Key::ProtectTargetLegend),
        targets: vec![
            Choosable {
                value: fields::TARGET_PAGE,
                label: t.get(Key::ProtectTargetPage),
                selected: !typed.namespace,
            },
            Choosable {
                value: fields::TARGET_NAMESPACE,
                label: t.get(Key::ProtectTargetNamespace),
                selected: typed.namespace,
            },
        ],
        target_field: fields::TARGET,
        title_label: t.get(Key::ProtectTitleLabel),
        title_field: fields::TITLE,
        typed_title: typed.title.clone(),
        require_legend: t.get(Key::ProtectRequireLegend),
        requires: vec![
            Choosable {
                value: fields::REQUIRE_REVIEW,
                label: t.get(Key::ProtectRequireReview),
                selected: typed.review != Some(false),
            },
            Choosable {
                value: fields::REQUIRE_RUNG,
                label: t.get(Key::ProtectRequireRung),
                selected: typed.review == Some(false),
            },
        ],
        require_field: fields::REQUIRE,
        rung_label: t.get(Key::ProtectRungLabel),
        rung_field: fields::ROLE,
        rungs: Role::ALL
            .into_iter()
            .map(|role| Choosable {
                value: role.as_str(),
                label: rung_word(t, role),
                selected: typed.role == Some(role),
            })
            .collect(),
        reason_label: t.get(Key::ProtectReasonLabel),
        reason_field: fields::REASON,
        typed_reason: typed.reason.clone(),
        reason_max: inklkunn_core::governance::MAX_REASON_CHARS,
        days_label: t.msg(Key::ProtectDaysLabel).with("limit", limit).text(),
        days_field: fields::DAYS,
        typed_days: typed.days.clone(),
        act_field: fields::ACT,
        buttons,
        back: Choice {
            label: t.get(Key::ProtectBack),
            href: back_to(ctx.slug(), typed),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// Where *back to the page* goes: the page itself, or the front page when the title is for a namespace or is not a page's.
fn back_to(slug: &str, typed: &Typed) -> String {
    if typed.namespace || typed.title.is_empty() {
        paths::front(slug)
    } else {
        paths::page(slug, &typed.title)
    }
}

/// The form, filled from the address (`?page=Title`, or `?namespace=Title`).
fn typed_from_address(params: &HashMap<String, String>) -> Typed {
    let (namespace, title) = match (params.get(fields::Q_PAGE), params.get(fields::Q_NAMESPACE)) {
        (_, Some(title)) => (true, title.clone()),
        (Some(title), None) => (false, title.clone()),
        (None, None) => (false, String::new()),
    };
    Typed {
        namespace,
        title,
        review: Some(true),
        ..Typed::default()
    }
}

/// `GET /{slug}/-/protect`.
pub(crate) async fn protect_form(
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
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !ctx
            .services
            .authorizer
            .may_moderate(&ctx.principal, &ctx.wiki.id)
        {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        form_page(
            page,
            &ctx,
            &typed_from_address(&params),
            Vec::new(),
            StatusCode::OK,
        )
    })
    .await
}

/// The form's fields, or the page that says the form was not one.
fn form_fields(
    page: &Page,
    ctx: &Ctx,
    headers: &HeaderMap,
    form: Raw,
) -> Result<HashMap<String, String>, Box<Response>> {
    crate::proposals::urlencoded::fields(headers, form).map_err(|status| {
        let key = if status == StatusCode::PAYLOAD_TOO_LARGE {
            Key::InvalidFormTooLarge
        } else {
            Key::InvalidIncomplete
        };
        Box::new(notice(
            page,
            &ctx.wiki,
            status,
            Key::RefusedInvalidHeading,
            vec![page.get(key)],
            vec![Choice {
                label: page.get(Key::ProtectBack),
                href: paths::front(ctx.slug()),
            }],
            None,
        ))
    })
}

/// What the fields say, or the problems with them (every one, at once).
fn setting_of(t: T, typed: &Typed, limit: u32) -> Result<(Target, Option<Setting>), Vec<String>> {
    let mut problems = Vec::new();
    let target = if typed.namespace {
        Target::namespace(&typed.title)
    } else {
        Target::path(&typed.title)
    };
    if target.is_none() {
        problems.push(t.get(Key::ProtectProblemTitle));
    }
    let require = match (typed.review, typed.role) {
        (Some(true), _) => Some(Require::Review),
        (Some(false), Some(role)) => Some(Require::Rung(role)),
        (Some(false), None) => {
            problems.push(t.get(Key::ProtectProblemRung));
            None
        }
        (None, _) => {
            problems.push(t.get(Key::ProtectProblemRequire));
            None
        }
    };
    if !inklkunn_core::governance::is_valid_reason(&typed.reason) {
        problems.push(
            t.msg(Key::ProtectProblemReason)
                .with("limit", inklkunn_core::governance::MAX_REASON_CHARS)
                .text(),
        );
    }
    let days = typed.days.trim().parse::<u32>().ok().filter(|d| *d >= 1);
    match days {
        None => problems.push(t.msg(Key::ProtectProblemDays).with("limit", limit).text()),
        Some(days) if days > limit => problems.push(
            t.msg(Key::ProtectProblemTooLong)
                .with("limit", limit)
                .text(),
        ),
        Some(_) => {}
    }
    match (target, require, days) {
        (Some(target), Some(require), Some(days)) if problems.is_empty() => {
            let setting = Setting {
                target: target.clone(),
                require,
                reason: typed.reason.clone(),
                days,
            };
            Ok((target, Some(setting)))
        }
        (Some(target), _, _) if problems.is_empty() => Ok((target, None)),
        _ => Err(problems),
    }
}

/// `POST /{slug}/-/protect`.
pub(crate) async fn protect_post(
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
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !ctx
            .services
            .authorizer
            .may_moderate(&ctx.principal, &ctx.wiki.id)
        {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let field = |name: &str| given.get(name).map_or("", String::as_str);
        let typed = Typed {
            namespace: field(fields::TARGET) == fields::TARGET_NAMESPACE,
            title: field(fields::TITLE).trim().to_owned(),
            review: match field(fields::REQUIRE) {
                fields::REQUIRE_REVIEW => Some(true),
                fields::REQUIRE_RUNG => Some(false),
                _ => None,
            },
            role: Role::parse(field(fields::ROLE)),
            reason: field(fields::REASON).trim().to_owned(),
            days: field(fields::DAYS).to_owned(),
        };
        let act = field(fields::ACT).to_owned();
        act_on(page, &ctx, &typed, &act)
    })
    .await
}

/// Do what the button says, and answer.
fn act_on(page: &Page, ctx: &Ctx, typed: &Typed, act: &str) -> Response {
    let t = page.t();
    let authorizer = &ctx.services.authorizer;
    let wiki = &ctx.wiki.id;
    let limit = authorizer.protection_limit_days().unwrap_or(0);
    let again =
        |problems: Vec<String>, status: StatusCode| form_page(page, ctx, typed, problems, status);
    let result = if act == fields::ACT_LIFT {
        // Lifting needs the title and, if one is given, a reason that can be shown; nothing else of the fields.
        let target = if typed.namespace {
            Target::namespace(&typed.title)
        } else {
            Target::path(&typed.title)
        };
        let Some(target) = target else {
            return again(
                vec![t.get(Key::ProtectProblemTitle)],
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        };
        let reason = (!typed.reason.is_empty()).then_some(typed.reason.as_str());
        authorizer.lift_protection(&ctx.principal, wiki, &target, reason)
    } else if act == fields::ACT_SET || act == fields::ACT_RENEW {
        let setting = match setting_of(t, typed, limit) {
            Ok((_, Some(setting))) => setting,
            Ok((_, None)) => return again(Vec::new(), StatusCode::UNPROCESSABLE_ENTITY),
            Err(problems) => return again(problems, StatusCode::UNPROCESSABLE_ENTITY),
        };
        if act == fields::ACT_SET {
            authorizer.protect(&ctx.principal, wiki, &setting)
        } else {
            authorizer.renew_protection(&ctx.principal, wiki, &setting)
        }
    } else {
        return again(
            vec![t.get(Key::ProtectProblemRequire)],
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    };
    match result {
        Ok(_) => see(&paths::form_for(ctx.slug(), typed.namespace, &typed.title)),
        Err(ProtectError::NotAllowed) => outcome_page(page, ctx, &Outcome::NotAllowed),
        Err(ProtectError::NotKept) => outcome_page(page, ctx, &Outcome::NotKept),
        Err(ProtectError::BadTarget) => again(
            vec![t.get(Key::ProtectProblemTitle)],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ProtectError::BadReason) => again(
            vec![
                t.msg(Key::ProtectProblemReason)
                    .with("limit", inklkunn_core::governance::MAX_REASON_CHARS)
                    .text(),
            ],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ProtectError::NoDays) => again(
            vec![t.msg(Key::ProtectProblemDays).with("limit", limit).text()],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ProtectError::TooLong { limit_days }) => again(
            vec![
                t.msg(Key::ProtectProblemTooLong)
                    .with("limit", limit_days)
                    .text(),
            ],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ProtectError::AlreadyInForce) => again(
            vec![t.get(Key::ProtectProblemInForce)],
            StatusCode::CONFLICT,
        ),
        Err(ProtectError::NotInForce) => again(
            vec![t.get(Key::ProtectProblemNotInForce)],
            StatusCode::CONFLICT,
        ),
        Err(ProtectError::Changed) => again(
            vec![t.get(Key::ProtectProblemChanged)],
            StatusCode::CONFLICT,
        ),
        Err(ProtectError::Store(why)) => {
            ctx.log(&format!("a protection could not be recorded: {why}"));
            outcome_page(page, ctx, &Outcome::Unavailable)
        }
    }
}
