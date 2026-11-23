//! The four forms of the people pages: change a role (a grant, or its revocation), restrict a person, renew a
//! restriction, lift one (RFC 006 D-6, D-7; handoff G-T2). No script.
//!
//! **The order of a request is the order of every write form.** A form from this site, the CSRF token of the session
//! (decided once by the layer), a wiki the visitor may read (an absent one and a forbidden one are one page), a person with
//! an account, whether they may use the form at all, and only then are the fields read. The act itself is the core's, which
//! authorizes it again, applies the rule, and records it: this module holds no key, mints no principal and writes no
//! record (INV-4).
//!
//! **A person who is held down by a restriction is told so, with its reason and its end**, instead of *not allowed*, on
//! every one of these forms: it is the same sentence every refusal of the wiki gives them.
//!
//! A problem with a field shows the form again with what was typed; every other end is a page of its own.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::RestrictionNotice;
use inklkunn_core::people::{PeopleError, PeopleView};
use inklkunn_store::{Role, WikiRecord};

use super::{fields, paths, sentences};
use crate::catalog::Key;
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, from_this_site, needs_account, see, serve};
use crate::wiki::{Choice, Shell, generic_not_found, notice, services_of, with_retry_after};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, axum::extract::rejection::QueryRejection>;

/// Which of the four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Grant,
    Restrict,
    Renew,
    Lift,
}

impl Act {
    fn title(self) -> Key {
        match self {
            Act::Grant => Key::TitlePeopleGrant,
            Act::Restrict => Key::TitlePeopleRestrict,
            Act::Renew => Key::TitlePeopleRenew,
            Act::Lift => Key::TitlePeopleLift,
        }
    }

    fn heading(self) -> Key {
        match self {
            Act::Grant => Key::PeopleGrantHeading,
            Act::Restrict => Key::PeopleRestrictHeading,
            Act::Renew => Key::PeopleRenewHeading,
            Act::Lift => Key::PeopleLiftHeading,
        }
    }

    fn intro(self) -> Key {
        match self {
            Act::Grant => Key::PeopleGrantIntro,
            Act::Restrict => Key::PeopleRestrictIntro,
            Act::Renew => Key::PeopleRenewIntro,
            Act::Lift => Key::PeopleLiftIntro,
        }
    }

    fn submit(self) -> Key {
        match self {
            Act::Grant => Key::PeopleGrantSubmit,
            Act::Restrict => Key::PeopleRestrictSubmit,
            Act::Renew => Key::PeopleRenewSubmit,
            Act::Lift => Key::PeopleLiftSubmit,
        }
    }

    fn action(self, slug: &str) -> String {
        match self {
            Act::Grant => paths::grant(slug, None),
            Act::Restrict => paths::restrict(slug, None),
            Act::Renew => paths::renew(slug, None),
            Act::Lift => paths::lift(slug, None),
        }
    }

    /// Has the form a number of days?
    fn has_days(self) -> bool {
        matches!(self, Act::Restrict | Act::Renew)
    }

    /// Is the reason required (a restriction is told its reason) or optional?
    fn reason_required(self) -> bool {
        self.has_days()
    }
}

/// What the form asks of the person it is about.
struct PersonField {
    name: String,
    id: String,
}

/// One role to choose, with what the wiki's rules say it looks for.
struct RoleOption {
    value: String,
    label: String,
    note: Option<String>,
    checked: bool,
}

struct RolesField {
    legend: String,
    note: String,
    options: Vec<RoleOption>,
}

#[derive(Template)]
#[template(path = "people_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    problems: Vec<String>,
    action: String,
    person: Option<PersonField>,
    who_field: &'static str,
    who_label: String,
    who_hint: String,
    who: String,
    roles: Option<RolesField>,
    role_field: &'static str,
    days_label: String,
    days_hint: String,
    days_field: &'static str,
    days: Option<String>,
    reason_label: String,
    reason_hint: String,
    reason_field: &'static str,
    reason: String,
    submit: String,
    back: LinkView,
}

/// What was typed, kept to show the form again.
#[derive(Default, Clone)]
struct Typed {
    who: String,
    role: Option<String>,
    days: String,
    reason: String,
}

/// The name the people page shows for the account `id`, if the page shows it **to this person**: a form opens on a person
/// the person who uses it can already see on the page, so that an id in a link tells no one who has an account.
fn known(view: &PeopleView, id: &str) -> Option<String> {
    let named = view.rungs.iter().find_map(|row| match &row.holders {
        inklkunn_core::people::Holders::Named(people) => people
            .iter()
            .find(|p| p.account.as_str() == id)
            .map(|p| p.name.clone()),
        inklkunn_core::people::Holders::Count(_) => None,
    });
    named
        .or_else(|| {
            view.restricted
                .iter()
                .find(|r| r.account.as_str() == id)
                .map(|r| r.name.clone())
        })
        .map(|name| name.unwrap_or_default())
}

fn roles_field(page: &Page, view: &PeopleView, typed: &Typed) -> Option<RolesField> {
    let t = page.t();
    let up_to = view.may_grant_up_to?;
    let promotion = |role: Role| {
        view.rungs
            .iter()
            .find(|row| row.rung == role)
            .and_then(|row| row.promotion.clone())
            .map(|text| t.msg(Key::PeoplePromotion).with("text", text).text())
    };
    let mut options: Vec<RoleOption> = Role::ALL
        .into_iter()
        .filter(|role| *role <= up_to)
        .map(|role| RoleOption {
            value: role.as_str().to_owned(),
            label: sentences::role_label(t, role),
            note: promotion(role),
            checked: typed.role.as_deref() == Some(role.as_str()),
        })
        .collect();
    options.push(RoleOption {
        value: fields::NO_ROLE.to_owned(),
        label: t.get(Key::PeopleGrantNone),
        note: None,
        checked: typed.role.as_deref() == Some(fields::NO_ROLE),
    });
    Some(RolesField {
        legend: t.get(Key::PeopleGrantRoleLegend),
        note: t
            .msg(Key::PeopleGrantReach)
            .with("role", sentences::role_label(t, up_to))
            .text(),
        options,
    })
}

#[allow(clippy::too_many_arguments)]
fn form_page(
    page: &Page,
    wiki: &WikiRecord,
    act: Act,
    view: &PeopleView,
    typed: &Typed,
    person: Option<PersonField>,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let head = page.wiki_head(wiki, Subject::Wiki);
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(act.title())
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: head.get(act.heading()),
        intro: head.get(act.intro()),
        problems,
        action: act.action(slug),
        person,
        who_field: fields::WHO,
        who_label: head.get(Key::PeopleWhoLabel),
        who_hint: head.get(Key::PeopleWhoHint),
        who: typed.who.clone(),
        roles: (act == Act::Grant)
            .then(|| roles_field(&head, view, typed))
            .flatten(),
        role_field: fields::ROLE,
        days_label: head.get(Key::PeopleDaysLabel),
        days_hint: t
            .msg(Key::PeopleDaysHint)
            .with("limit", view.limit_days)
            .text(),
        days_field: fields::DAYS,
        days: act.has_days().then(|| typed.days.clone()),
        reason_label: head.get(if act.reason_required() {
            Key::PeopleReasonLabelRequired
        } else {
            Key::PeopleReasonLabel
        }),
        reason_hint: t
            .msg(Key::PeopleReasonHint)
            .with("limit", inklkunn_core::people::MAX_REASON_CHARS)
            .text(),
        reason_field: fields::REASON,
        reason: typed.reason.clone(),
        submit: head.get(act.submit()),
        back: LinkView {
            href: paths::page(slug),
            label: head.get(Key::PeopleBack),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// A page of its own for a refusal that is not about a field: a heading and sentences, with the way back to the people.
fn refusal_page(
    page: &Page,
    ctx: &Ctx,
    status: StatusCode,
    heading: Key,
    paragraphs: Vec<String>,
) -> Response {
    let t = page.t();
    notice(
        page,
        &ctx.wiki,
        status,
        heading,
        paragraphs,
        vec![Choice {
            label: t.get(Key::PeopleBack),
            href: paths::page(ctx.slug()),
        }],
        None,
    )
}

/// The page for a refusal that is not about a field.
fn ended(page: &Page, ctx: &Ctx, error: &PeopleError) -> Response {
    let t = page.t();
    match error {
        PeopleError::NotFound => generic_not_found(page),
        // The person may contest the restriction from the page that says so.
        PeopleError::Restricted(restriction) => crate::wiki::notice(
            page,
            &ctx.wiki,
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            sentences::restriction_paragraphs(t, restriction),
            vec![
                Choice {
                    label: t.get(Key::PeopleBack),
                    href: paths::page(ctx.slug()),
                },
                crate::appeals::words::restriction_choice(t, ctx.slug(), restriction),
            ],
            None,
        ),
        PeopleError::RulesUnreadable(fault) => refusal_page(
            page,
            ctx,
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeRulesUnreadable,
            crate::proposals::governance::unreadable_paragraphs(t, fault),
        ),
        PeopleError::Changed => refusal_page(
            page,
            ctx,
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![t.get(Key::PeopleChanged)],
        ),
        PeopleError::Unavailable(cause) => {
            ctx.log(&format!("people could not be changed now: {cause}"));
            with_retry_after(refusal_page(
                page,
                ctx,
                StatusCode::SERVICE_UNAVAILABLE,
                Key::OutcomeUnavailable,
                vec![t.get(Key::OutcomeNoteUnavailable)],
            ))
        }
        // Everything else is not allowed, or is about a field and was shown with the form.
        _ => refusal_page(
            page,
            ctx,
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            vec![t.get(Key::RefusedNotAllowedBody)],
        ),
    }
}

/// May the person use this form at all? `None`: they may. Else the page that says why not: the restriction when it is the
/// reason, *not allowed* when it is not.
fn cannot(page: &Page, ctx: &Ctx, act: Act, view: &PeopleView) -> Option<Response> {
    let (may, held_down): (bool, Option<&RestrictionNotice>) = match act {
        Act::Grant => (
            view.may_grant_up_to.is_some(),
            view.grant_held_down_by.as_ref(),
        ),
        Act::Restrict | Act::Renew | Act::Lift => {
            (view.may_restrict, view.restrict_held_down_by.as_ref())
        }
    };
    if may {
        return None;
    }
    Some(match held_down {
        Some(notice) => ended(page, ctx, &PeopleError::Restricted(notice.clone())),
        None => ended(page, ctx, &PeopleError::NotAllowed),
    })
}

/// The view, or the page that says it could not be had.
fn viewed(page: &Page, ctx: &Ctx) -> Result<PeopleView, Box<Response>> {
    ctx.services
        .people
        .view(&ctx.principal, &ctx.wiki)
        .map_err(|error| Box::new(ended(page, ctx, &error)))
}

async fn open(
    state: PageState,
    page: Page,
    headers: HeaderMap,
    path: PathName,
    params: Params,
    act: Act,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let account = params
        .ok()
        .and_then(|Query(mut p)| p.remove(fields::ACCOUNT));
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let view = match viewed(page, &ctx) {
            Ok(view) => view,
            Err(response) => return *response,
        };
        if let Some(response) = cannot(page, &ctx, act, &view) {
            return response;
        }
        let person = account.as_deref().and_then(|id| {
            known(&view, id).map(|name| PersonField {
                name,
                id: id.to_owned(),
            })
        });
        let typed = Typed {
            who: person.as_ref().map(|p| p.id.clone()).unwrap_or_default(),
            ..Typed::default()
        };
        form_page(
            page,
            &ctx.wiki,
            act,
            &view,
            &typed,
            person,
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
        Box::new(refusal_page(
            page,
            ctx,
            status,
            Key::RefusedInvalidHeading,
            vec![page.get(key)],
        ))
    })
}

async fn submit(
    state: PageState,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
    act: Act,
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
        let view = match viewed(page, &ctx) {
            Ok(view) => view,
            Err(response) => return *response,
        };
        if let Some(response) = cannot(page, &ctx, act, &view) {
            return response;
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let field = |name: &str| given.get(name).cloned().unwrap_or_default();
        let typed = Typed {
            who: field(fields::WHO),
            role: given.get(fields::ROLE).cloned(),
            days: field(fields::DAYS),
            reason: field(fields::REASON),
        };
        let t = page.t();
        let again = |problems: Vec<String>| {
            form_page(
                page,
                &ctx.wiki,
                act,
                &view,
                &typed,
                person_of(&view, &typed),
                problems,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        };
        let people = &ctx.services.people;
        let done = match act {
            Act::Grant => {
                let new = match typed.role.as_deref() {
                    Some(fields::NO_ROLE) => Some(None),
                    Some(word) => Role::parse(word).map(Some),
                    None => None,
                };
                let Some(new) = new else {
                    return again(vec![t.get(Key::PeopleProblemRoleMissing)]);
                };
                people
                    .grant(&ctx.principal, &ctx.wiki, &typed.who, new, &typed.reason)
                    .map(|made| {
                        if made.after.is_some() {
                            fields::DONE_GRANTED
                        } else {
                            fields::DONE_REVOKED
                        }
                    })
            }
            Act::Restrict | Act::Renew => {
                let Ok(days) = typed.days.trim().parse::<u32>() else {
                    return again(vec![
                        t.msg(Key::PeopleProblemDays)
                            .with("limit", view.limit_days)
                            .text(),
                    ]);
                };
                let made = if act == Act::Restrict {
                    people.restrict(&ctx.principal, &ctx.wiki, &typed.who, days, &typed.reason)
                } else {
                    people.renew(&ctx.principal, &ctx.wiki, &typed.who, days, &typed.reason)
                };
                made.map(|_| {
                    if act == Act::Restrict {
                        fields::DONE_RESTRICTED
                    } else {
                        fields::DONE_RENEWED
                    }
                })
            }
            Act::Lift => people
                .lift(&ctx.principal, &ctx.wiki, &typed.who, &typed.reason)
                .map(|_| fields::DONE_LIFTED),
        };
        match done {
            Ok(word) => see(&paths::done(ctx.slug(), word)),
            Err(error) => match sentences::problem(t, &error, view.may_grant_up_to) {
                Some(problem) => again(vec![problem]),
                None => ended(page, &ctx, &error),
            },
        }
    })
    .await
}

/// The person field for a form shown again: the same person, when the form was opened on one the page names.
fn person_of(view: &PeopleView, typed: &Typed) -> Option<PersonField> {
    known(view, typed.who.trim()).map(|name| PersonField {
        name,
        id: typed.who.trim().to_owned(),
    })
}

macro_rules! handlers {
    ($form:ident, $post:ident, $act:expr) => {
        /// The form's `GET`.
        pub(crate) async fn $form(
            State(state): State<PageState>,
            page: Page,
            headers: HeaderMap,
            path: PathName,
            params: Params,
        ) -> Response {
            open(state, page, headers, path, params, $act).await
        }

        /// The form's `POST`.
        pub(crate) async fn $post(
            State(state): State<PageState>,
            page: Page,
            headers: HeaderMap,
            decided: Decided,
            path: PathName,
            form: Raw,
        ) -> Response {
            submit(state, page, headers, decided, path, form, $act).await
        }
    };
}

handlers!(grant_form, grant_post, Act::Grant);
handlers!(restrict_form, restrict_post, Act::Restrict);
handlers!(renew_form, renew_post, Act::Renew);
handlers!(lift_form, lift_post, Act::Lift);
