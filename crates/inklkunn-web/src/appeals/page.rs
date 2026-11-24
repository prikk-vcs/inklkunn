//! The appeals page of the people who decide them, and the decision (RFC 006 D-9; handoff G-T5): the open appeals oldest first and
//! the decided ones newest first, each with what the appellant said, who they are, what the appeal is against and how it ended,
//! and for each open one the form that decides it. **Read by the rung that decides and by no one else**: to everyone else the page
//! is *not allowed*, and to a person who may not read the wiki it is not there.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::appeal::{AppealRow, AppealsView, Outcome, Queued};
use inklkunn_store::AppealId;

use super::paths::{self, fields};
use super::words;
use crate::catalog::{Key, Rich, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, from_this_site, see, serve};
use crate::time::utc_minutes;
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// The form that decides one open appeal.
struct DecideForm {
    action: String,
    appeal_field: &'static str,
    appeal: String,
    decision_field: &'static str,
    legend: String,
    upheld_value: &'static str,
    upheld_label: String,
    overturned_value: &'static str,
    overturned_label: String,
    reason_field: &'static str,
    reason_label: String,
    reason_hint: String,
    submit: String,
    id: String,
}

struct RowView {
    head: String,
    facts: Vec<String>,
    said_label: String,
    said: Vec<String>,
    proposal: Option<LinkView>,
    decision: Option<String>,
    /// The rung and the rules the decision was made under, the rules' change linked.
    decision_authority: Option<Rich>,
    blocked: Option<String>,
    form: Option<DecideForm>,
}

#[derive(Template)]
#[template(path = "appeals.html")]
struct PageTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    note: Option<String>,
    open_heading: String,
    open_none: String,
    open: Vec<RowView>,
    open_more: Option<String>,
    decided_heading: String,
    decided_none: String,
    decided: Vec<RowView>,
    mine: LinkView,
}

fn facts(t: T, row: &AppealRow) -> Vec<String> {
    let mut out = vec![
        t.msg(Key::AppealsFiledBy)
            .with("person", row.appellant_name.as_str())
            .with("date", utc_minutes(row.at))
            .text(),
    ];
    out.push(match &row.action_by {
        Some(name) => t
            .msg(Key::AppealsActionBy)
            .with("person", name.as_str())
            .text(),
        None => t.get(Key::AppealsActionByOperator),
    });
    out
}

fn row_view(t: T, slug: &str, row: &AppealRow, queued: Option<&Queued>) -> RowView {
    let proposal = row.proposal.as_ref().map(|p| LinkView {
        href: crate::proposals::paths::proposal(slug, p.as_str()),
        label: t.get(Key::ContestProposalLink),
    });
    let blocked = queued
        .and_then(|q| q.blocked)
        .map(|why| words::decider_refusal(t, why));
    let form = (queued.is_some() && blocked.is_none()).then(|| DecideForm {
        action: paths::decide(slug),
        appeal_field: fields::APPEAL,
        appeal: row.id.to_string(),
        decision_field: fields::DECISION,
        legend: t.get(Key::AppealsDecideLegend),
        upheld_value: fields::UPHELD,
        upheld_label: t.get(Key::AppealsDecideUpheld),
        overturned_value: fields::OVERTURNED,
        overturned_label: t.get(Key::AppealsDecideOverturned),
        reason_field: fields::REASON,
        reason_label: t.get(Key::AppealsDecideReason),
        reason_hint: t
            .msg(Key::PeopleReasonHint)
            .with("limit", inklkunn_core::people::MAX_REASON_CHARS)
            .text(),
        submit: t.get(Key::AppealsDecideSubmit),
        id: format!("appeal-{}", row.id),
    });
    RowView {
        head: t
            .msg(Key::AppealsRowHead)
            .with("person", row.appellant_name.as_str())
            .with("action", words::kind_label(t, row.action.kind))
            .text(),
        facts: facts(t, row),
        said_label: t.get(Key::AppealsSaidLabel),
        said: words::text_lines(&row.text),
        proposal,
        decision: row.decision.as_ref().map(|d| words::decision_line(t, d)),
        decision_authority: row
            .decision
            .as_ref()
            .map(|d| words::decision_authority(t, slug, d)),
        blocked,
        form,
    }
}

/// The note of what was just done, from the address's query (a fixed set of words).
fn note_of(t: T, params: &HashMap<String, String>) -> Option<String> {
    let done = params.get(fields::DONE)?;
    let mut sentences = vec![t.get(match done.as_str() {
        fields::DONE_UPHELD => Key::AppealsDoneUpheld,
        fields::DONE_OVERTURNED | fields::DONE_REOPENED => Key::AppealsDoneOverturned,
        _ => return None,
    })];
    if done == fields::DONE_REOPENED {
        sentences.push(t.get(Key::AppealsDoneReopened));
    }
    Some(sentences.join(" "))
}

fn back_choice(t: T, slug: &str) -> Vec<Choice> {
    vec![Choice {
        label: t.get(Key::AppealsBack),
        href: paths::page(slug),
    }]
}

/// `GET /{slug}/-/appeals`.
pub(crate) async fn appeals_page(
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
        let t = page.t();
        let view: AppealsView = match services.appeals.appeals(&ctx.principal, &ctx.wiki) {
            Ok(view) => view,
            Err(error) => {
                let back = vec![Choice {
                    label: t.get(Key::AppealsMineLink),
                    href: paths::mine(ctx.slug()),
                }];
                return super::refused(page, &ctx, &error, back);
            }
        };
        let slug = ctx.slug();
        let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
        let shell = Shell::of(&ctx.wiki);
        let rendered = PageTpl {
            page: &head,
            shell: &shell,
            title: t
                .msg(Key::TitleAppeals)
                .with("wiki", ctx.wiki.display_name.as_str())
                .product()
                .text(),
            heading: t.get(Key::AppealsHeading),
            intro: t.get(Key::AppealsIntro),
            note: note_of(t, &params),
            open_heading: t.get(Key::AppealsOpenHeading),
            open_none: t.get(Key::AppealsOpenNone),
            open: view
                .open
                .iter()
                .map(|q| row_view(t, slug, &q.row, Some(q)))
                .collect(),
            open_more: (view.more_open > 0)
                .then(|| t.msg(Key::AppealsOpenMore).n(view.more_open).text()),
            decided_heading: t.get(Key::AppealsDecidedHeading),
            decided_none: t.get(Key::AppealsDecidedNone),
            decided: view
                .decided
                .iter()
                .map(|row| row_view(t, slug, row, None))
                .collect(),
            mine: LinkView {
                href: paths::mine(slug),
                label: t.get(Key::AppealsMineLink),
            },
        }
        .render();
        answer(StatusCode::OK, &head, rendered)
    })
    .await
}

/// `POST /{slug}/-/appeals/decide`.
pub(crate) async fn decide_post(
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
                    back_choice(t, ctx.slug()),
                    None,
                );
            }
        };
        let field = |name: &str| given.get(name).cloned().unwrap_or_default();
        // An id that is no appeal's is answered as one that is not there: to a person who does not decide appeals that is *not
        // allowed* either way, so the answer says nothing of which appeals exist.
        let appeal = AppealId::parse(field(fields::APPEAL).trim()).unwrap_or_default();
        let outcome = match field(fields::DECISION).as_str() {
            fields::UPHELD => Outcome::Upheld,
            fields::OVERTURNED => Outcome::Overturned,
            _ => {
                return crate::wiki::notice(
                    page,
                    &ctx.wiki,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Key::RefusedInvalidHeading,
                    vec![t.get(Key::AppealsProblemDecision)],
                    back_choice(t, ctx.slug()),
                    None,
                );
            }
        };
        match services.appeals.decide(
            &ctx.principal,
            &ctx.wiki,
            &appeal,
            outcome,
            &field(fields::REASON),
        ) {
            Ok(made) => see(&paths::page_done(
                ctx.slug(),
                match (made.outcome, made.reopened.is_some()) {
                    (Outcome::Upheld, _) => fields::DONE_UPHELD,
                    (Outcome::Overturned, false) => fields::DONE_OVERTURNED,
                    (Outcome::Overturned, true) => fields::DONE_REOPENED,
                },
            )),
            Err(error) => super::refused(page, &ctx, &error, back_choice(t, ctx.slug())),
        }
    })
    .await
}
