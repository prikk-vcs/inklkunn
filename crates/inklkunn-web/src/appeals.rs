//! Appeals: *contest this*, the appellant's own appeals, and the page of the people who decide them (RFC 006 D-9, D-12; handoff
//! G-T5; threat model C-14f, C-14g). **Web only**: the read API gets an appellant's own appeals (G-T7) and nothing writes.
//!
//! **Every handler begins as every handler of a wiki does** ([`Ctx::open`](crate::proposals::Ctx)): who is asking, which wiki the
//! slug names, whether that principal may read it. A wiki that is absent and a wiki that may not be read are one page, and so are
//! the appeals pages and forms of it. The forms need an account, the CSRF token of the session, and a post from this site; the act
//! itself is the core's ([`AppealService`](inklkunn_core::appeal::AppealService)), which authorizes it, applies the rules, records
//! it, and refuses by name. This crate writes no record and holds no key (INV-4).
//!
//! **An appeal's words are read in two places only**: the appellant's own page, and the page of the people who decide appeals.
//! Every other surface, the moderation log included, says only *appealed* and how it ended.
//!
//! The pieces: [`page`] (the deciders' page and the decision), [`mine`] (a person's own appeals and what they can contest),
//! [`contest`] (the form that files one), [`words`] (the sentences) and [`paths`] (the addresses and the field names).

pub(crate) mod contest;
pub(crate) mod mine;
pub(crate) mod page;
pub(crate) mod paths;
pub(crate) mod words;

use axum::http::StatusCode;
use axum::response::Response;
use inklkunn_core::appeal::AppealError;

use crate::catalog::Key;
use crate::pages::Page;
use crate::proposals::Ctx;
use crate::wiki::{Choice, generic_not_found, notice, with_retry_after};

/// The body bound of every form of the appeals pages: an appeal's text, written three times over at most, and a few fields.
pub(crate) const FORM_LIMIT: usize = crate::proposals::SMALL_FORM_LIMIT;

/// The page that says why an act of appeals was refused, in words, with the way back. **Never the cause of a failure**: that goes
/// to the operator's log.
pub(crate) fn refused(page: &Page, ctx: &Ctx, error: &AppealError, back: Vec<Choice>) -> Response {
    let t = page.t();
    let (status, heading, paragraphs) = match error {
        AppealError::NotFound => return generic_not_found(page),
        AppealError::NotAllowed => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            vec![t.get(Key::RefusedNotAllowedBody)],
        ),
        AppealError::Restricted(notice) => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            crate::people::sentences::restriction_refusal(t, notice),
        ),
        AppealError::RulesUnreadable(fault) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeRulesUnreadable,
            crate::proposals::governance::unreadable_paragraphs(t, fault),
        ),
        AppealError::NotAffected => (
            StatusCode::NOT_FOUND,
            Key::ContestNothingHeading,
            vec![t.get(Key::ContestNothingBody)],
        ),
        AppealError::AlreadyOpen { .. } => (
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![t.get(Key::ContestAlreadyOpen)],
        ),
        AppealError::Quota(q) => (
            StatusCode::TOO_MANY_REQUESTS,
            Key::OutcomeRefusedHeading,
            crate::proposals::words::quota_paragraphs(t, q),
        ),
        AppealError::NoSuchAppeal => (
            StatusCode::NOT_FOUND,
            Key::AppealsRefusedHeading,
            vec![t.get(Key::AppealsRefusedNoSuch)],
        ),
        AppealError::AlreadyDecided => (
            StatusCode::CONFLICT,
            Key::AppealsRefusedHeading,
            vec![t.get(Key::AppealsRefusedDecided)],
        ),
        AppealError::Decider(why) => (
            StatusCode::FORBIDDEN,
            Key::AppealsRefusedHeading,
            vec![words::decider_refusal(t, *why)],
        ),
        AppealError::Overturn(why) => (
            StatusCode::CONFLICT,
            Key::AppealsRefusedHeading,
            vec![words::overturn_refusal(t, why)],
        ),
        AppealError::Text(fault) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::RefusedInvalidHeading,
            vec![words::text_problem(t, *fault)],
        ),
        AppealError::Reason(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::RefusedInvalidHeading,
            vec![words::reason_problem(t, error).unwrap_or_default()],
        ),
        AppealError::Changed => (
            StatusCode::CONFLICT,
            Key::RefusedNotNowHeading,
            vec![t.get(Key::RefusedChanged)],
        ),
        AppealError::Unavailable(cause) => {
            ctx.log(&format!("an appeal could not be handled now: {cause}"));
            return with_retry_after(notice(
                page,
                &ctx.wiki,
                StatusCode::SERVICE_UNAVAILABLE,
                Key::OutcomeUnavailable,
                vec![t.get(Key::OutcomeNoteUnavailable)],
                back,
                None,
            ));
        }
    };
    notice(page, &ctx.wiki, status, heading, paragraphs, back, None)
}
