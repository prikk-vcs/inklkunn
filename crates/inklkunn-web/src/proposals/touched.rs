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

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::knowledge::compose::{Curation, LinkEdit, curate};
use inklkunn_core::knowledge::onpage::touches;
use inklkunn_core::knowledge::touched::{How, PageChange, Touch};
use inklkunn_core::proposal::{ProposalEdit, ProposalError};
use inklkunn_store::{FileChange, ProposalState};

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

// ------------------------------------------------------------------------------------ the one step

/// `POST /{slug}/-/proposals/{id}/reconfirm`: one curation proposal that judges again, on the wording each has now,
/// the claims the accepted proposal `id` touched and left judged on an earlier wording.
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
    if record.state != ProposalState::Accepted {
        return refused(
            page,
            ctx,
            &ProposalError::NotInThisState {
                op: inklkunn_store::ProposalOp::Accept,
                state: record.state,
            },
            Vec::new(),
        );
    }
    let content = match service.revisions(p, w, id).map(|mut r| r.pop()) {
        Ok(Some(latest)) => latest.content,
        Ok(None) => return generic_not_found(page),
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    };
    let mut pages = Vec::new();
    for f in content
        .files
        .iter()
        .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_none())
    {
        match ctx
            .services
            .read
            .source_at(&ctx.wiki, &content.base.block_id, &f.path)
        {
            Ok(old) => pages.push((f.path.clone(), old, f.text.clone())),
            Err(e) => return read_failure(page, ctx, e),
        }
    }
    let touched = match of_revision(ctx, &record.view, &content.base.block_id, &pages) {
        Ok(found) => found,
        Err(e) => return read_failure(page, ctx, e),
    };
    // Each claim as the wiki has it now, judged again on the wording it has now.
    let view = Some(record.view.as_str());
    let base = match service.capture_base(w, view) {
        Ok(base) => base,
        Err(e) => return refused(page, ctx, &e, Vec::new()),
    };
    let mut files: Vec<FileChange> = Vec::new();
    for touch in touched.iter().filter(|t| leaves(t)) {
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
    let made = service.create(p, w, view, proposed);
    match made.and_then(|r| service.submit(p, w, &r.id).map(|_| r.id)) {
        Ok(new) => see(&paths::proposal(ctx.slug(), new.as_str())),
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}
