//! Creating an alternative view (RFC 005 part 2 D-9; RFC 002 D-2's `create-view`; handoff 005 K-T8): a form with no
//! script, for a signed-in person whose role includes creating views (reviewers and above by default), and the
//! page that says how a creation ended, for each way it can.
//!
//! **The order of a request is the order of every write form.** A form from this site, the CSRF token of the
//! session (decided once by the layer), a wiki the visitor may read (an absent one and a forbidden one are one
//! page), a person with an account, `create-view` asked of the authorizer, and only then the fields are read. The
//! act itself goes through the acceptor, which authorizes again, records the act and asks the signer: this module
//! holds no key, mints no principal and writes no record (INV-4).
//!
//! The name is checked where the form is (so the person is told what is wrong with it) and again by the core,
//! which is the authority. A problem with a field shows the form again with what was typed; every other end is a
//! page of its own, in words that say whether anything was changed.

use askama::Template;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::views::{
    NameFault, ViewKind, ViewList, list as list_views, new_name, short_name,
};
use inklkunn_core::{Action, Resource, ViewFailure};
use inklkunn_store::WikiRecord;

use super::{fields, named, paths};
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, SMALL_FORM_LIMIT, from_this_site, see, serve};
use crate::wiki::{
    Choice, Shell, Shown, generic_not_found, notice, present, services_of, with_retry_after,
};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;

// ------------------------------------------------------------------------------------ how a creation ended

/// How a creation ended when it did not end in the new view: the fixed cases the words are written for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewOutcome {
    /// This server does not keep what creating a view needs (its records, or its build).
    NotKept,
    /// The server was started without the means to sign.
    SigningNotConfigured,
    /// *Not allowed.*
    NotAllowed,
    /// *The wiki is busy.*
    Busy,
    /// *The wiki needs recovery.*
    NeedsRecovery,
    /// The wiki already has as many open views as it keeps.
    TooMany {
        /// The bound.
        bound: usize,
    },
    /// A change to the wiki's keys is not finished.
    RetirementUnfinished,
    /// The history did not accept the view; nothing was changed.
    Failed,
    /// The repository or the records could not be had before anything was changed.
    Unavailable,
    /// The view may or may not have been created.
    Uncertain,
}

/// The page that says how a creation ended, in the visitor's language (nothing was created, or the result is not
/// certain, and which).
pub fn render_view_outcome(page: &Page, wiki: &WikiRecord, outcome: &ViewOutcome) -> Response {
    let t = page.t();
    let (status, heading, paragraphs) = match outcome {
        ViewOutcome::NotKept => (
            StatusCode::NOT_IMPLEMENTED,
            Key::ViewsNewNotKeptHeading,
            vec![t.get(Key::ViewsNewNotKept)],
        ),
        ViewOutcome::SigningNotConfigured => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::ViewsNewUnconfiguredHeading,
            vec![t.get(Key::ViewsNewUnconfigured)],
        ),
        ViewOutcome::NotAllowed => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            vec![t.get(Key::ViewsNewNotAllowed)],
        ),
        ViewOutcome::Busy => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeBusy,
            vec![t.get(Key::ViewsNewBusy)],
        ),
        ViewOutcome::NeedsRecovery => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeRecovery,
            vec![t.get(Key::OutcomeNoteRecovery)],
        ),
        ViewOutcome::TooMany { bound } => (
            StatusCode::CONFLICT,
            Key::ViewsNewTooManyHeading,
            vec![t.msg(Key::ViewsNewTooMany).with("bound", *bound).text()],
        ),
        ViewOutcome::RetirementUnfinished => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::ViewsNewRetirementHeading,
            vec![t.get(Key::ViewsNewRetirement)],
        ),
        ViewOutcome::Failed => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::ViewsNewFailedHeading,
            vec![t.get(Key::ViewsNewFailed)],
        ),
        ViewOutcome::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeUnavailable,
            vec![t.get(Key::OutcomeNoteUnavailable)],
        ),
        ViewOutcome::Uncertain => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Key::OutcomeStore,
            vec![t.get(Key::ViewsNewUncertain)],
        ),
    };
    let response = notice(
        page,
        wiki,
        status,
        heading,
        paragraphs,
        vec![Choice {
            label: t.get(Key::CompareBack),
            href: paths::list(wiki.slug.as_str()),
        }],
        None,
    );
    match outcome {
        ViewOutcome::Busy
        | ViewOutcome::NeedsRecovery
        | ViewOutcome::Unavailable
        | ViewOutcome::SigningNotConfigured => with_retry_after(response),
        _ => response,
    }
}

// ------------------------------------------------------------------------------------ the form

struct KindItem {
    value: &'static str,
    label: String,
    note: String,
    checked: bool,
}

struct Option_ {
    value: String,
    label: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "view_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    problems: Vec<String>,
    action: String,
    name_field: &'static str,
    kind_field: &'static str,
    from_field: &'static str,
    name_label: String,
    name_hint: String,
    name: String,
    kind_legend: String,
    kinds: Vec<KindItem>,
    from_label: String,
    options: Vec<Option_>,
    submit: String,
    back: LinkView,
}

/// What was typed, kept to show the form again.
#[derive(Default)]
struct Typed {
    name: String,
    kind: Option<ViewKind>,
    from: String,
}

fn form_page(
    page: &Page,
    wiki: &WikiRecord,
    list: &ViewList,
    typed: &Typed,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let head = page.wiki_head(wiki, Subject::Wiki);
    let default = short_name(&wiki.default_view);
    let chosen = if typed.from.is_empty() {
        default
    } else {
        typed.from.as_str()
    };
    let kinds = [
        (
            ViewKind::Experimental,
            Key::ViewsKindExperimental,
            Key::ViewsKindExperimentalNote,
        ),
        (
            ViewKind::Interpretation,
            Key::ViewsKindInterpretation,
            Key::ViewsKindInterpretationNote,
        ),
    ]
    .map(|(kind, label, note)| KindItem {
        value: kind.as_str(),
        label: t.get(label),
        note: t.get(note),
        checked: typed.kind == Some(kind),
    })
    .into();
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleViewsNew)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: head.get(Key::ViewsNewHeading),
        intro: head.get(Key::ViewsNewIntro),
        problems,
        action: paths::new(slug),
        name_field: fields::NAME,
        kind_field: fields::KIND,
        from_field: fields::FROM,
        name_label: head.get(Key::ViewsNewNameLabel),
        name_hint: head.get(Key::ViewsNewNameHint),
        name: typed.name.clone(),
        kind_legend: head.get(Key::ViewsNewKindLegend),
        kinds,
        from_label: head.get(Key::ViewsNewFromLabel),
        options: list
            .rows
            .iter()
            .map(|row| Option_ {
                value: row.name.clone(),
                label: if row.is_default {
                    named(t, wiki, &row.name)
                } else {
                    row.name.clone()
                },
                selected: row.name == chosen,
            })
            .collect(),
        submit: head.get(Key::ViewsNewSubmit),
        back: LinkView {
            href: paths::list(slug),
            label: head.get(Key::CompareBack),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// May the visitor create views in this wiki? Asked of the authorizer, as the act will be.
fn may_create(ctx: &Ctx) -> bool {
    ctx.may(
        Action::CreateView,
        &Resource::Wiki {
            wiki: ctx.wiki.id.clone(),
        },
    )
}

/// The reason, if there is one, that this server cannot create a view at all, before anyone is asked anything.
fn cannot(ctx: &Ctx) -> Option<ViewOutcome> {
    let Some(acceptor) = &ctx.services.acceptor else {
        return Some(ViewOutcome::NotKept);
    };
    if !acceptor.is_configured() {
        return Some(ViewOutcome::SigningNotConfigured);
    }
    if !ctx.services.acts.keeps_view_acts() {
        return Some(ViewOutcome::NotKept);
    }
    None
}

/// The visitor holds no account: said in words, with the way back to the views.
fn needs_account(page: &Page, ctx: &Ctx) -> Option<Response> {
    if ctx.account().is_some() {
        return None;
    }
    let t = page.t();
    Some(notice(
        page,
        &ctx.wiki,
        StatusCode::FORBIDDEN,
        Key::NeedsAccountHeading,
        vec![t.get(Key::NeedsAccountBody), t.get(Key::NeedsAccountAdvice)],
        vec![Choice {
            label: t.get(Key::CompareBack),
            href: paths::list(ctx.slug()),
        }],
        None,
    ))
}

/// The views, for the choice of the one to fork from; or the page that says they could not be read now.
fn views_of(ctx: &Ctx, page: &Page) -> Result<ViewList, Box<Response>> {
    list_views(&ctx.services.read, &*ctx.services.acts, &ctx.wiki).map_err(|e| {
        ctx.log(&format!("the views could not be read now: {e}"));
        Box::new(present(
            page,
            Shown::Unavailable {
                wiki: Box::new(ctx.wiki.clone()),
            },
        ))
    })
}

/// `GET /{slug}/-/views/new`.
pub(crate) async fn new_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: PathName,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !may_create(&ctx) {
            return render_view_outcome(page, &ctx.wiki, &ViewOutcome::NotAllowed);
        }
        if let Some(outcome) = cannot(&ctx) {
            return render_view_outcome(page, &ctx.wiki, &outcome);
        }
        match views_of(&ctx, page) {
            Ok(list) => form_page(
                page,
                &ctx.wiki,
                &list,
                &Typed::default(),
                Vec::new(),
                StatusCode::OK,
            ),
            Err(response) => *response,
        }
    })
    .await
}

/// The form's fields, parsed now (after the checks that come first), or the page that says the form was not one.
fn form_fields(
    page: &Page,
    ctx: &Ctx,
    headers: &HeaderMap,
    form: Raw,
) -> Result<std::collections::HashMap<String, String>, Box<Response>> {
    match crate::proposals::urlencoded::fields(headers, form) {
        Ok(fields) => Ok(fields),
        Err(status) => {
            let key = if status == StatusCode::PAYLOAD_TOO_LARGE {
                Key::InvalidFormTooLarge
            } else {
                Key::InvalidIncomplete
            };
            Err(Box::new(notice(
                page,
                &ctx.wiki,
                status,
                Key::RefusedInvalidHeading,
                vec![page.get(key)],
                vec![Choice {
                    label: page.get(Key::CompareBack),
                    href: paths::list(ctx.slug()),
                }],
                None,
            )))
        }
    }
}

/// The sentence for a name that is refused.
fn name_problem(t: T, why: NameFault) -> String {
    t.get(match why {
        NameFault::Empty => Key::ViewsNewProblemNameEmpty,
        NameFault::Start => Key::ViewsNewProblemNameStart,
        NameFault::Characters => Key::ViewsNewProblemNameChars,
        NameFault::TooLong => Key::ViewsNewProblemNameLong,
        NameFault::Retired => Key::ViewsNewProblemNameRetired,
    })
}

/// `POST /{slug}/-/views/new`.
pub(crate) async fn new_post(
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
        if !may_create(&ctx) {
            return render_view_outcome(page, &ctx.wiki, &ViewOutcome::NotAllowed);
        }
        if let Some(outcome) = cannot(&ctx) {
            return render_view_outcome(page, &ctx.wiki, &outcome);
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let field = |name: &str| given.get(name).map_or("", String::as_str);
        let typed = Typed {
            name: field(fields::NAME).to_owned(),
            kind: ViewKind::parse(field(fields::KIND)),
            from: field(fields::FROM).to_owned(),
        };
        let list = match views_of(&ctx, page) {
            Ok(list) => list,
            Err(response) => return *response,
        };
        let t = page.t();
        let mut problems = Vec::new();
        if let Err(why) = new_name(&typed.name) {
            problems.push(name_problem(t, why));
        }
        if typed.kind.is_none() {
            problems.push(t.get(Key::ViewsNewProblemKind));
        }
        if !typed.from.is_empty() && list.view(&typed.from).is_none() {
            problems.push(t.get(Key::ViewsNewProblemFrom));
        }
        let Some(kind) = typed.kind.filter(|_| problems.is_empty()) else {
            return form_page(
                page,
                &ctx.wiki,
                &list,
                &typed,
                problems,
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        };
        let Some(acceptor) = ctx.services.acceptor.clone() else {
            return render_view_outcome(page, &ctx.wiki, &ViewOutcome::NotKept);
        };
        let from = (!typed.from.is_empty()).then_some(typed.from.as_str());
        match acceptor.create_view(&ctx.principal, &ctx.wiki, &typed.name, from, kind) {
            Ok(created) => {
                if created.seeds_left {
                    ctx.log("a view was created and a temporary directory of seeds could not be removed");
                }
                see(&paths::list_at(ctx.slug(), &created.name))
            }
            Err(failure) => ended(page, &ctx, &list, &typed, &failure),
        }
    })
    .await
}

/// The page for a creation that did not create the view: the form again for a field's problem, else the outcome.
fn ended(
    page: &Page,
    ctx: &Ctx,
    list: &ViewList,
    typed: &Typed,
    failure: &ViewFailure,
) -> Response {
    let t = page.t();
    let again = |key: Key, status: StatusCode| {
        form_page(page, &ctx.wiki, list, typed, vec![t.get(key)], status)
    };
    let outcome = |o: ViewOutcome| render_view_outcome(page, &ctx.wiki, &o);
    match failure {
        ViewFailure::InvalidName(why) => form_page(
            page,
            &ctx.wiki,
            list,
            typed,
            vec![name_problem(t, *why)],
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ViewFailure::NoSuchSource => {
            again(Key::ViewsNewProblemFrom, StatusCode::UNPROCESSABLE_ENTITY)
        }
        ViewFailure::NameTaken => again(Key::ViewsNewProblemTaken, StatusCode::CONFLICT),
        ViewFailure::NotAllowed => outcome(ViewOutcome::NotAllowed),
        ViewFailure::NotKept => outcome(ViewOutcome::NotKept),
        ViewFailure::TooMany { bound } => outcome(ViewOutcome::TooMany { bound: *bound }),
        ViewFailure::Busy(_) => outcome(ViewOutcome::Busy),
        ViewFailure::NeedsRecovery(flag) => {
            ctx.log(&format!(
                "a view was not created: the wiki needs recovery ({})",
                flag.reason.as_str()
            ));
            outcome(ViewOutcome::NeedsRecovery)
        }
        ViewFailure::RetirementUnfinished(why) => {
            ctx.log(&format!("a view was not created: {why}"));
            outcome(ViewOutcome::RetirementUnfinished)
        }
        ViewFailure::Failed { act, word, detail } => {
            ctx.log(&format!(
                "creating a view (act {act}) failed ({word}): {detail}"
            ));
            outcome(if *word == inklkunn_core::acceptance::words::UNAVAILABLE {
                ViewOutcome::Unavailable
            } else {
                ViewOutcome::Failed
            })
        }
        ViewFailure::Unavailable(why) => {
            ctx.log(&format!("a view could not be created now: {why}"));
            outcome(ViewOutcome::Unavailable)
        }
        ViewFailure::Uncertain(why) => {
            ctx.log(&format!(
                "it is not known whether a view was created: {why}"
            ));
            outcome(ViewOutcome::Uncertain)
        }
        ViewFailure::Store(error) => {
            ctx.log(&format!(
                "the records failed while a view was created: {error}"
            ));
            outcome(ViewOutcome::Unavailable)
        }
    }
}

/// The body bound of the form: a name, a kind and a view.
pub(crate) const FORM_LIMIT: usize = SMALL_FORM_LIMIT;
