//! Review (RFC 004 D-3, D-8; WEB-04): a comment (on a part or a reply), a decision (*approve*, *request
//! changes*) bound to the revision that was reviewed, the thin forms that decline a proposal, take it back or
//! ask for a revision (each with a required reason, worded to assume good faith), and *Accept*.
//!
//! **Accept is not served yet** (handoff 004 T4): [`accept`] answers a page that says so, after the same checks
//! the acceptance will make first (a form from this site, an account, the `accept` action on the proposal, a
//! proposal that exists). It is the one function the acceptance orchestration replaces; the words for every
//! outcome an acceptance can have are already here ([`AcceptOutcome`], [`render_accept_outcome`]).

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{FormRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::proposal::{Invalid, ProposalError, QuotaRefusal};
use inklkunn_core::{Action, proposal::NewComment};
use inklkunn_store::{CommentId, DecisionKind, ProposalId, ProposalOp, ProposalState, WikiRecord};

use super::{
    Ctx, Stop, fields, fields_of, from_this_site, needs_account, parse_id, paths, refused, see,
    serve, words,
};
use crate::catalog::Key;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::wiki::{Choice, Shell, generic_not_found, notice, services_of, with_retry_after};

type Fields = Result<axum::extract::Form<HashMap<String, String>>, FormRejection>;
type Ids = Result<Path<(String, String)>, PathRejection>;

/// The way back to a proposal.
fn back(page: &Page, ctx: &Ctx, id: &ProposalId) -> Vec<Choice> {
    vec![Choice {
        label: page.get(Key::ProposalBack),
        href: paths::proposal(ctx.slug(), id.as_str()),
    }]
}

/// What every write on a proposal begins with: a form from this site, a wiki the visitor may read, a proposal
/// id, an account, and the form's fields. `Err` is the answer to give.
fn begin(
    state: &PageState,
    page: &Page,
    headers: &HeaderMap,
    ids: Ids,
    form: Fields,
) -> Result<Begun, Stop> {
    if let Some(response) = from_this_site(page, headers) {
        return Err(Box::new(response));
    }
    let (Ok(Path((slug, id))), Some(services)) = (ids, services_of(state)) else {
        return Err(Box::new(generic_not_found(page)));
    };
    Ok(Begun {
        services,
        slug,
        id,
        form,
    })
}

struct Begun {
    services: std::sync::Arc<crate::services::Services>,
    slug: String,
    id: String,
    form: Fields,
}

fn field<'a>(fields: &'a HashMap<String, String>, name: &str) -> &'a str {
    fields.get(name).map_or("", String::as_str)
}

// ------------------------------------------------------------------------------------ a comment

/// `POST /{slug}/-/proposals/{id}/comment`.
pub(crate) async fn comment(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
) -> Response {
    let begun = match begin(&state, &page, &headers, ids, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&begun.services, &headers, &begun.slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&begun.id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let fields = match fields_of(page, &ctx, begun.form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let new = match new_comment(&ctx, &id, &fields) {
            Ok(n) => n,
            Err(error) => return refused(page, &ctx, &error, back(page, &ctx, &id)),
        };
        match ctx
            .services
            .proposals
            .comment(&ctx.principal, &ctx.wiki, &id, new)
        {
            Ok(c) => see(&paths::proposal_at(
                ctx.slug(),
                id.as_str(),
                &format!("k-{}", c.id),
            )),
            Err(error) => refused(page, &ctx, &error, back(page, &ctx, &id)),
        }
    })
    .await
}

/// The comment a form says. An anchor is rebuilt here from the latest revision's own text, never taken from
/// the form: the form names the page and the position, and the revision it was drawn from, and the comment is
/// refused if the proposal has moved on.
fn new_comment(
    ctx: &Ctx,
    id: &ProposalId,
    fields: &HashMap<String, String>,
) -> Result<NewComment, ProposalError> {
    let mut new = NewComment {
        body: field(fields, fields::BODY).to_owned(),
        parent: fields
            .get(fields::PARENT)
            .filter(|p| !p.is_empty())
            .map(|p| CommentId::parse(p).ok_or(Invalid::BadAnchor))
            .transpose()?,
        anchor: None,
    };
    if let Some(page) = fields.get(fields::ANCHOR_PAGE).filter(|p| !p.is_empty()) {
        let ordinal = field(fields, fields::ANCHOR_ORDINAL).parse::<usize>();
        let seen = field(fields, fields::ANCHOR_REVISION).parse::<u32>();
        let (Ok(ordinal), Ok(seen)) = (ordinal, seen) else {
            return Err(Invalid::BadAnchor.into());
        };
        let latest = ctx
            .services
            .proposals
            .revisions(&ctx.principal, &ctx.wiki, id)?
            .pop()
            .ok_or(ProposalError::Changed)?;
        if latest.seq != seen {
            return Err(ProposalError::Changed);
        }
        let text = latest
            .content
            .files
            .iter()
            .find(|f| f.path == *page)
            .and_then(|f| f.text.as_deref())
            .ok_or(Invalid::BadAnchor)?;
        new.anchor = Some(super::anchor::make(page, text, ordinal).ok_or(Invalid::BadAnchor)?);
    }
    Ok(new)
}

// ------------------------------------------------------------------------------------ a decision

/// `POST /{slug}/-/proposals/{id}/decide`.
pub(crate) async fn decide(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
) -> Response {
    let begun = match begin(&state, &page, &headers, ids, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&begun.services, &headers, &begun.slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&begun.id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let fields = match fields_of(page, &ctx, begun.form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let kind = match field(&fields, fields::OP) {
            fields::OP_APPROVE => DecisionKind::Approve,
            fields::OP_REQUEST_CHANGES => DecisionKind::RequestChanges,
            _ => {
                return refused(
                    page,
                    &ctx,
                    &ProposalError::Invalid(Invalid::BadAnchor),
                    back(page, &ctx, &id),
                );
            }
        };
        let Ok(revision) = field(&fields, fields::REVISION).parse::<u32>() else {
            return refused(
                page,
                &ctx,
                &ProposalError::Invalid(Invalid::BadAnchor),
                back(page, &ctx, &id),
            );
        };
        match ctx.services.proposals.decide(
            &ctx.principal,
            &ctx.wiki,
            &id,
            revision,
            kind,
            field(&fields, fields::BODY),
        ) {
            Ok(_) => see(&paths::proposal_at(ctx.slug(), id.as_str(), "decisions")),
            Err(error) => refused(page, &ctx, &error, back(page, &ctx, &id)),
        }
    })
    .await
}

// ------------------------------------------------------------------------------------ accept

/// What an acceptance can come to (RFC 004 D-12), in the glossary's words. [`AcceptOutcome::NotYetAvailable`]
/// is what this release answers; the others are what the acceptance (handoff 004 T4) will answer with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    /// Accepting is not served by this build.
    NotYetAvailable,
    /// *Signed into the history.*
    Signed,
    /// *Stopped: changes overlap.*
    Overlap,
    /// *Not allowed.*
    NotAllowed,
    /// *The wiki is busy*, with how many acceptances are waiting before this one.
    Busy {
        /// The queue's depth.
        waiting: u64,
    },
    /// *The wiki needs recovery.*
    NeedsRecovery,
    /// *Refused: the named limit.*
    Refused(QuotaRefusal),
}

/// The page that says how an acceptance of `proposal` of `wiki` ended, in the visitor's language.
pub fn render_accept_outcome(
    page: &Page,
    wiki: &WikiRecord,
    proposal: &ProposalId,
    outcome: &AcceptOutcome,
) -> Response {
    let t = page.t();
    let (status, heading, paragraphs) = match outcome {
        AcceptOutcome::NotYetAvailable => (
            StatusCode::NOT_IMPLEMENTED,
            Key::AcceptHeading,
            vec![t.get(Key::AcceptNotYet), t.get(Key::AcceptNotYetKept)],
        ),
        AcceptOutcome::Signed => (
            StatusCode::OK,
            Key::OutcomeSigned,
            vec![t.get(Key::OutcomeNoteSigned)],
        ),
        AcceptOutcome::Overlap => (
            StatusCode::CONFLICT,
            Key::OutcomeOverlap,
            vec![t.get(Key::OutcomeNoteOverlap)],
        ),
        AcceptOutcome::NotAllowed => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            vec![t.get(Key::OutcomeNoteNotAllowed)],
        ),
        AcceptOutcome::Busy { waiting } => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeBusy,
            vec![
                t.get(Key::OutcomeNoteBusy),
                t.msg(Key::OutcomeNoteBusyDepth).n(*waiting).text(),
            ],
        ),
        AcceptOutcome::NeedsRecovery => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeRecovery,
            vec![t.get(Key::OutcomeNoteRecovery)],
        ),
        AcceptOutcome::Refused(q) => (
            StatusCode::TOO_MANY_REQUESTS,
            Key::OutcomeRefusedHeading,
            words::quota_paragraphs(t, q),
        ),
    };
    let response = notice(
        page,
        wiki,
        status,
        heading,
        paragraphs,
        vec![Choice {
            label: t.get(Key::ProposalBack),
            href: paths::proposal(wiki.slug.as_str(), proposal.as_str()),
        }],
        None,
    );
    match outcome {
        AcceptOutcome::Busy { .. } | AcceptOutcome::NeedsRecovery => with_retry_after(response),
        _ => response,
    }
}

/// `POST /{slug}/-/proposals/{id}/accept`. **The one function the acceptance replaces** (handoff 004 T4):
/// until then it makes the checks an acceptance makes first and says that accepting is not available.
pub(crate) async fn accept(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path((slug, id))), Some(services)) = (ids, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if let Err(error) = ctx.services.proposals.get(&ctx.principal, &ctx.wiki, &id) {
            return refused(page, &ctx, &error, Vec::new());
        }
        let outcome = if ctx.may_on(Action::Accept, &id) {
            AcceptOutcome::NotYetAvailable
        } else {
            AcceptOutcome::NotAllowed
        };
        render_accept_outcome(page, &ctx.wiki, &id, &outcome)
    })
    .await
}

// ------------------------------------------------------------------------------------ a reason

/// The three acts that end or send back a proposal with a reason.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Decline,
    Withdraw,
    Ask,
}

struct ReasonWords {
    title: Key,
    heading: Key,
    intro: Key,
    button: Key,
    act: &'static str,
}

impl Reason {
    fn words(self) -> ReasonWords {
        match self {
            Reason::Decline => ReasonWords {
                title: Key::TitleDecline,
                heading: Key::ReasonDeclineHeading,
                intro: Key::ReasonDeclineIntro,
                button: Key::ReasonDeclineButton,
                act: paths::ACT_DECLINE,
            },
            Reason::Withdraw => ReasonWords {
                title: Key::TitleWithdraw,
                heading: Key::ReasonWithdrawHeading,
                intro: Key::ReasonWithdrawIntro,
                button: Key::ReasonWithdrawButton,
                act: paths::ACT_WITHDRAW,
            },
            Reason::Ask => ReasonWords {
                title: Key::TitleAsk,
                heading: Key::ReasonAskHeading,
                intro: Key::ReasonAskIntro,
                button: Key::ReasonAskButton,
                act: paths::ACT_ASK,
            },
        }
    }

    /// The state the act is done in, and the act, for the refusal of an act the state does not allow.
    fn op(self) -> ProposalOp {
        match self {
            Reason::Decline => ProposalOp::Decline,
            Reason::Withdraw => ProposalOp::Withdraw,
            Reason::Ask => ProposalOp::AskForRevision,
        }
    }

    fn allowed_in(self, state: ProposalState) -> bool {
        inklkunn_core::proposal::machine::apply(Some(state), self.op()).is_ok()
    }
}

#[derive(Template)]
#[template(path = "proposal_reason.html")]
struct ReasonTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    problem: Option<String>,
    action: String,
    label: String,
    body_name: &'static str,
    text: String,
    button: String,
    back: crate::proposals::render::LinkView,
}

/// May the principal do this to the proposal now? `Ok` with its record, or the refusal.
fn check(ctx: &Ctx, id: &ProposalId, reason: Reason) -> Result<(), ProposalError> {
    let record = ctx.services.proposals.get(&ctx.principal, &ctx.wiki, id)?;
    let permitted = match reason {
        Reason::Withdraw => ctx.account() == Some(&record.author),
        Reason::Decline | Reason::Ask => ctx.may_on(Action::Accept, id),
    };
    if !permitted {
        return Err(ProposalError::NotAllowed);
    }
    if !reason.allowed_in(record.state) {
        return Err(ProposalError::NotInThisState {
            op: reason.op(),
            state: record.state,
        });
    }
    Ok(())
}

fn reason_page(
    page: &Page,
    ctx: &Ctx,
    id: &ProposalId,
    reason: Reason,
    text: &str,
    problem: Option<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let w = reason.words();
    let shell = Shell::of(&ctx.wiki);
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let rendered = ReasonTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(w.title)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(w.heading),
        intro: t.get(w.intro),
        problem,
        action: paths::act(ctx.slug(), id.as_str(), w.act),
        label: t.get(Key::ReasonLabel),
        body_name: fields::REASON,
        text: text.to_owned(),
        button: t.get(w.button),
        back: crate::proposals::render::LinkView {
            href: paths::proposal(ctx.slug(), id.as_str()),
            label: t.get(Key::ProposalBack),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// `GET /{slug}/-/proposals/{id}/{decline,withdraw,ask}`: the form with a required reason.
pub(crate) async fn reason_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    reason: Reason,
) -> Response {
    let (Ok(Path((slug, id))), Some(services)) = (ids, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        match check(&ctx, &id, reason) {
            Ok(()) => reason_page(page, &ctx, &id, reason, "", None, StatusCode::OK),
            Err(error) => refused(page, &ctx, &error, back(page, &ctx, &id)),
        }
    })
    .await
}

/// `POST /{slug}/-/proposals/{id}/{decline,withdraw,ask}`.
pub(crate) async fn reason_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
    reason: Reason,
) -> Response {
    let begun = match begin(&state, &page, &headers, ids, form) {
        Ok(b) => b,
        Err(response) => return *response,
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&begun.services, &headers, &begun.slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&begun.id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let fields = match fields_of(page, &ctx, begun.form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let text = field(&fields, fields::REASON).to_owned();
        let service = &ctx.services.proposals;
        let done = match reason {
            Reason::Decline => service.decline(&ctx.principal, &ctx.wiki, &id, &text),
            Reason::Withdraw => service.withdraw(&ctx.principal, &ctx.wiki, &id, &text),
            Reason::Ask => service.ask_for_revision(&ctx.principal, &ctx.wiki, &id, &text),
        };
        match done {
            Ok(_) => see(&paths::proposal(ctx.slug(), id.as_str())),
            Err(ProposalError::Invalid(
                why @ (Invalid::ReasonMissing | Invalid::ReasonTooLong),
            )) => reason_page(
                page,
                &ctx,
                &id,
                reason,
                &text,
                Some(words::invalid_text(page.t(), why)),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            Err(error) => refused(page, &ctx, &error, back(page, &ctx, &id)),
        }
    })
    .await
}

/// `GET …/decline`.
pub(crate) async fn decline_form(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
) -> Response {
    reason_form(state, page, headers, ids, Reason::Decline).await
}

/// `POST …/decline`.
pub(crate) async fn decline_post(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
) -> Response {
    reason_post(state, page, headers, ids, form, Reason::Decline).await
}

/// `GET …/withdraw`.
pub(crate) async fn withdraw_form(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
) -> Response {
    reason_form(state, page, headers, ids, Reason::Withdraw).await
}

/// `POST …/withdraw`.
pub(crate) async fn withdraw_post(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
) -> Response {
    reason_post(state, page, headers, ids, form, Reason::Withdraw).await
}

/// `GET …/ask`.
pub(crate) async fn ask_form(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
) -> Response {
    reason_form(state, page, headers, ids, Reason::Ask).await
}

/// `POST …/ask`.
pub(crate) async fn ask_post(
    state: State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
) -> Response {
    reason_post(state, page, headers, ids, form, Reason::Ask).await
}
