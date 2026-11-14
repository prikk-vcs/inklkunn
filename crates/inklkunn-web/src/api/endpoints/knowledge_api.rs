//! The knowledge layer's read endpoints (RFC 005 part 2 D-12; handoff 005 K-T11): a claim, a source, a page's provenance
//! paragraph by paragraph, and the comparison of two views. Each calls the read service's own reads (the ones the claim
//! page, the source lists, *Why is this here?* and *Compare views* call) and shapes the result; none reads a repository
//! file again, none fetches a source, and none ranks anything.
//!
//! **Authorized as the pages are, and existence-neutral.** The start is every endpoint's ([`serve`]): a wiki that is
//! absent and one the caller may not read answer the same problem before anything else. A claim or a source the wiki does
//! not hold, and an id that is not an id, are the same `not-found`. A proposal that decided a claim is named only to a
//! caller who may read it, and the same goes for the proposal a change was made from.

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::knowledge::index::KnowledgeIndex;
use inklkunn_core::knowledge::{ClaimId, SourceId};
use inklkunn_core::read::{Basis, Reach, WhyOutcome};
use inklkunn_markup::path::path_to_title;
use serde_json::{Value, json};

use super::past::status_code;
use super::{ambiguous, short, tip_json, view_of};
use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::reply::Reply;
use crate::api::request::{from_read, serve};
use crate::api::shape;
use crate::pages::PageState;
use crate::services::Services;
use crate::url::{decode, view_ref};

/// The most claims a source lists.
const MAX_CITED_BY: usize = 50;

/// The most characters of a proposal's summary or of one of its sources that a change names.
const MAX_SHOWN_CHARS: usize = 300;

/// The id the address names, or `not-found`: an id that is not a well formed id names nothing.
fn parse_id<T>(rest: &str, parse: impl Fn(&str) -> Option<T>) -> Result<(String, T), Problem> {
    let text = decode(rest).ok_or_else(|| Problem::new(Code::NotFound))?;
    let id = parse(&text).ok_or_else(|| Problem::new(Code::NotFound))?;
    Ok((text, id))
}

/// The knowledge index of the view's tip, with the tip it was read at (once, so the two agree).
fn index_at(
    services: &Services,
    wiki: &inklkunn_store::WikiRecord,
    view: Option<&str>,
) -> Result<(inklkunn_core::read::Tip, std::sync::Arc<KnowledgeIndex>), Problem> {
    let read = services.read.tip(wiki, view).and_then(|tip| {
        services
            .read
            .knowledge_at(wiki, &tip)
            .map(|index| (tip, index))
    });
    read.map_err(|e| from_read(services, wiki, e))
}

/// `GET /{slug}/-/api/v1/claims/{id}`.
pub(super) async fn claim(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::CLAIM,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let (id, _) = parse_id(&ctx.rest, ClaimId::parse)?;
            let (tip, index) = index_at(services, wiki, view.full.as_deref())?;
            let row = index
                .claim(&id)
                .ok_or_else(|| Problem::new(Code::NotFound))?;
            let decided = row
                .standing_proposal
                .as_deref()
                .filter(|_| row.standing != "unknown");
            let readable = decided
                .and_then(crate::proposals::parse_id)
                .filter(|p| services.proposals.get(&ctx.principal, wiki, p).is_ok());
            Reply::json(&json!({
                "id": row.id,
                "kind": row.kind,
                "lifecycle": row.lifecycle,
                "status": {
                    "standing": row.standing,
                    "proposal": readable.as_ref().map(ToString::to_string),
                    "proposal_unlisted": decided.is_some() && readable.is_none(),
                    "judged_on": row.standing_judgement,
                },
                "said": index.anchors_of(&row.id).map(|a| json!({
                    "title": a.page_path.as_deref().and_then(path_to_title),
                    "page_id": a.page_id,
                    "block": a.block,
                    "state": a.state,
                })).collect::<Vec<_>>(),
                "sources": index.links_of(&row.id).map(|l| json!({
                    "source": l.source,
                    "relation": l.relation,
                    "bearing": l.bearing,
                    "judged_on": l.judgement,
                    "readable": index.source(&l.source).is_some(),
                })).collect::<Vec<_>>(),
                "relations": index.relations_of(&row.id).map(|r| json!({
                    "type": r.kind,
                    "claim": r.other,
                })).collect::<Vec<_>>(),
                "tip": tip_json(&tip),
            }))
        },
    )
    .await
}

/// `GET /{slug}/-/api/v1/sources/{id}`.
pub(super) async fn source(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::SOURCE,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let (id, _) = parse_id(&ctx.rest, SourceId::parse)?;
            let (tip, index) = index_at(services, wiki, view.full.as_deref())?;
            let row = index
                .source(&id)
                .ok_or_else(|| Problem::new(Code::NotFound))?;
            let cited: Vec<_> = index.claims_citing(&row.id).collect();
            Reply::json(&json!({
                "id": row.id,
                "kind": row.kind,
                "availability": row.availability,
                "last_reviewed": row.last_reviewed,
                "superseded_by": row.superseded_by,
                "title": row.title,
                "author": row.author,
                "date": row.date,
                "reference": row.reference,
                "claims": cited.iter().take(MAX_CITED_BY).map(|l| json!({
                    "claim": l.claim,
                    "relation": l.relation,
                    "bearing": l.bearing,
                    "judged_on": l.judgement,
                })).collect::<Vec<_>>(),
                "claims_total": cited.len(),
                "tip": tip_json(&tip),
            }))
        },
    )
    .await
}

/// `text` on one line and at most [`MAX_SHOWN_CHARS`] characters, with an ellipsis where it was cut.
fn shortened(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_SHOWN_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(MAX_SHOWN_CHARS - 1).collect();
    cut.push('…');
    cut
}

const fn reach_code(reach: Reach) -> &'static str {
    match reach {
        Reach::Whole => "whole",
        Reach::Bounded => "bounded",
        Reach::Spent => "spent",
        Reach::Unfollowed => "unfollowed",
    }
}

/// `GET /{slug}/-/api/v1/provenance/{*title}`.
pub(super) async fn provenance(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::PROVENANCE,
        |services, wiki, params, ctx| {
            let view = view_of(params)?;
            let title = ctx.title()?;
            let page = match services.read.page_why(wiki, view.full.as_deref(), &title) {
                Ok(WhyOutcome::Page(page)) => *page,
                Ok(WhyOutcome::Missing { .. }) => return Err(Problem::new(Code::PageNotFound)),
                Ok(WhyOutcome::Ambiguous { candidates }) => return Err(ambiguous(&candidates)),
                Err(e) => return Err(from_read(services, wiki, e)),
            };
            // The claims said in the page's paragraphs, from the knowledge index of the same view; a page that cannot be
            // asked about its claims is served without them, as the page is.
            let index = page.page_id.as_deref().and_then(|_| {
                services
                    .read
                    .knowledge_index(wiki, view.full.as_deref())
                    .map_err(|e| {
                        (services.log)(&format!(
                            "the claims of a page of wiki `{}` could not be read now: {e}",
                            wiki.slug
                        ));
                    })
                    .ok()
            });
            let said_in = |block: &str| -> Vec<&str> {
                match (&index, page.page_id.as_deref()) {
                    (Some(index), Some(page_id)) => index
                        .claims_on_page(page_id)
                        .filter(|a| a.block == block)
                        .map(|a| a.claim.as_str())
                        .collect(),
                    _ => Vec::new(),
                }
            };
            let number = |step: Option<usize>| -> Option<u64> {
                step.and_then(|s| page.changes.get(s)).map(|c| c.number)
            };
            let paragraphs: Vec<Value> = page
                .paragraphs
                .iter()
                .map(|p| {
                    json!({
                        "ordinal": p.paragraph.ordinal,
                        "id": p.paragraph.id,
                        "excerpt": p.paragraph.excerpt,
                        "claims": p.paragraph.id.as_deref().map(said_in).unwrap_or_default(),
                        "basis": match p.chain.basis {
                            Basis::Anchor => "anchor",
                            Basis::Text => "text",
                        },
                        "last_changed": number(p.chain.last_changed),
                        "first_written": number(p.chain.first_written),
                    })
                })
                .collect();
            let named: std::collections::BTreeSet<usize> = page
                .paragraphs
                .iter()
                .flat_map(|p| [p.chain.first_written, p.chain.last_changed])
                .flatten()
                .collect();
            let changes: Vec<Value> = named
                .into_iter()
                .filter_map(|step| page.changes.get(step))
                .map(|info| {
                    let mut change = shape::change_head(ctx.t, info);
                    change.insert(
                        "proposal".to_owned(),
                        proposal_of(services, ctx, wiki, info),
                    );
                    Value::Object(change)
                })
                .collect();
            Reply::json(&json!({
                "title": page.title,
                "tip": tip_json(&page.tip),
                "reach": reach_code(page.reach),
                "left_out": page.left_out,
                "paragraphs": paragraphs,
                "changes": changes,
            }))
        },
    )
    .await
}

/// The proposal a change was made from, for a caller who may read it: the message names it, the acceptance records tie
/// it to this change, and the proposal is the caller's to read. `null` otherwise, and `null` says nothing of which.
fn proposal_of(
    services: &Services,
    ctx: &crate::api::request::Ctx,
    wiki: &inklkunn_store::WikiRecord,
    info: &inklkunn_core::read::ChangeInfo,
) -> Value {
    let Some(id) = info
        .proposal
        .as_deref()
        .and_then(crate::proposals::parse_id)
    else {
        return Value::Null;
    };
    let Ok(record) = services.proposals.get(&ctx.principal, wiki, &id) else {
        return Value::Null;
    };
    if !crate::knowledge::why::sealed_as(services, &ctx.principal, wiki, &id, &info.block_id) {
        return Value::Null;
    }
    json!({
        "id": id.to_string(),
        "summary": record.summary,
        "sources": record.sources.iter().map(|s| shortened(s)).collect::<Vec<_>>(),
    })
}

/// `GET /{slug}/-/api/v1/views/compare`.
pub(super) async fn compare_views(
    State(state): State<PageState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    serve(
        state,
        headers,
        uri,
        &describe::COMPARE_VIEWS,
        |services, wiki, params, _| {
            let named = |name: &str| {
                view_ref(name).ok_or_else(|| Problem::new(Code::NoSuchView).named("view", name))
            };
            let a = params.text("a");
            let b = params
                .text("b")
                .ok_or_else(|| Problem::new(Code::InvalidParameter).named("parameter", "b"))?;
            let a_full = a.map(named).transpose()?;
            let b_full = named(b)?;
            // The same view twice compares nothing with itself.
            let default = short(&wiki.default_view);
            if a.unwrap_or(default) == b {
                return Err(Problem::new(Code::InvalidParameter).named("parameter", "b"));
            }
            let comparison = services
                .read
                .compare_views(wiki, a_full.as_deref(), Some(&b_full))
                .map_err(|e| from_read(services, wiki, e))?;
            Reply::json(&json!({
                "a": tip_json(&comparison.a),
                "b": tip_json(&comparison.b),
                "same": comparison.same,
                "pages": comparison.pages.iter().map(|p| json!({
                    "title": p.title,
                    "path": p.path,
                    "status": status_code(p.status),
                    "added": p.added,
                    "removed": p.removed,
                })).collect::<Vec<_>>(),
                "pages_omitted": comparison.pages_omitted,
                "claims": comparison.claims.iter().map(|c| json!({
                    "id": c.difference.id,
                    "presence": presence_code(c.difference.presence),
                    "wording": c.difference.wording,
                    "status": c.difference.status.as_ref().map(|(x, y)| json!({"a": x, "b": y})),
                    "sources": c.difference.sources,
                    "said_in": c.said_in,
                })).collect::<Vec<_>>(),
                "claims_omitted": comparison.claims_omitted,
            }))
        },
    )
    .await
}

const fn presence_code(presence: inklkunn_core::knowledge::compare::Presence) -> &'static str {
    use inklkunn_core::knowledge::compare::Presence;
    match presence {
        Presence::Both => "both",
        Presence::OnlyA => "only-a",
        Presence::OnlyB => "only-b",
        Presence::UnreadableInA => "unreadable-in-a",
        Presence::UnreadableInB => "unreadable-in-b",
    }
}
