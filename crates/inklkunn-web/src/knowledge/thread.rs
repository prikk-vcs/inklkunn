//! A claim's thread on its page, and the form that adds a comment to it (RFC 005 part 2 D-11; requirement KN-7; handoff
//! 005 K-T10): the **epistemic discussion** of a claim, labelled *Discussion of this claim*, with the proposal thread's
//! records and rules.
//!
//! - **A discussion never changes a claim**, and the page says so in one sentence beside the thread. Nothing here reaches
//!   a claim file, a status or a source: the form posts a comment to the service's own records and nothing else
//!   (`ProposalService::comment_on_claim`), and a status changes only through an accepted curation proposal (D-3).
//! - **Authorized as pages are**: the claim page begins as every handler of a wiki does (a wiki the visitor may not read
//!   is a wiki that is not there), and the thread is read through the service's `Read` check. The form is offered to who
//!   may comment on a proposal, an account with the `Comment` action, and to nobody else; the post is checked again by the
//!   service, behind the CSRF layer.
//! - **Existence-neutral**: a thread nobody has written in is an empty thread, so reading it tells nothing the claim's page
//!   did not; a claim the wiki does not hold is the page that says so, and a post to it is that page too.
//! - **Every text is escaped** (the template prints every name and body as letters), **bounded** (a comment is at most the
//!   size of a proposal's, a page of the thread holds [`PAGE`] comments) and **paginated**: *Show the next comments* leads
//!   on, by the comment the page ended at. The thread's records are append-only, so a comment's number never changes.
//! - **Plain forms, no script**: a comment is a post of a form with the session's CSRF token; a reply is a link that opens
//!   the same form with the comment it answers.

use std::collections::HashMap;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use inklkunn_core::Principal;
use inklkunn_core::knowledge::ClaimId;
use inklkunn_core::proposal::{NewClaimComment, ProposalError};
use inklkunn_store::{AccountId, ClaimCommentRow, ClaimThreadPage, CommentId, WikiRecord};

use super::paths;
use crate::catalog::{Key, Rich, T};
use crate::csrf::Decided;
use crate::pages::{Page, PageState};
use crate::proposals::render::{CommentView, LinkView};
use crate::proposals::{
    Ctx, fields, fields_of, from_this_site, needs_account, refused, see, serve, urlencoded,
};
use crate::services::Services;
use crate::time::utc_minutes;
use crate::url::view_ref;
use crate::wiki::{Choice, generic_not_found, services_of};

#[cfg(test)]
mod tests;

/// How many comments a page of a thread holds.
pub(crate) const PAGE: usize = 50;

/// The form that adds a comment.
pub(crate) struct Form {
    pub(crate) action: String,
    pub(crate) label: String,
    /// Said above the field when the comment answers another.
    pub(crate) about: Option<String>,
    /// The fields the form carries besides the text: the view, and the comment it answers.
    pub(crate) hidden: Vec<(&'static str, String)>,
    pub(crate) button: String,
    pub(crate) body_name: &'static str,
    pub(crate) body_id: &'static str,
}

/// What the page shows of a claim's thread.
pub(crate) struct Section {
    pub(crate) heading: String,
    /// *A discussion never changes a claim*, in one sentence.
    pub(crate) never_changes: String,
    pub(crate) total: Option<String>,
    pub(crate) none: Option<String>,
    pub(crate) comments: Vec<CommentView>,
    pub(crate) first: Option<LinkView>,
    pub(crate) more: Option<LinkView>,
    pub(crate) unavailable: Option<String>,
    pub(crate) form: Option<Form>,
}

/// The thread as read for a request: what is needed to draw it, or that it could not be read.
pub(crate) struct Read {
    page: Option<ClaimThreadPage>,
    /// The comment this page begins after; `None` on the first page.
    after: Option<CommentId>,
    /// The comment the form replies to.
    reply: Option<ClaimCommentRow>,
    can_comment: bool,
    /// The display names of the people who wrote, by account id; `None` for one the service does not know.
    names: HashMap<String, Option<String>>,
    /// Where a comment this page answers is, when it is on an earlier page, by its place in the thread.
    parents: HashMap<u32, Parent>,
}

/// A comment on an earlier page, and the comment its page follows (`None` for the first page).
struct Parent {
    id: CommentId,
    after: Option<CommentId>,
}

impl Read {
    /// A thread that could not be read.
    pub(crate) fn none() -> Read {
        Read {
            page: None,
            after: None,
            reply: None,
            can_comment: false,
            names: HashMap::new(),
            parents: HashMap::new(),
        }
    }
}

/// Where the comments that this page's replies answer are, when they are on an earlier page (K42): for each, the comment
/// and the comment its page follows, by two lookups of the thread's key, made once per place and per page. A reply
/// page holds at most [`PAGE`] comments, so at most that many places are looked up; one that cannot be found is left
/// as the sentence without a link.
fn earlier_parents(
    services: &Services,
    principal: &Principal,
    wiki: &WikiRecord,
    claim: &ClaimId,
    page: &ClaimThreadPage,
) -> HashMap<u32, Parent> {
    let place = |position: u64| {
        services
            .proposals
            .claim_thread_place(principal, wiki, claim, position)
            .ok()
            .flatten()
    };
    let per_page = u32::try_from(PAGE).unwrap_or(u32::MAX);
    let mut found: HashMap<u32, Parent> = HashMap::new();
    let mut followed: HashMap<u32, Option<CommentId>> = HashMap::new();
    let Some(first) = page.rows.first().map(|r| r.position) else {
        return found;
    };
    for position in page
        .rows
        .iter()
        .filter_map(|r| r.parent_position)
        .filter(|p| *p < first)
    {
        if found.contains_key(&position) {
            continue;
        }
        // The comments before the parent's page: every full page of them.
        let before = (position - 1) / per_page * per_page;
        let after = followed
            .entry(before)
            .or_insert_with(|| (before > 0).then(|| place(u64::from(before))).flatten())
            .clone();
        let Some(id) = place(u64::from(position)) else {
            continue;
        };
        if before > 0 && after.is_none() {
            continue;
        }
        found.insert(position, Parent { id, after });
    }
    found
}

/// Read a page of the thread of `claim` for the request in `headers`. The query's `after` and `reply` name comments; one
/// that is not in this thread is not followed (the first page, no reply).
pub(crate) fn read(
    services: &Services,
    headers: &HeaderMap,
    wiki: &WikiRecord,
    claim: &ClaimId,
    params: &HashMap<String, String>,
) -> Read {
    let principal = services.identity.principal(headers);
    let asked = |after: Option<&CommentId>| {
        services
            .proposals
            .claim_thread(&principal, wiki, claim, after, PAGE)
    };
    let after = params.get(paths::Q_AFTER).and_then(|a| CommentId::parse(a));
    let (page, after) = match asked(after.as_ref()) {
        Ok(page) => (Some(page), after),
        // A comment that is not in this thread is no place to continue from.
        Err(ProposalError::NotFound) => (asked(None).ok(), None),
        Err(_) => (None, None),
    };
    // A reply is to a comment on the page the form is drawn with: a reply link is made on a page of the thread.
    let reply = params
        .get(fields::Q_REPLY)
        .and_then(|r| CommentId::parse(r))
        .and_then(|id| {
            page.as_ref()
                .and_then(|p| p.rows.iter().find(|r| r.comment.id == id).cloned())
        });
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    for row in page.iter().flat_map(|p| p.rows.iter()) {
        let author: &AccountId = &row.comment.author;
        names.entry(author.to_string()).or_insert_with(|| {
            match services.accounts.account(author) {
                Ok(Some(account)) => Some(account.display_name.as_str().to_owned()),
                _ => None,
            }
        });
    }
    let parents = page.as_ref().map_or_else(HashMap::new, |p| {
        earlier_parents(services, &principal, wiki, claim, p)
    });
    Read {
        can_comment: page.is_some()
            && services
                .proposals
                .may_comment_on_claim(&principal, wiki, claim),
        page,
        after,
        reply,
        names,
        parents,
    }
}

/// The page's section: the label, the one sentence, the comments of this page in order, the way on, and the form for
/// whoever may use it.
pub(crate) fn section(
    t: T,
    read: &Read,
    wiki: &WikiRecord,
    claim: &str,
    view: Option<&str>,
) -> Section {
    let heading = t.get(Key::ClaimThreadHeading);
    let never_changes = t.get(Key::ClaimThreadNeverChanges);
    let Some(page) = &read.page else {
        return Section {
            heading,
            never_changes,
            total: None,
            none: None,
            comments: Vec::new(),
            first: None,
            more: None,
            unavailable: Some(t.get(Key::ClaimThreadUnavailable)),
            form: None,
        };
    };
    let slug = wiki.slug.as_str();
    let after = read.after.as_ref().map(CommentId::as_str);
    let name = |row: &ClaimCommentRow| match read.names.get(row.comment.author.as_str()) {
        Some(Some(name)) => name.clone(),
        _ => t.get(Key::PersonUnknown),
    };
    // The sentence that names the comment a reply answers, linked to it: on this page by a fragment, on an earlier page by
    // that page's address and the fragment. Without a place to link to it is the sentence alone (`location`).
    let answers = |row: &ClaimCommentRow| -> Option<Rich> {
        let position = row.parent_position?;
        let here = page.rows.iter().find(|r| r.position == position);
        let href = match (here, read.parents.get(&position)) {
            // A message's link is a path, never a bare fragment: the page's own address, and the comment.
            (Some(parent), _) => format!(
                "{}#k-{}",
                paths::claim_page(slug, claim, view, after, None),
                parent.comment.id
            ),
            (None, Some(parent)) => format!(
                "{}#k-{}",
                paths::claim_page(
                    slug,
                    claim,
                    view,
                    parent.after.as_ref().map(CommentId::as_str),
                    None
                ),
                parent.id
            ),
            (None, None) => return None,
        };
        let label = t
            .msg(Key::ClaimThreadCommentLabel)
            .with("number", position)
            .text();
        Some(
            t.msg(Key::ClaimThreadInReplyToLink)
                .link("comment", label, href)
                .html(),
        )
    };
    let comments = page
        .rows
        .iter()
        .map(|row| {
            let author = name(row);
            CommentView {
                parent: answers(row),
                id: format!("k-{}", row.comment.id),
                depth: 0,
                meta: t
                    .msg(Key::ClaimThreadMeta)
                    .with("number", row.position)
                    .with("when", utc_minutes(row.comment.created_at))
                    .text(),
                location: row
                    .parent_position
                    .map(|n| t.msg(Key::ClaimThreadInReplyTo).with("number", n).text()),
                body: row.comment.body.clone(),
                quote: None,
                orphaned: None,
                reply: read.can_comment.then(|| LinkView {
                    href: paths::reply(slug, claim, view, after, row.comment.id.as_str()),
                    label: t.msg(Key::CommentReply).with("author", &author).text(),
                }),
                author,
            }
        })
        .collect();
    let form = read.can_comment.then(|| {
        let mut hidden = Vec::new();
        if let Some(view) = view {
            hidden.push((fields::VIEW, view.to_owned()));
        }
        let mut about = None;
        if let Some(parent) = &read.reply {
            hidden.push((fields::PARENT, parent.comment.id.to_string()));
            about = Some(
                t.msg(Key::ClaimThreadFormReply)
                    .with("number", parent.position)
                    .text(),
            );
        }
        Form {
            action: paths::discuss(slug, claim),
            label: t.get(Key::ClaimThreadFormLabel),
            about,
            hidden,
            button: t.get(Key::CommentFormButton),
            body_name: fields::BODY,
            body_id: paths::COMMENT_FIELD_ID,
        }
    });
    Section {
        heading,
        never_changes,
        total: (page.total > 0).then(|| t.msg(Key::ClaimThreadTotal).n(page.total).text()),
        none: (page.total == 0).then(|| t.get(Key::ClaimThreadNone)),
        comments,
        first: read.after.is_some().then(|| LinkView {
            href: paths::claim_page(slug, claim, view, None, None),
            label: t.get(Key::ClaimThreadFirst),
        }),
        more: page.next.as_ref().map(|next| LinkView {
            href: paths::claim_page(slug, claim, view, Some(next.as_str()), None),
            label: t.get(Key::ClaimThreadMore),
        }),
        unavailable: None,
        form,
    }
}

// ------------------------------------------------------------------------------------ the comment

type Fields = urlencoded::Raw;
type Ids = Result<Path<(String, String)>, PathRejection>;

/// `POST /{slug}/-/claim/{id}/discuss`: add a comment to the claim's thread. A form from this site (the CSRF layer
/// decided who the request is), a wiki the visitor may read, a claim id, an account, the text; then the service
/// authorizes `Comment` itself, checks that the claim is there in the view, and applies the comment limit.
pub(crate) async fn comment_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
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
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        let Some(claim) = ClaimId::parse(&id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let fields = match fields_of(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let view_name = fields
            .get(fields::VIEW)
            .filter(|v| !v.is_empty())
            .map(String::as_str);
        let full = match view_name {
            None => None,
            Some(v) => match view_ref(v) {
                Some(full) => Some(full),
                None => return generic_not_found(page),
            },
        };
        let parent = match fields.get(fields::PARENT).filter(|p| !p.is_empty()) {
            None => None,
            Some(p) => match CommentId::parse(p) {
                Some(p) => Some(p),
                None => return generic_not_found(page),
            },
        };
        let new = NewClaimComment {
            body: fields.get(fields::BODY).cloned().unwrap_or_default(),
            parent,
        };
        let back = || {
            vec![Choice {
                label: page.get(Key::ClaimThreadBack),
                href: paths::claim(ctx.slug(), &id, view_name),
            }]
        };
        match ctx.services.proposals.comment_on_claim(
            &ctx.principal,
            &ctx.wiki,
            full.as_deref(),
            &claim,
            new,
        ) {
            Ok(comment) => {
                // The new comment is the last of the thread: the page that has it is the last page.
                let after = last_page(&ctx, &claim);
                see(&format!(
                    "{}#k-{}",
                    paths::claim_page(
                        ctx.slug(),
                        &id,
                        view_name,
                        after.as_ref().map(CommentId::as_str),
                        None
                    ),
                    comment.id
                ))
            }
            Err(error) => refused(page, &ctx, &error, back()),
        }
    })
    .await
}

/// The comment that the last page of the thread begins after (`None` when the thread has one page).
fn last_page(ctx: &Ctx, claim: &ClaimId) -> Option<CommentId> {
    // Two lookups however long the thread is (handoff 005 K42): its count, and the comment the last page follows.
    ctx.services
        .proposals
        .claim_thread_last_page(&ctx.principal, &ctx.wiki, claim, PAGE)
        .ok()
        .flatten()
}
