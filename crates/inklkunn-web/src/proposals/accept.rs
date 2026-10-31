//! Accept (RFC 004 D-4, D-12; WEB-04): the form post that takes a proposal into the history, and the page that
//! says how the acceptance ended.
//!
//! [`accept`] makes the checks an acceptance makes first (a form from this site, an account, a proposal the
//! visitor may read, the `accept` action on it), reads the revision the form says it was drawn from, and asks
//! the [`Acceptor`](crate::services::Acceptor) on a blocking thread. **Every answer of the acceptance is a page
//! of its own** ([`outcome_of`] maps each `Accepted` and `AcceptFailure` to an [`AcceptOutcome`]; the match is
//! exhaustive, so a new variant of the acceptance does not compile until a page says it). A page says what
//! state things are in and never prikk's text, a path or a cause: operators get the detail through
//! `services.log`.
//!
//! A server without an acceptor (`Services::acceptor` is `None`: the binary builds none until sign-in is
//! served) answers [`AcceptOutcome::NotYetAvailable`], after the same checks.

use std::collections::HashMap;

use axum::extract::rejection::{FormRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use inklkunn_core::acceptance::{Reconciled, words as accept_words};
use inklkunn_core::proposal::{ProposalError, QuotaRefusal};
use inklkunn_core::{AcceptFailure, Accepted, Action};
use inklkunn_store::{ProposalId, WikiRecord};

use super::{
    Ctx, Stop, fields, fields_of, from_this_site, needs_account, notice_page, parse_id, paths,
    refused, serve, words,
};
use crate::catalog::Key;
use crate::pages::{Page, PageState};
use crate::wiki::{Choice, generic_not_found, notice, services_of, with_retry_after};

type Fields = Result<axum::extract::Form<HashMap<String, String>>, FormRejection>;
type Ids = Result<Path<(String, String)>, PathRejection>;

/// The longest a visitor is told to wait before asking again, in seconds.
const LONGEST_WAIT: u64 = 3600;

/// Why an acceptance that did not reach the history left the repository as it was, in the fixed words of the
/// acceptance ([`accept_words`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchanged {
    /// The history refused the prepared change.
    Refused,
    /// The change needs more memory than one signing may use.
    TooLarge,
    /// The signer was busy.
    Busy,
}

/// What an acceptance can come to (RFC 004 D-12), in the glossary's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptOutcome {
    /// Accepting is not served by this build.
    NotYetAvailable,
    /// *Signed into the history* (by this call, or by an earlier one: a retry finds the record).
    Signed,
    /// *Stopped: changes overlap.*
    Overlap,
    /// *Not allowed.*
    NotAllowed,
    /// *The wiki is busy*, with how many acts are ahead and how many seconds to wait before asking again.
    Busy {
        /// The queue's depth: the acts ahead of this one.
        waiting: u64,
        /// What `Retry-After` says.
        retry_after: u64,
    },
    /// *The wiki needs recovery.*
    NeedsRecovery,
    /// *Refused: the named limit.*
    Refused(QuotaRefusal),
    /// Nothing was changed, and the fixed word says why.
    Unchanged(Unchanged),
    /// An earlier acceptance of this revision has not finished.
    InProgress,
    /// The history already has everything the revision asks for.
    NothingToChange,
    /// The repository or the records could not be asked before anything was written.
    Unavailable,
    /// The records failed after the change may have been made: the result is not certain.
    Uncertain,
    /// The proposal's own state or revision does not allow it (the state machine's word).
    Proposal(ProposalError),
}

/// What an acceptance's answer comes to as a page. The detail an operator needs goes to `log`; nothing of it
/// reaches the page.
pub(crate) fn outcome_of(
    result: Result<Accepted, AcceptFailure>,
    log: &dyn Fn(&str),
) -> AcceptOutcome {
    match result {
        Ok(Accepted::Signed(signed)) => {
            if let Some(why) = &signed.unconfirmed {
                log(&format!(
                    "acceptance {} is sealed but the seal could not be confirmed: {why:?}",
                    signed.acceptance
                ));
            }
            if signed.seeds_left {
                log("a temporary directory of decrypted seeds was left behind; remove it");
            }
            match &signed.reconciliation {
                Reconciled::Findings(findings) => log(&format!(
                    "the reconciliation after acceptance {} found {} disagreement(s)",
                    signed.acceptance,
                    findings.len()
                )),
                Reconciled::NotRun(why) => {
                    log(&format!("the reconciliation could not run now: {why}"));
                }
                Reconciled::Clean | Reconciled::Skipped => {}
            }
            AcceptOutcome::Signed
        }
        Ok(Accepted::AlreadySigned { .. }) => AcceptOutcome::Signed,
        // A proposal that is not there is told like one the visitor may not accept: never a difference.
        Err(AcceptFailure::NotAllowed | AcceptFailure::NotFound) => AcceptOutcome::NotAllowed,
        Err(AcceptFailure::Proposal(error)) => AcceptOutcome::Proposal(error),
        Err(AcceptFailure::Busy(busy)) => AcceptOutcome::Busy {
            waiting: busy.depth,
            retry_after: busy.estimate.as_secs().clamp(1, LONGEST_WAIT),
        },
        Err(AcceptFailure::Refused(refusal)) => AcceptOutcome::Refused(refusal),
        Err(AcceptFailure::NeedsRecovery(flag)) => {
            log(&format!(
                "the wiki needs recovery ({}); writes are refused until it has been looked at",
                flag.reason.as_str()
            ));
            AcceptOutcome::NeedsRecovery
        }
        Err(AcceptFailure::Incomplete {
            acceptance,
            reason,
            flagged,
            detail,
        }) => {
            log(&format!(
                "acceptance {acceptance} is incomplete ({}; flag recorded: {flagged}): {detail}",
                reason.as_str()
            ));
            AcceptOutcome::NeedsRecovery
        }
        Err(AcceptFailure::InProgress { acceptance }) => {
            log(&format!(
                "acceptance {acceptance} of this revision is pending"
            ));
            AcceptOutcome::InProgress
        }
        Err(AcceptFailure::Overlap { .. }) => AcceptOutcome::Overlap,
        Err(AcceptFailure::NothingToChange) => AcceptOutcome::NothingToChange,
        Err(AcceptFailure::Failed {
            acceptance,
            word,
            detail,
        }) => {
            log(&format!(
                "acceptance {acceptance} failed ({word}): {detail}"
            ));
            match word {
                accept_words::REFUSED => AcceptOutcome::Unchanged(Unchanged::Refused),
                accept_words::MEMORY_LIMIT => AcceptOutcome::Unchanged(Unchanged::TooLarge),
                accept_words::BUSY => AcceptOutcome::Unchanged(Unchanged::Busy),
                _ => AcceptOutcome::Unavailable,
            }
        }
        Err(AcceptFailure::Unavailable(why)) => {
            log(&format!("an acceptance could not be done now: {why}"));
            AcceptOutcome::Unavailable
        }
        Err(AcceptFailure::Store(error)) => {
            log(&format!("the records failed during an acceptance: {error}"));
            AcceptOutcome::Uncertain
        }
    }
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
        AcceptOutcome::Busy { waiting, .. } => {
            let mut paragraphs = vec![t.get(Key::OutcomeNoteBusy)];
            if *waiting > 0 {
                paragraphs.push(t.msg(Key::OutcomeNoteBusyDepth).n(*waiting).text());
            }
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Key::OutcomeBusy,
                paragraphs,
            )
        }
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
        AcceptOutcome::Unchanged(Unchanged::Refused) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::OutcomeUnchangedRefused,
            vec![t.get(Key::OutcomeNoteUnchangedRefused)],
        ),
        AcceptOutcome::Unchanged(Unchanged::TooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Key::OutcomeTooLarge,
            vec![t.get(Key::OutcomeNoteTooLarge)],
        ),
        AcceptOutcome::Unchanged(Unchanged::Busy) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeBusy,
            vec![t.get(Key::OutcomeNoteBusy)],
        ),
        AcceptOutcome::InProgress => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeInProgress,
            vec![t.get(Key::OutcomeNoteInProgress)],
        ),
        AcceptOutcome::NothingToChange => (
            StatusCode::CONFLICT,
            Key::OutcomeNothingToChange,
            vec![t.get(Key::OutcomeNoteNothingToChange)],
        ),
        AcceptOutcome::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeUnavailable,
            vec![t.get(Key::OutcomeNoteUnavailable)],
        ),
        AcceptOutcome::Uncertain => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Key::OutcomeStore,
            vec![t.get(Key::OutcomeNoteStore)],
        ),
        AcceptOutcome::Proposal(error) => match words::refusal(t, error) {
            Some(r) => (r.status, r.heading, r.paragraphs),
            None => (
                StatusCode::FORBIDDEN,
                Key::OutcomeNotAllowed,
                vec![t.get(Key::OutcomeNoteNotAllowed)],
            ),
        },
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
        AcceptOutcome::Busy { retry_after, .. } => retry_after_secs(response, *retry_after),
        AcceptOutcome::NeedsRecovery
        | AcceptOutcome::Unchanged(Unchanged::Busy)
        | AcceptOutcome::InProgress
        | AcceptOutcome::Unavailable => with_retry_after(response),
        _ => response,
    }
}

fn retry_after_secs(mut response: Response, secs: u64) -> Response {
    if let Ok(value) = HeaderValue::from_str(&secs.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// The revision the form says it was drawn from, or the page that says the form was not complete.
fn revision_of(page: &Page, ctx: &Ctx, id: &ProposalId, form: Fields) -> Result<u32, Stop> {
    let fields = fields_of(page, ctx, form)?;
    fields
        .get(fields::REVISION)
        .and_then(|v| v.trim().parse::<u32>().ok())
        .ok_or_else(|| {
            Box::new(notice_page(
                page,
                ctx,
                StatusCode::UNPROCESSABLE_ENTITY,
                Key::RefusedInvalidHeading,
                vec![page.get(Key::InvalidIncomplete)],
                vec![Choice {
                    label: page.get(Key::ProposalBack),
                    href: paths::proposal(ctx.slug(), id.as_str()),
                }],
            ))
        })
}

/// `POST /{slug}/-/proposals/{id}/accept`.
pub(crate) async fn accept(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    ids: Ids,
    form: Fields,
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
        // A proposal the visitor may not read is the generic not-found page; one they may read but not accept
        // is *not allowed*. Neither runs anything.
        if let Err(error) = ctx.services.proposals.get(&ctx.principal, &ctx.wiki, &id) {
            return refused(page, &ctx, &error, Vec::new());
        }
        let outcome = if !ctx.may_on(Action::Accept, &id) {
            AcceptOutcome::NotAllowed
        } else if let Some(acceptor) = ctx.services.acceptor.clone() {
            let revision = match revision_of(page, &ctx, &id, form) {
                Ok(revision) => revision,
                Err(response) => return *response,
            };
            outcome_of(
                acceptor.accept(&ctx.principal, &ctx.wiki, &id, revision),
                &|line| ctx.log(line),
            )
        } else {
            AcceptOutcome::NotYetAvailable
        };
        render_accept_outcome(page, &ctx.wiki, &id, &outcome)
    })
    .await
}
