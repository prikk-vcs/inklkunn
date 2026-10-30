//! The proposal's page (RFC 004 D-1, D-3; WEB-04): its state in the glossary's words, who proposed it and
//! when, what it changes page by page and part by part, the discussion with comments anchored to parts, the
//! decisions with the revision each is bound to, and the actions the viewer's role and the state allow.
//!
//! **Revisions.** The page shows the latest revision. A comment keeps the anchor it was made with and is
//! **re-anchored every time the page is drawn** ([`super::anchor::land`]); one whose part is gone is shown as
//! *orphaned* with the text it was about. A decision is shown with the revision it was made on ("on revision
//! 1"); once a newer revision exists the page says the decision no longer counts toward accepting it.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::proposal::ProposalError;
use inklkunn_store::{
    CommentRecord, DecisionKind, DecisionRecord, ProposalId, ProposalOp, ProposalRecord,
    ProposalState, ProposalTransition, RevisionContent, RevisionRecord,
};

use super::anchor::{Landing, land};
use super::changes::delta;
use super::render::{
    Beside, ChangesView, CommentView, LinkView, PageInput, build, on_parse, on_value,
};
use super::{Ctx, fields, parse_id, paths, refused, serve, words};
use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::time::utc_minutes;
use crate::wiki::{Shell, generic_not_found, services_of};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

// ------------------------------------------------------------------------------------ the view

struct DecisionView {
    line: String,
    note: Option<String>,
    body: String,
}

struct CommentFormView {
    action: String,
    label: String,
    about: Option<String>,
    hidden: Vec<(String, String)>,
    button: String,
    body_name: &'static str,
}

struct DecideFormView {
    action: String,
    label: String,
    body_name: &'static str,
    revision_name: &'static str,
    revision: u32,
    op_name: &'static str,
    approve_op: &'static str,
    approve: String,
    request_op: &'static str,
    request: String,
}

struct AcceptView {
    action: String,
    button: String,
    note: String,
}

#[derive(Template)]
#[template(path = "proposal.html")]
struct ProposalTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    state: String,
    notes: Vec<Rich>,
    facts: Vec<String>,
    sources_heading: String,
    sources: Vec<String>,
    signed: Option<(String, Option<LinkView>)>,
    actions: Vec<LinkView>,
    accept: Option<AcceptView>,
    changes_heading: String,
    changes: Option<ChangesView>,
    thread_id: &'static str,
    thread_heading: String,
    thread: Vec<CommentView>,
    no_thread: Option<String>,
    comment_id: &'static str,
    comment_form: Option<CommentFormView>,
    decisions_heading: String,
    decisions: Vec<DecisionView>,
    no_decisions: Option<String>,
    decide: Option<DecideFormView>,
    list: LinkView,
}

// ------------------------------------------------------------------------------------ comments

/// Who may be named, and the people the comments mention, looked up once.
struct Cast<'a> {
    ctx: &'a Ctx,
    t: T,
}

impl Cast<'_> {
    fn name(&self, id: &inklkunn_store::AccountId) -> String {
        self.ctx.person(self.t, id)
    }
}

/// A thread: a comment and its replies, in order, with where it belongs.
struct Thread {
    views: Vec<CommentView>,
    /// Beside the part at this (page path, ordinal) when it is shown there.
    beside: Option<(String, usize)>,
}

fn comment_view(
    cast: &Cast<'_>,
    c: &CommentRecord,
    depth: usize,
    reply: Option<LinkView>,
    location: Option<String>,
    quote: Option<String>,
    orphaned: Option<String>,
) -> CommentView {
    let t = cast.t;
    CommentView {
        id: format!("k-{}", c.id),
        depth: depth.min(3),
        author: cast.name(&c.author),
        meta: t
            .msg(Key::CommentMeta)
            .with("when", utc_minutes(c.created_at))
            .with("number", c.revision)
            .text(),
        body: c.body.clone(),
        location,
        quote,
        orphaned,
        reply,
    }
}

/// The threads of a proposal, each placed where its first comment lands on the latest text.
fn threads(
    cast: &Cast<'_>,
    slug: &str,
    id: &ProposalId,
    comments: &[CommentRecord],
    files: &[(String, String, Option<String>)],
    shown: &dyn Fn(&str, usize) -> bool,
    may_reply: bool,
) -> Vec<Thread> {
    let t = cast.t;
    let reply_link = |c: &CommentRecord| {
        may_reply.then(|| LinkView {
            href: paths::proposal_with(
                slug,
                id.as_str(),
                fields::Q_REPLY,
                c.id.as_str(),
                fields::ID_COMMENT,
            ),
            label: t
                .msg(Key::CommentReply)
                .with("author", cast.name(&c.author))
                .text(),
        })
    };
    let mut out = Vec::new();
    for root in comments.iter().filter(|c| c.parent.is_none()) {
        let mut location = None;
        let mut quote = None;
        let mut orphaned = None;
        let mut beside = None;
        if let Some(anchor) = &root.anchor {
            let page = files.iter().find(|(path, _, _)| *path == anchor.page);
            let title = page.map_or_else(|| anchor.page.clone(), |(_, title, _)| title.clone());
            let landing = page
                .and_then(|(_, _, text)| text.as_deref())
                .map_or(Landing::Orphaned, |text| land(anchor, text));
            quote = anchor.quote.as_ref().map(|q| q.exact.clone());
            match landing {
                Landing::At(o) if shown(&anchor.page, o) => {
                    beside = Some((anchor.page.clone(), o));
                    quote = None;
                }
                Landing::At(o) => {
                    location = Some(
                        t.msg(Key::CommentOnPart)
                            .with("number", o + 1)
                            .with("title", &title)
                            .text(),
                    );
                }
                Landing::Orphaned => {
                    orphaned = Some(t.msg(Key::CommentOrphaned).with("title", &title).text());
                }
            }
        }
        let mut views = vec![comment_view(
            cast,
            root,
            0,
            reply_link(root),
            location,
            quote,
            orphaned,
        )];
        // Replies, depth first, in the order they were written.
        let mut stack: Vec<(&CommentRecord, usize)> = vec![(root, 0)];
        let mut order: Vec<(&CommentRecord, usize)> = Vec::new();
        while let Some((parent, depth)) = stack.pop() {
            let mut children: Vec<&CommentRecord> = comments
                .iter()
                .filter(|c| c.parent.as_ref() == Some(&parent.id))
                .collect();
            children.sort_by_key(|c| std::cmp::Reverse((c.created_at, c.id.to_string())));
            for child in children {
                stack.push((child, depth + 1));
            }
            if depth > 0 {
                order.push((parent, depth));
            }
        }
        for (reply, depth) in order {
            views.push(comment_view(
                cast,
                reply,
                depth,
                reply_link(reply),
                None,
                None,
                None,
            ));
        }
        out.push(Thread { views, beside });
    }
    out
}

/// The comments the changes show beside a part.
struct Placed {
    links: Vec<(usize, usize, LinkView)>,
    threads: Vec<Thread>,
}

impl Beside for Placed {
    fn comment_link(&self, index: usize, ordinal: usize) -> Option<LinkView> {
        self.links
            .iter()
            .find(|(i, o, _)| *i == index && *o == ordinal)
            .map(|(_, _, link)| LinkView {
                href: link.href.clone(),
                label: link.label.clone(),
            })
    }

    fn comments_at(&self, path: &str, ordinal: usize) -> Vec<CommentView> {
        // Comments are moved out once by `take_beside`; this only reads a copy of what was placed.
        self.threads
            .iter()
            .filter(|t| {
                t.beside
                    .as_ref()
                    .is_some_and(|(p, o)| p == path && *o == ordinal)
            })
            .flat_map(|t| t.views.iter().map(copy_comment))
            .collect()
    }
}

fn copy_comment(c: &CommentView) -> CommentView {
    CommentView {
        id: c.id.clone(),
        depth: c.depth,
        author: c.author.clone(),
        meta: c.meta.clone(),
        body: c.body.clone(),
        location: c.location.clone(),
        quote: c.quote.clone(),
        orphaned: c.orphaned.clone(),
        reply: c.reply.as_ref().map(|l| LinkView {
            href: l.href.clone(),
            label: l.label.clone(),
        }),
    }
}

// ------------------------------------------------------------------------------------ the page

/// `GET /{slug}/-/proposals/{id}`.
pub(crate) async fn proposal(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, id))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&id) else {
            return generic_not_found(page);
        };
        draw(page, &ctx, &id, &params, StatusCode::OK)
    })
    .await
}

/// What the page needs from the records, read once.
struct Records {
    record: ProposalRecord,
    revisions: Vec<RevisionRecord>,
    content: Option<RevisionContent>,
    comments: Vec<CommentRecord>,
    decisions: Vec<DecisionRecord>,
    transitions: Vec<ProposalTransition>,
}

fn records(ctx: &Ctx, id: &ProposalId) -> Result<Records, ProposalError> {
    let service = &ctx.services.proposals;
    let (p, w) = (&ctx.principal, &ctx.wiki);
    let record = service.get(p, w, id)?;
    let revisions = service.revisions(p, w, id)?;
    let content = match (record.state, revisions.last()) {
        (_, Some(latest)) => Some(latest.content.clone()),
        (ProposalState::Draft, None) => Some(service.draft(p, w, id)?),
        _ => None,
    };
    Ok(Records {
        comments: service.comments(p, w, id)?,
        decisions: service.decisions(p, w, id)?,
        transitions: service.transitions(p, w, id)?,
        record,
        revisions,
        content,
    })
}

/// The proposal's page, with `status`.
pub(crate) fn draw(
    page: &Page,
    ctx: &Ctx,
    id: &ProposalId,
    params: &HashMap<String, String>,
    status: StatusCode,
) -> Response {
    let r = match records(ctx, id) {
        Ok(r) => r,
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    };
    let t = page.t();
    let slug = ctx.slug();
    let cast = Cast { ctx, t };
    let record = &r.record;
    let latest = r.revisions.last().map(|rev| rev.seq);
    let open_for_talk = matches!(
        record.state,
        ProposalState::HeldForReview | ProposalState::NeedsRevision
    );
    let is_author = ctx.account() == Some(&record.author);
    let can_comment = open_for_talk && ctx.may_on(Action::Comment, id);
    let can_review = record.state == ProposalState::HeldForReview && ctx.may_on(Action::Review, id);
    let can_accept = ctx.may_on(Action::Accept, id);

    // The pages: what the proposal holds, against what it was written on.
    let mut files: Vec<(String, String, Option<String>)> = Vec::new();
    let mut olds: Vec<Option<String>> = Vec::new();
    if let Some(content) = &r.content {
        for f in &content.files {
            let old = match ctx
                .services
                .read
                .source_at(&ctx.wiki, &content.base.block_id, &f.path)
            {
                Ok(old) => old,
                Err(e) => return crate::proposals::new::read_failure(page, ctx, e),
            };
            files.push((
                f.path.clone(),
                inklkunn_markup::path::path_to_title(&f.path).unwrap_or_else(|| f.path.clone()),
                f.text.clone(),
            ));
            olds.push(old);
        }
    }
    let shown = |path: &str, ordinal: usize| -> bool {
        files.iter().zip(&olds).any(|((p, _, new), old)| {
            p == path
                && delta(old.as_deref(), new.as_deref())
                    .parts
                    .iter()
                    .any(|part| part.ordinal == Some(ordinal))
        })
    };
    let all = threads(&cast, slug, id, &r.comments, &files, &shown, can_comment);
    let (inline, general): (Vec<Thread>, Vec<Thread>) =
        all.into_iter().partition(|th| th.beside.is_some());
    let mut links = Vec::new();
    if can_comment {
        for (index, (_, _, new)) in files.iter().enumerate() {
            let Some(text) = new else { continue };
            let old = olds.get(index).and_then(|o| o.as_deref());
            for part in delta(old, Some(text)).parts {
                if let Some(ordinal) = part.ordinal {
                    links.push((
                        index,
                        ordinal,
                        LinkView {
                            href: paths::proposal_with(
                                slug,
                                id.as_str(),
                                fields::Q_ON,
                                &on_value(index, ordinal),
                                fields::ID_COMMENT,
                            ),
                            label: t
                                .msg(Key::CommentOnThisPart)
                                .with("number", ordinal + 1)
                                .text(),
                        },
                    ));
                }
            }
        }
    }
    let placed = Placed {
        links,
        threads: inline,
    };
    let inputs: Vec<PageInput<'_>> = files
        .iter()
        .zip(&olds)
        .map(|((path, title, new), old)| PageInput {
            path,
            title: title.clone(),
            old: old.as_deref(),
            new: new.as_deref(),
        })
        .collect();
    let changes = build(t, &inputs, &placed);

    // The state, in words, and what has been said about it.
    let mut notes = vec![
        t.msg(words::state_note(record.state))
            .with("author", ctx.person(t, &record.author))
            .html(),
    ];
    for tr in &r.transitions {
        let key = match tr.op {
            ProposalOp::Decline => Key::ReasonDeclined,
            ProposalOp::Withdraw => Key::ReasonWithdrawn,
            ProposalOp::AskForRevision => Key::ReasonAsked,
            _ => continue,
        };
        if let Some(reason) = &tr.reason {
            notes.push(
                t.msg(key)
                    .with("person", ctx.person(t, &tr.actor))
                    .with("reason", reason)
                    .html(),
            );
        }
    }
    let superseded = record.superseded_by.as_ref().map(|new| {
        let label = ctx
            .services
            .proposals
            .get(&ctx.principal, &ctx.wiki, new)
            .map_or_else(|_| t.get(Key::ProposalAnother), |p| p.summary);
        t.msg(Key::SupersededBy)
            .link("proposal", label, paths::proposal(slug, new.as_str()))
            .html()
    });
    let mut facts = vec![
        t.msg(Key::FactProposedBy)
            .with("author", ctx.person(t, &record.author))
            .with(
                "when",
                utc_minutes(record.submitted_at.unwrap_or(record.created_at)),
            )
            .text(),
    ];
    facts.push(
        t.msg(Key::FactPages)
            .n(r.content.as_ref().map_or(0, |c| c.files.len() as u64))
            .text(),
    );
    if let Some(n) = latest {
        facts.push(
            t.msg(Key::FactRevision)
                .with("number", n)
                .with("total", r.revisions.len())
                .text(),
        );
    }
    if let Some(view) = record
        .view
        .strip_prefix("heads/")
        .filter(|_| record.view != ctx.wiki.default_view)
    {
        facts.push(t.msg(Key::FactView).with("view", view).text());
    }
    let signed = (record.state == ProposalState::Accepted).then(|| {
        let block = latest.and_then(|n| {
            ctx.services
                .acceptances
                .acceptances_of(id, n)
                .ok()?
                .into_iter()
                .find_map(|e| match e.result?.outcome {
                    inklkunn_store::AcceptanceOutcome::Sealed { block_id, .. } => Some(block_id),
                    inklkunn_store::AcceptanceOutcome::Failed { .. } => None,
                })
        });
        (
            t.get(Key::OutcomeSigned),
            block.map(|b| LinkView {
                href: crate::url::change_path(slug, None, &b),
                label: t.get(Key::SignedChangeLink),
            }),
        )
    });

    // The actions the viewer's role and the state allow.
    let mut actions = Vec::new();
    let link = |label: Key, act: &str| LinkView {
        href: paths::act(slug, id.as_str(), act),
        label: t.get(label),
    };
    if is_author {
        match record.state {
            ProposalState::Draft => actions.push(link(Key::ActionContinue, paths::ACT_EDIT)),
            ProposalState::HeldForReview | ProposalState::NeedsRevision => {
                actions.push(link(Key::ActionRevise, paths::ACT_EDIT));
            }
            _ => {}
        }
        if record.state.is_open() {
            actions.push(link(Key::ActionWithdraw, paths::ACT_WITHDRAW));
        }
    }
    if can_accept && record.state == ProposalState::HeldForReview {
        actions.push(link(Key::ActionAsk, paths::ACT_ASK));
    }
    if can_accept
        && matches!(
            record.state,
            ProposalState::HeldForReview | ProposalState::NeedsRevision
        )
    {
        actions.push(link(Key::ActionDecline, paths::ACT_DECLINE));
    }
    let accept = (can_accept && record.state == ProposalState::HeldForReview).then(|| AcceptView {
        action: paths::act(slug, id.as_str(), paths::ACT_ACCEPT),
        button: t.get(Key::ActionAccept),
        note: t.get(Key::AcceptNotYetNote),
    });

    // The comment form: about a part, or a reply, or neither.
    let comment_form = can_comment.then(|| {
        let mut hidden = Vec::new();
        let mut about = None;
        if let Some((index, ordinal)) = params.get(fields::Q_ON).and_then(|v| on_parse(v))
            && let Some((path, title, Some(_))) = files.get(index)
            && let Some(rev) = latest
        {
            hidden.push((fields::ANCHOR_PAGE.to_owned(), path.clone()));
            hidden.push((fields::ANCHOR_ORDINAL.to_owned(), ordinal.to_string()));
            hidden.push((fields::ANCHOR_REVISION.to_owned(), rev.to_string()));
            about = Some(
                t.msg(Key::CommentFormAboutPart)
                    .with("number", ordinal + 1)
                    .with("title", title)
                    .text(),
            );
        }
        if let Some(parent) = params
            .get(fields::Q_REPLY)
            .and_then(|v| r.comments.iter().find(|c| c.id.as_str() == v))
        {
            hidden.push((fields::PARENT.to_owned(), parent.id.to_string()));
            about = Some(
                t.msg(Key::CommentFormReplyTo)
                    .with("author", ctx.person(t, &parent.author))
                    .text(),
            );
        }
        CommentFormView {
            action: paths::act(slug, id.as_str(), paths::ACT_COMMENT),
            label: t.get(Key::CommentFormLabel),
            about,
            hidden,
            button: t.get(Key::CommentFormButton),
            body_name: fields::BODY,
        }
    });

    // The decisions, each with the revision it is bound to.
    let decisions: Vec<DecisionView> = r
        .decisions
        .iter()
        .map(|d| DecisionView {
            line: t
                .msg(match d.kind {
                    DecisionKind::Approve => Key::DecisionApproved,
                    DecisionKind::RequestChanges => Key::DecisionRequested,
                })
                .with("reviewer", ctx.person(t, &d.reviewer))
                .with("number", d.revision)
                .text(),
            note: latest
                .is_some_and(|n| d.revision < n)
                .then(|| t.get(Key::DecisionOlder)),
            body: d.body.clone(),
        })
        .collect();
    let decide = (can_review).then(|| DecideFormView {
        action: paths::act(slug, id.as_str(), paths::ACT_DECIDE),
        label: t.get(Key::DecideLabel),
        body_name: fields::BODY,
        revision_name: fields::REVISION,
        revision: latest.unwrap_or(1),
        op_name: fields::OP,
        approve_op: fields::OP_APPROVE,
        approve: t.get(Key::DecideApprove),
        request_op: fields::OP_REQUEST_CHANGES,
        request: t.get(Key::DecideRequest),
    });

    if let Some(rich) = superseded {
        notes.push(rich);
    }
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let summary = if record.summary.is_empty() {
        t.get(Key::ProposalNoSummary)
    } else {
        record.summary.clone()
    };
    let rendered = ProposalTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleProposal)
            .with("summary", &summary)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: summary,
        state: t.get(words::state_label(record.state)),
        notes,
        facts,
        sources_heading: t.get(Key::SourcesHeading),
        sources: record.sources.clone(),
        signed,
        actions,
        accept,
        changes_heading: t.get(Key::ProposalChangesHeading),
        changes: Some(changes),
        thread_id: fields::ID_THREAD,
        thread_heading: t.get(Key::ThreadHeading),
        thread: general.into_iter().flat_map(|th| th.views).collect(),
        no_thread: r.comments.is_empty().then(|| t.get(Key::ThreadNone)),
        comment_id: fields::ID_COMMENT,
        comment_form,
        decisions_heading: t.get(Key::DecisionsHeading),
        no_decisions: decisions.is_empty().then(|| t.get(Key::DecisionsNone)),
        decisions,
        decide,
        list: LinkView {
            href: paths::list(slug),
            label: t.get(Key::ProposalsBackToList),
        },
    }
    .render();
    answer(status, &head, rendered)
}
