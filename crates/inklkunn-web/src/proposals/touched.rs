//! The claims a proposal touches, as its page says them, and the one step that judges them again (RFC 005 part 1
//! D-1 c; part 2 D-3).
//!
//! The page lists, for each claim whose paragraph the change rewords or removes, what the change does to what was
//! judged on the paragraph's earlier wording: the sources and the status that will be shown as *judged on an earlier
//! wording*. The service states the facts; it never suggests whether a reworded paragraph "means the same".
//!
//! **One step for all of them.** Once the proposal is accepted, the page offers a single button that proposes
//! judging every one of those claims again on the wording it has now. It is an ordinary curation proposal (a person
//! reviews it, someone accepts it, the history records it); nothing is judged again by a click. Before the
//! acceptance there is nothing to judge again, so the page says where the step will be.
//!
//! The step works from **the revision that was accepted**, names the change it re-confirms (the curation proposal is
//! made with that note, and the review record of its acceptance keeps which claims it re-confirmed), and is **not
//! offered again** while a proposal made for it is open, nor for claims an accepted one already judged again (handoff
//! 005 K11).

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::knowledge::compose::{Curation, LinkEdit, curate};
use inklkunn_core::knowledge::onpage::touches;
use inklkunn_core::knowledge::touched::{How, PageChange, Touch};
use inklkunn_core::proposal::{ProposalEdit, ProposalError};
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, FileChange, ProposalId, ProposalState, RevisionContent,
};

use super::curate::load;
use super::knowledge::Line;
use super::labels::standing_word;
use super::new::read_failure;
use super::parts::excerpt;
use super::{
    Ctx, fields_of, from_this_site, needs_account, notice_page, parse_id, paths, refused, see,
    serve,
};
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::pages::{Page, PageState};
use crate::wiki::{Choice, generic_not_found, services_of};

type Fields = super::urlencoded::Raw;

/// The lines of a proposal's page for these touches: what happens to the paragraph, and what that leaves judged on
/// an earlier wording.
pub(crate) fn lines(t: T, touches: &[Touch]) -> Vec<Line> {
    touches
        .iter()
        .map(|touch| {
            let words = excerpt(&touch.words);
            let mut more = Vec::new();
            if !touch.links.is_empty() {
                more.push(
                    t.msg(Key::TouchLinksEarlier)
                        .n(touch.links.len() as u64)
                        .text(),
                );
            }
            if let Some(standing) = touch.standing {
                more.push(
                    t.msg(Key::TouchStandingEarlier)
                        .with("status", t.get(standing_word(standing)))
                        .text(),
                );
            }
            match touch.how {
                How::Removed => Line {
                    text: t.msg(Key::TouchRemoved).with("words", words).text(),
                    more,
                },
                How::Reworded => {
                    if more.is_empty() {
                        more.push(t.get(Key::TouchNothingEarlier));
                    }
                    Line {
                        text: t.msg(Key::TouchReworded).with("words", words).text(),
                        more,
                    }
                }
            }
        })
        .collect()
}

/// Does this touch leave something judged on an earlier wording?
fn leaves(touch: &Touch) -> bool {
    !touch.links.is_empty() || touch.standing.is_some()
}

/// Does any of these leave something judged on an earlier wording?
pub(crate) fn leaves_earlier(touches: &[Touch]) -> bool {
    touches.iter().any(leaves)
}

/// The claims a proposal's revision touches: its pages against the base they were written on.
pub(crate) fn of_revision(
    ctx: &Ctx,
    view: &str,
    base_block: &str,
    pages: &[(String, Option<String>, Option<String>)],
) -> Result<Vec<Touch>, inklkunn_core::read::ReadError> {
    let changes: Vec<PageChange<'_>> = pages
        .iter()
        .map(|(path, old, new)| PageChange {
            path,
            old: old.as_deref(),
            new: new.as_deref(),
        })
        .collect();
    touches(
        &ctx.services.read,
        &ctx.wiki,
        Some(view),
        base_block,
        &changes,
    )
}

/// A page a revision writes: its path, its text at the base and the text the revision gives it.
pub(crate) type WrittenPage = (String, Option<String>, Option<String>);

/// The pages a revision writes, each with its text at the base the revision was made on and the text the revision gives
/// it (the claim and source files are left out: they are listed on their own).
pub(crate) fn pages_of(
    ctx: &Ctx,
    content: &RevisionContent,
) -> Result<Vec<WrittenPage>, inklkunn_core::read::ReadError> {
    let mut pages = Vec::new();
    for f in content
        .files
        .iter()
        .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_none())
    {
        let old = ctx
            .services
            .read
            .source_at(&ctx.wiki, &content.base.block_id, &f.path)?;
        pages.push((f.path.clone(), old, f.text.clone()));
    }
    Ok(pages)
}

/// The claims a revision touches: [`pages_of`], then [`of_revision`].
pub(crate) fn of_content(
    ctx: &Ctx,
    view: &str,
    content: &RevisionContent,
) -> Result<Vec<Touch>, inklkunn_core::read::ReadError> {
    let pages = pages_of(ctx, content)?;
    if pages.is_empty() {
        return Ok(Vec::new());
    }
    of_revision(ctx, view, &content.base.block_id, &pages)
}

/// Why the claims of a revision could not be found.
pub(crate) enum Unfound {
    /// The records said no.
    Proposal(ProposalError),
    /// The repository could not be read.
    Read(inklkunn_core::read::ReadError),
}

/// The ids of the claims that revision `revision` of the proposal touches (RFC 005 part 1 D-1 d): what a *request
/// changes* decision on it is recorded with. Empty for a revision the proposal does not have (the decision itself
/// says it is stale).
pub(crate) fn claims_of_revision(
    ctx: &Ctx,
    id: &ProposalId,
    revision: u32,
) -> Result<Vec<String>, Unfound> {
    let record = ctx
        .services
        .proposals
        .get(&ctx.principal, &ctx.wiki, id)
        .map_err(Unfound::Proposal)?;
    let revisions = ctx
        .services
        .proposals
        .revisions(&ctx.principal, &ctx.wiki, id)
        .map_err(Unfound::Proposal)?;
    let Some(found) = revisions.into_iter().find(|r| r.seq == revision) else {
        return Ok(Vec::new());
    };
    let touched = of_content(ctx, &record.view, &found.content).map_err(Unfound::Read)?;
    Ok(touched.into_iter().map(|t| t.claim.to_string()).collect())
}

/// What an acceptance put in the history: **the revision that was accepted** (not the latest, not whatever a later
/// revision says) and the acceptance that sealed it, which is the change.
pub(crate) struct AcceptedChange {
    /// The acceptance that sealed the revision: the change the review record and the reconfirm step name.
    pub(crate) acceptance: AcceptanceId,
    /// The revision that was accepted.
    pub(crate) content: RevisionContent,
}

/// The change an accepted proposal made: the revision whose acceptance sealed, with that acceptance. `None` when no
/// revision of the proposal has a sealed acceptance.
pub(crate) fn accepted_change(
    ctx: &Ctx,
    id: &ProposalId,
) -> Result<Option<AcceptedChange>, ProposalError> {
    let revisions = ctx
        .services
        .proposals
        .revisions(&ctx.principal, &ctx.wiki, id)?;
    for revision in revisions {
        let entries = ctx
            .services
            .acceptances
            .acceptances_of(id, revision.seq)
            .map_err(ProposalError::Store)?;
        let sealed = entries.into_iter().find_map(|e| match e.result?.outcome {
            AcceptanceOutcome::Sealed { .. } => Some(e.acceptance.id),
            AcceptanceOutcome::Failed { .. } => None,
        });
        if let Some(acceptance) = sealed {
            return Ok(Some(AcceptedChange {
                acceptance,
                content: revision.content,
            }));
        }
    }
    Ok(None)
}

/// What the page does about the reconfirm step for an accepted change.
pub(crate) enum Offer {
    /// The change left nothing judged on an earlier wording.
    Nothing,
    /// The claims still waiting to be judged again: the step is offered. (Those the change's review record says an
    /// accepted curation proposal already re-confirmed are not among them.)
    Offered(Vec<Touch>),
    /// Every claim that was left is judged again by an accepted proposal: the step is not offered.
    Done,
    /// A proposal made to judge them again is still open: the step is not offered again until it is closed.
    Open(ProposalId),
}

/// The step's state for the change `change` whose claims are `touched`.
pub(crate) fn offer(
    ctx: &Ctx,
    change: &AcceptanceId,
    touched: &[Touch],
) -> Result<Offer, ProposalError> {
    let leaving: Vec<&Touch> = touched.iter().filter(|t| leaves(t)).collect();
    if leaving.is_empty() {
        return Ok(Offer::Nothing);
    }
    let service = &ctx.services.proposals;
    // A proposal made to re-confirm this change that is still open stands in the way of another.
    for id in service.reconfirmations_of(&ctx.principal, &ctx.wiki, change)? {
        if service
            .get(&ctx.principal, &ctx.wiki, &id)
            .is_ok_and(|p| p.state.is_open())
        {
            return Ok(Offer::Open(id));
        }
    }
    // The review record names the claims that an accepted curation proposal judged again.
    let judged: Vec<String> = service
        .claim_reviews_of(&ctx.principal, &ctx.wiki, change)?
        .into_iter()
        .filter(|r| r.reconfirmed_by.is_some())
        .map(|r| r.claim)
        .collect();
    let waiting: Vec<Touch> = leaving
        .into_iter()
        .filter(|t| !judged.iter().any(|c| c == t.claim.as_str()))
        .cloned()
        .collect();
    Ok(if waiting.is_empty() {
        Offer::Done
    } else {
        Offer::Offered(waiting)
    })
}

// ------------------------------------------------------------------------------------ the one step

/// `POST /{slug}/-/proposals/{id}/reconfirm`: one curation proposal that judges again, on the wording each has now,
/// the claims the accepted proposal `id` touched and left judged on an earlier wording. It works from **the revision that
/// was accepted**, names the change it re-confirms (the review record keeps that, so the acceptance of the curation can
/// say which change it re-confirmed), and is not made again while one is open.
pub(crate) async fn reconfirm(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: Result<Path<(String, String)>, PathRejection>,
    form: Fields,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path((slug, id))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        // The form carries nothing but the session's token: it is read, so that a body that is not a form is refused.
        if let Err(response) = fields_of(page, &ctx, &headers, form) {
            return *response;
        }
        judge_again(page, &ctx, &id)
    })
    .await
}

fn judge_again(page: &Page, ctx: &Ctx, id: &inklkunn_store::ProposalId) -> Response {
    let t = page.t();
    let service = &ctx.services.proposals;
    let (p, w) = (&ctx.principal, &ctx.wiki);
    let record = match service.get(p, w, id) {
        Ok(record) => record,
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    };
    // Only what has been accepted left anything judged on an earlier wording.
    let not_accepted = ProposalError::NotInThisState {
        op: inklkunn_store::ProposalOp::Accept,
        state: record.state,
    };
    if record.state != ProposalState::Accepted {
        return refused(page, ctx, &not_accepted, Vec::new());
    }
    // The revision that was accepted, and the change that put it in the history.
    let accepted = match accepted_change(ctx, id) {
        Ok(Some(accepted)) => accepted,
        Ok(None) => return refused(page, ctx, &not_accepted, Vec::new()),
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    };
    let touched = match of_content(ctx, &record.view, &accepted.content) {
        Ok(found) => found,
        Err(e) => return read_failure(page, ctx, e),
    };
    // Not again while a proposal made for it is open; and only the claims no accepted curation judged again.
    let waiting = match offer(ctx, &accepted.acceptance, &touched) {
        Ok(Offer::Offered(waiting)) => waiting,
        Ok(Offer::Open(open)) => {
            let label = service
                .get(p, w, &open)
                .map_or_else(|_| t.get(Key::ProposalAnother), |o| excerpt(&o.summary));
            return notice_page(
                page,
                ctx,
                StatusCode::OK,
                Key::TouchHeading,
                vec![
                    t.msg(Key::TouchReconfirmOpen)
                        .with("proposal", &label)
                        .text(),
                ],
                vec![Choice {
                    label,
                    href: paths::proposal(ctx.slug(), open.as_str()),
                }],
            );
        }
        Ok(Offer::Nothing | Offer::Done) => Vec::new(),
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    };
    // Each claim as the wiki has it now, judged again on the wording it has now.
    let view = Some(record.view.as_str());
    let base = match service.capture_base(w, view) {
        Ok(base) => base,
        Err(e) => return refused(page, ctx, &e, Vec::new()),
    };
    let mut files: Vec<FileChange> = Vec::new();
    for touch in &waiting {
        let loaded = match load(ctx, view, &base.block_id, &touch.claim) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => continue,
            Err(e) => return read_failure(page, ctx, e),
        };
        let claim = loaded.claim();
        let edit = Curation {
            status: claim.community_status.status,
            links: claim
                .sources
                .iter()
                .map(|l| LinkEdit {
                    source: l.source.clone(),
                    relation: l.relation,
                    bearing: l.bearing,
                })
                .collect(),
            sources: Vec::new(),
            confirm: true,
        };
        // A claim said nowhere now has no wording to judge on; it is left as it is.
        let Ok(written) = curate(claim, &[], loaded.digest(), &edit) else {
            continue;
        };
        files.extend(written.into_iter().map(|(path, text)| FileChange {
            path,
            text: Some(text),
        }));
    }
    if files.is_empty() {
        return notice_page(
            page,
            ctx,
            StatusCode::OK,
            Key::TouchHeading,
            vec![t.get(Key::TouchNothingToJudge)],
            vec![Choice {
                label: t.get(Key::ProposalBack),
                href: paths::proposal(ctx.slug(), id.as_str()),
            }],
        );
    }
    // The proposal's own summary, cut to a length a summary of its own can hold (the most one may be is 120).
    let summary = t
        .msg(Key::TouchReconfirmSummary)
        .with("summary", excerpt(&record.summary))
        .text();
    let proposed = ProposalEdit {
        summary,
        files,
        base: Some(base),
        ..ProposalEdit::default()
    };
    // The curation proposal is made with the note of the change it re-confirms, in one step.
    let made = service.create_reconfirm(p, w, view, proposed, &accepted.acceptance);
    match made.and_then(|r| service.submit(p, w, &r.id).map(|_| r.id)) {
        Ok(new) => see(&paths::proposal(ctx.slug(), new.as_str())),
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}
