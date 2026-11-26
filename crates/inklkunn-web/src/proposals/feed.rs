//! The feed of a wiki's proposals (RFC 007 D-9; handoff 007 I-T5; requirements IX-5): `-/proposals.atom`, with an entry for each
//! proposal **when it was opened** (sent for review) and one **when it was decided** (accepted, declined, withdrawn or
//! superseded), newest first.
//!
//! **It shows what the proposals' list shows, to whoever may read the wiki**, and by the same reads:
//! * a wiki the reader may not read is the generic not-found page, byte for byte, as the page and the API answer it;
//! * the proposals are what [`ProposalService::list_sent`](inklkunn_core::proposal::ProposalService::list_sent) gives: **never a
//!   draft**, not even the reader's own (a feed is read by programs and shared; a draft is its author's working copy);
//! * what moderation hides is said as the page says it (the same [`named`](super::list::named)): a hidden summary is the
//!   tombstone's words and a hidden author is not named, in the entry's title, its text and its address's text, and nothing of
//!   a hidden text is anywhere in the feed.
//!
//! **Bounded, and by the time of the event:** the feed lists the newest [`ENTRIES`] events of the wiki's proposals
//! ([`ProposalService::list_events`](inklkunn_core::proposal::ProposalService::list_events)), the store choosing them in one
//! read, so the cost is the feed's and not the wiki's, and **a decision made now on a proposal sent long ago is there**, above
//! the openings that are older than it. The time of an entry is the time of the event itself (when it was sent, when it
//! closed), not when the feed was drawn.

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use inklkunn_store::{ProposalEvent, ProposalEventKind};

use super::{Ctx, serve};
use crate::catalog::{Key, T};
use crate::feed::{Entry, Feed};
use crate::pages::{Page, PageState};
use crate::proposals::{list::named, paths, words};
use crate::wiki::{generic_not_found, services_of, with_retry_after};

/// How many events the feed lists: the size of the window the store is asked for.
pub(crate) const ENTRIES: usize = 50;

/// The entry of one event of a proposal, with when it happened.
fn entry_of(t: T, ctx: &Ctx, state: &PageState, event: &ProposalEvent) -> (i64, Entry) {
    let p = &event.proposal;
    let (summary, author) = named(ctx, t, p);
    let wiki = &ctx.wiki;
    let href = state
        .origins
        .app(&paths::proposal(ctx.slug(), p.id.as_str()));
    let by = t
        .msg(Key::FeedProposalBy)
        .with("author", author.as_str())
        .text();
    let at = event.at;
    let entry = |kind: &str, title: String, categories: Vec<&'static str>| Entry {
        id: format!("urn:inklkunn:{}:proposal:{}:{kind}", wiki.id, p.id),
        title,
        updated: at,
        href: href.clone(),
        related: None,
        author: author.clone(),
        summary: Some(by.clone()),
        categories,
    };
    let entry = match event.kind {
        ProposalEventKind::Opened => entry(
            "opened",
            t.msg(Key::FeedProposalOpened)
                .with("summary", summary.as_str())
                .text(),
            vec!["opened"],
        ),
        ProposalEventKind::Decided => entry(
            "closed",
            t.msg(Key::FeedProposalDecided)
                .with("state", t.get(words::state_label(p.state)))
                .with("summary", summary.as_str())
                .text(),
            vec!["decided", p.state.as_str()],
        ),
    };
    (at, entry)
}

/// `GET /{slug}/-/proposals.atom`.
pub(crate) async fn atom(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (slug, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let t = page.t();
        let found = ctx
            .services
            .proposals
            .list_events(&ctx.principal, &ctx.wiki, ENTRIES);
        let events = match found {
            Ok(records) => records,
            // A proposal that is absent and one that may not be seen are the page's one answer.
            Err(
                inklkunn_core::proposal::ProposalError::NotFound
                | inklkunn_core::proposal::ProposalError::NotAllowed,
            ) => return generic_not_found(page),
            Err(error) => {
                ctx.log(&format!("the feed of proposals could not be read: {error}"));
                return with_retry_after(crate::pages::plain(
                    StatusCode::SERVICE_UNAVAILABLE,
                    t.lang(),
                    page.get(Key::UnavailableBody),
                ));
            }
        };
        // The store gave them newest first (the time, then the proposal's id, the opening before the decision).
        let all: Vec<(i64, Entry)> = events
            .iter()
            .map(|event| entry_of(t, &ctx, &state, event))
            .collect();
        let wiki = &ctx.wiki;
        let name = wiki.display_name.as_str();
        let base = format!("/{}", wiki.slug);
        let feed = Feed {
            lang: t.lang().tag(),
            id: format!("urn:inklkunn:{}:proposals", wiki.id),
            title: t.msg(Key::FeedProposalsTitle).with("wiki", name).text(),
            updated: all
                .first()
                .map_or_else(|| (ctx.services.clock)(), |(at, _)| *at),
            self_href: state.origins.app(&format!("{base}/-/proposals.atom")),
            alternate_href: state.origins.app(&format!("{base}/-/proposals")),
            author: name.to_owned(),
            version: env!("CARGO_PKG_VERSION"),
            entries: all.into_iter().map(|(_, entry)| entry).collect(),
        };
        let mut res = (StatusCode::OK, feed.render()).into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/atom+xml; charset=utf-8"),
        );
        if wiki.visibility != inklkunn_store::Visibility::Public {
            res.headers_mut().insert(
                axum::http::HeaderName::from_static("x-robots-tag"),
                HeaderValue::from_static("noindex, nofollow"),
            );
        }
        res
    })
    .await
}
