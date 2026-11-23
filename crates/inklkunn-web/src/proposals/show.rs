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
use super::knowledge::{Place, file_lines};
use super::overlaps::OverlapsView;
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
    /// The role the reviewer held and the rules the decision was made under (RFC 006 D-5; criterion 12), or that it was not
    /// recorded.
    authority: Rich,
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

struct ReconfirmView {
    action: String,
    hint: String,
    button: String,
}

struct AcceptView {
    action: String,
    revision_name: &'static str,
    revision: u32,
    button: String,
    /// Said beside the button only when this server does not accept.
    note: Option<String>,
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
    /// What the proposer may contest about the proposal (RFC 006 D-9): the decline of it, a temporary protection that holds it
    /// back. Said to the proposer alone.
    contests: Vec<crate::appeals::mine::ContestView>,
    facts: Vec<String>,
    /// The protections in force on the pages the proposal touches, in the one display.
    protections: Vec<crate::protections::NoticeView>,
    sources_heading: String,
    sources: Vec<String>,
    signed: Option<(String, Option<LinkView>)>,
    /// Who accepted it, in which role, under which rules, and whether as an admin without approvals (RFC 006 D-5, D-4).
    accepted: Option<Rich>,
    actions: Vec<LinkView>,
    accept: Option<AcceptView>,
    overlaps: Option<OverlapsView>,
    governance: Option<super::governance_view::GovernanceView>,
    changes_heading: String,
    changes: Option<ChangesView>,
    know_heading: String,
    know_notes: Vec<String>,
    /// Where a change that needs a curation proposal is made: the curation form of each claim it touches, for those
    /// who may propose (K39).
    know_curate: crate::knowledge::curate::Offers,
    know: Vec<super::knowledge::Line>,
    touch_heading: String,
    touch_intro: String,
    touch: Vec<super::touched::Line>,
    reconfirm: Option<ReconfirmView>,
    reconfirm_later: Option<String>,
    reconfirm_open: Option<Rich>,
    reconfirm_done: Option<String>,
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
    own_proposal: Option<String>,
    list: LinkView,
}

// ------------------------------------------------------------------------------------ comments

/// Who may be named, and the people the comments mention, looked up once.
struct Cast<'a> {
    ctx: &'a Ctx,
    t: T,
    /// What moderation hides of the comments (RFC 006 D-8), by comment.
    hidden: &'a HashMap<inklkunn_store::CommentId, inklkunn_core::tombstone::Hidden>,
    /// May the person hide or restore a comment? Offered to the `moderate` rung alone.
    may_moderate: bool,
    proposal: &'a ProposalId,
}

impl Cast<'_> {
    fn name(&self, id: &inklkunn_store::AccountId) -> String {
        self.ctx.person(self.t, id)
    }

    /// The name of a comment's author: **a hidden author is named nowhere**.
    fn author_of(&self, c: &CommentRecord) -> String {
        crate::tombstone::author_of(self.t, || self.name(&c.author), self.hidden.get(&c.id))
    }

    /// *Contest this*, for the author of a comment moderation hid: the link of the first hide of it the person may appeal.
    fn contest_link(&self, c: &CommentRecord) -> Option<LinkView> {
        crate::appeals::words::tombstone_contest(self.ctx, self.t, self.hidden.get(&c.id))
    }

    /// The link that hides or restores a comment, offered to the `moderate` rung.
    fn moderate_link(&self, c: &CommentRecord) -> Option<LinkView> {
        self.may_moderate.then(|| LinkView {
            href: crate::tombstone::paths::form_for(
                self.ctx.slug(),
                &inklkunn_core::tombstone::Item::ProposalComment {
                    proposal: self.proposal.clone(),
                    comment: c.id.clone(),
                },
            ),
            label: self.t.get(Key::TombstoneModerateLink),
        })
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
    let (body_hidden, tombstone) = crate::tombstone::comment_state(t, cast.hidden.get(&c.id));
    CommentView {
        body_hidden,
        tombstone,
        moderate: cast.moderate_link(c),
        contest: cast.contest_link(c),
        id: format!("k-{}", c.id),
        depth: depth.min(3),
        author: cast.author_of(c),
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
        parent: None,
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
                .with("author", cast.author_of(c))
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
        body_hidden: c.body_hidden,
        tombstone: c.tombstone.clone(),
        moderate: c.moderate.as_ref().map(|l| LinkView {
            href: l.href.clone(),
            label: l.label.clone(),
        }),
        contest: c.contest.as_ref().map(|l| LinkView {
            href: l.href.clone(),
            label: l.label.clone(),
        }),
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
        parent: c.parent.clone(),
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
    /// The declines that a hide made: drawn as *declined by moderation*, never with the moderator's name (D-8).
    moderation_declines: Vec<inklkunn_core::tombstone::ModerationDecline>,
    /// What moderation hides of the proposal wherever it is named: its summary and its author.
    hidden: inklkunn_core::tombstone::ProposalHidden,
    /// What moderation hides of each revision, with its number.
    revision_marks: Vec<(u32, inklkunn_core::tombstone::Hidden)>,
    /// What moderation hides of each comment that has something hidden.
    comment_marks: HashMap<inklkunn_store::CommentId, inklkunn_core::tombstone::Hidden>,
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
        hidden: service.hidden_of(p, w, id)?,
        revision_marks: service.revision_marks(p, w, id)?,
        comment_marks: service.comment_marks(p, w, id)?,
        comments: service.comments(p, w, id)?,
        decisions: service.decisions(p, w, id)?,
        transitions: service.transitions(p, w, id)?,
        moderation_declines: ctx
            .services
            .moderation
            .declines_by_moderation(w, id)
            .map_err(|e| {
                ProposalError::Store(inklkunn_store::StoreError::Backend(e.to_string()))
            })?,
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
    let may_moderate = ctx
        .services
        .moderation
        .may_moderate(&ctx.principal, &ctx.wiki);
    let cast = Cast {
        ctx,
        t,
        hidden: &r.comment_marks,
        may_moderate,
        proposal: id,
    };
    // The name of the proposer: **a hidden author is named nowhere** (RFC 006 D-8).
    let proposer = if r.hidden.author.is_some() {
        crate::tombstone::author_word(t)
    } else {
        ctx.person(t, &r.record.author)
    };
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
    // The hiding of the text the proposal was written against, when moderation hides it (RFC 006 D-8; H-1): that text is not shown,
    // and the change is shown without it, with the tombstone.
    let mut base_hidden: Option<inklkunn_core::tombstone::Mark> = None;
    if let Some(content) = &r.content {
        // The pages; the claim and source files are listed on their own (RFC 005 part 2 D-3).
        for f in content.files.iter().filter(|f| {
            inklkunn_core::knowledge::family_of(&f.path).is_none()
                // The rules are said as before and after sentences ([`super::governance_view`]), never as the text of a file.
                && inklkunn_core::governance::GovernanceFile::at_or_below(&f.path).is_none()
        }) {
            let old =
                match ctx
                    .services
                    .read
                    .source_or_hidden(&ctx.wiki, &content.base.block_id, &f.path)
                {
                    Ok(Ok(old)) => old,
                    Ok(Err(mark)) => {
                        base_hidden.get_or_insert(*mark);
                        None
                    }
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
            .with("author", &proposer)
            .html(),
    ];
    for tr in &r.transitions {
        // A decline that a hide made names no one and quotes nothing the moderator wrote: it says *by moderation* and why.
        if let Some(by_moderation) = r
            .moderation_declines
            .iter()
            .find(|m| tr.op == ProposalOp::Decline && m.step == *tr)
        {
            notes.push(
                t.msg(Key::ReasonDeclinedByModeration)
                    .with(
                        "category",
                        crate::tombstone::category_word(t, by_moderation.category),
                    )
                    .html(),
            );
            continue;
        }
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
            .map_or_else(
                |_| t.get(Key::ProposalAnother),
                |p| {
                    if p.summary.is_empty() {
                        t.get(Key::TombstoneSummaryHidden)
                    } else {
                        p.summary
                    }
                },
            );
        t.msg(Key::SupersededBy)
            .link("proposal", label, paths::proposal(slug, new.as_str()))
            .html()
    });
    let mut facts = vec![
        t.msg(Key::FactProposedBy)
            .with("author", &proposer)
            .with(
                "when",
                utc_minutes(record.submitted_at.unwrap_or(record.created_at)),
            )
            .text(),
    ];
    facts.push(t.msg(Key::FactPages).n(files.len() as u64).text());
    if let Some(mark) = &base_hidden {
        facts.push(t.get(Key::TombstoneDiffFrom));
        facts.extend(crate::tombstone::mark_facts(t, mark));
    }
    // What moderation hides of the revisions, in place of it: the text of a hidden revision is not shown above, and each
    // hiding says why, which role and when (RFC 006 D-8). **A name is never in it.**
    for (seq, hidden) in &r.revision_marks {
        if hidden.content().is_some() {
            facts.push(
                t.msg(Key::TombstoneRevisionHidden)
                    .with("number", *seq)
                    .text(),
            );
        }
        facts.extend(crate::tombstone::facts(t, hidden));
    }
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
    // The acceptance that signed it: its change, and who accepted it, as which role, under which rules.
    let acceptance = (record.state == ProposalState::Accepted)
        .then(|| {
            latest.and_then(|n| {
                ctx.services
                    .acceptances
                    .acceptances_of(id, n)
                    .ok()?
                    .into_iter()
                    .find_map(|e| match e.result?.outcome {
                        inklkunn_store::AcceptanceOutcome::Sealed { block_id, .. } => {
                            Some((block_id, e.acceptance))
                        }
                        inklkunn_store::AcceptanceOutcome::Failed { .. } => None,
                    })
            })
        })
        .flatten();
    let signed = (record.state == ProposalState::Accepted).then(|| {
        (
            t.get(Key::OutcomeSigned),
            acceptance.as_ref().map(|(b, _)| LinkView {
                href: crate::url::change_path(slug, None, b),
                label: t.get(Key::SignedChangeLink),
            }),
        )
    });
    let accepted_by = acceptance.as_ref().map(|(_, a)| {
        t.msg(if a.hatch {
            Key::AuthorityAcceptedHatch
        } else {
            Key::AuthorityAccepted
        })
        .with("person", ctx.person(t, &a.acceptor))
        .with(
            "role",
            crate::people::sentences::role_label(t, a.acceptor_role),
        )
        .rich(
            "authority",
            crate::authority::under(t, slug, a.policy_block.as_deref()),
        )
        .html()
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
    // The `moderate` rung hides or restores a revision here (RFC 006 D-8): one link for each, named by its number.
    if may_moderate {
        for revision in &r.revisions {
            actions.push(LinkView {
                href: crate::tombstone::paths::form_for(
                    slug,
                    &inklkunn_core::tombstone::Item::ProposalRevision {
                        proposal: id.clone(),
                        seq: revision.seq,
                    },
                ),
                label: t
                    .msg(Key::ModerateItemProposalRevision)
                    .with("seq", revision.seq)
                    .with("id", id.as_str().chars().take(12).collect::<String>())
                    .text(),
            });
        }
    }
    let accept = (can_accept && record.state == ProposalState::HeldForReview).then(|| AcceptView {
        action: paths::act(slug, id.as_str(), paths::ACT_ACCEPT),
        revision_name: fields::REVISION,
        revision: latest.unwrap_or(1),
        button: t.get(Key::ActionAccept),
        note: match ctx.services.acceptor.as_deref() {
            None => Some(t.get(Key::AcceptNotServedNote)),
            Some(acceptor) if !acceptor.is_configured() => Some(t.get(Key::AcceptUnconfiguredNote)),
            Some(_) => None,
        },
    });
    let overlaps = super::overlaps::view(ctx, t, record, latest);

    // The claims the change touches, and what it does to what they were judged on (part 1 D-1 c).
    let pages: Vec<(String, Option<String>, Option<String>)> = files
        .iter()
        .zip(&olds)
        .map(|((path, _, new), old)| (path.clone(), old.clone(), new.clone()))
        .collect();
    // Once accepted, the claims are those of **the revision that was accepted**, whatever the page shows below.
    let accepted = match record.state {
        ProposalState::Accepted => match super::touched::accepted_change(ctx, id) {
            Ok(found) => found,
            Err(error) => return refused(page, ctx, &error, Vec::new()),
        },
        _ => None,
    };
    let touched = match (&accepted, &r.content) {
        (Some(change), _) => {
            match super::touched::of_accepted(ctx, &record.view, &change.content) {
                Ok(found) => found,
                Err(super::touched::Unfound::Proposal(error)) => {
                    return refused(page, ctx, &error, Vec::new());
                }
                Err(super::touched::Unfound::Read(e)) => {
                    return crate::proposals::new::read_failure(page, ctx, e);
                }
            }
        }
        (None, Some(content)) if !pages.is_empty() => {
            match super::touched::of_revision(ctx, &record.view, &content.base.block_id, &pages) {
                Ok(found) => found,
                Err(e) => return crate::proposals::new::read_failure(page, ctx, e),
            }
        }
        _ => Vec::new(),
    };
    let leaves = super::touched::leaves_earlier(&touched);
    let step = match &accepted {
        Some(change) if leaves => match super::touched::offer(ctx, &change.acceptance, &touched) {
            Ok(found) => Some(found),
            Err(error) => return refused(page, ctx, &error, Vec::new()),
        },
        _ => None,
    };
    let reconfirm = (matches!(step, Some(super::touched::Offer::Offered(_)))
        && ctx.account().is_some()
        && ctx.may_wiki(Action::Propose))
    .then(|| ReconfirmView {
        action: paths::act(slug, id.as_str(), paths::ACT_RECONFIRM),
        hint: t.get(Key::TouchReconfirmHint),
        button: t.get(Key::TouchReconfirmButton),
    });
    let reconfirm_later =
        (leaves && record.state.is_open()).then(|| t.get(Key::TouchReconfirmLater));
    let reconfirm_open = match &step {
        Some(super::touched::Offer::Open(open)) => {
            let label = ctx
                .services
                .proposals
                .get(&ctx.principal, &ctx.wiki, open)
                .map_or_else(
                    |_| t.get(Key::ProposalAnother),
                    |p| {
                        if p.summary.is_empty() {
                            t.get(Key::TombstoneSummaryHidden)
                        } else {
                            p.summary
                        }
                    },
                );
            Some(
                t.msg(Key::TouchReconfirmOpen)
                    .link("proposal", label, paths::proposal(slug, open.as_str()))
                    .html(),
            )
        }
        _ => None,
    };
    let reconfirm_done =
        matches!(step, Some(super::touched::Offer::Done)).then(|| t.get(Key::TouchReconfirmDone));
    let touch = super::touched::lines(t, &touched);

    // What the proposal does to claims and sources, and whether it is a curation proposal.
    let (know, know_notes, know_curate) = match &r.content {
        Some(content) => {
            let place = Place {
                view: Some(&record.view),
                base: &content.base,
                renames: &content.renames,
            };
            let known = match file_lines(ctx, t, &place, &content.files, &record.id.to_string()) {
                Ok(found) => found,
                Err(e) => return crate::proposals::new::read_failure(page, ctx, e),
            };
            let mut notes = Vec::new();
            let mut offered = crate::knowledge::curate::Offers::default();
            let has_knowledge_files = content
                .files
                .iter()
                .any(|f| inklkunn_core::knowledge::family_of(&f.path).is_some());
            if known.lines.is_empty() {
                // An empty list says so (K5): a page never shows nothing where there are files.
                if has_knowledge_files {
                    notes.push(t.get(Key::KnowNothingChanges));
                }
            } else if inklkunn_core::knowledge::curation::is_curation(content) {
                notes.push(t.get(Key::KnowIsCuration));
            } else if !known.curated.is_empty() {
                // The acceptance's own rule (K7): the same sentences the submission is refused with. What a form makes
                // is said to be for a curation proposal; what no form makes, to be not offered in this version (K44,
                // K62), with no reviewer or form named: each said once (K67).
                notes.extend(crate::knowledge::curate::sentences(t, &known.curated));
                // The way to where the change is proposed (K39): the curation form of each claim it touches.
                offered =
                    match crate::knowledge::curate::offers(ctx, t, &record.view, &known.curated) {
                        Ok(offered) => offered,
                        Err(e) => return crate::proposals::new::read_failure(page, ctx, e),
                    };
            }
            // A new link judged on words its claim does not have (K25): a draft may hold one, and says so.
            if known.unjudged {
                notes.push(t.get(Key::KnowNotJudged));
            }
            // A source moved to another reference that still carries the check of the old one (K71, K78): the rule the
            // submission and the acceptance have, said before a reviewer accepts.
            if known.carried {
                notes.push(t.get(Key::KnowCheckUnstated));
            }
            (known.lines, notes, offered)
        }
        None => (Vec::new(), Vec::new(), Default::default()),
    };

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
                    .with("author", cast.author_of(parent))
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
            authority: match d.reviewer_rung {
                Some(role) => t
                    .msg(Key::AuthorityReviewed)
                    .with("role", crate::people::sentences::role_label(t, role))
                    .rich(
                        "authority",
                        crate::authority::under(t, slug, d.policy_block.as_deref()),
                    )
                    .html(),
                None => crate::catalog::plain(&t.get(Key::AuthorityUnrecorded)),
            },
            note: latest
                .is_some_and(|n| d.revision < n)
                .then(|| t.get(Key::DecisionOlder)),
            body: d.body.clone(),
        })
        .collect();
    // The proposer is not offered a button that can only be refused (corrections E1): the sentence the refusal
    // would give stands where the form would.
    let decide = (can_review && !is_author).then(|| DecideFormView {
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
    let own_proposal = (can_review && is_author).then(|| t.get(Key::RefusedOwnProposal));

    match super::touched::reconfirm_note(ctx, t, id) {
        Ok(found) => notes.extend(found),
        Err(error) => return refused(page, ctx, &error, Vec::new()),
    }
    // RFC 006 D-4: the rules of this wiki ask for more approvals than the people who could give them. Computed now, from the
    // rules and the grants as they are; a warning the page cannot compute is not a reason for the page to fail.
    if record.state == ProposalState::HeldForReview
        && let Ok(Some(warning)) = ctx.services.proposals.approvals_warning(&ctx.wiki, record)
    {
        notes.push(
            t.msg(Key::ProposalApprovalsUnreachable)
                .n(u64::from(warning.needed))
                .html(),
        );
    }
    if let Some(rich) = superseded {
        notes.push(rich);
    }
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let summary = if r.hidden.summary.is_some() {
        t.get(Key::TombstoneSummaryHidden)
    } else if record.summary.is_empty() {
        t.get(Key::ProposalNoSummary)
    } else {
        record.summary.clone()
    };
    // The protections of the pages this proposal touches, while it can still be accepted (RFC 006 D-7): what its acceptance will
    // ask, in the one display. Asked of the authorizer, which decides nothing here.
    let protections = if record.state.is_open() {
        let touched: Vec<String> = r
            .revisions
            .last()
            .map(|rev| {
                rev.content
                    .files
                    .iter()
                    .map(|f| f.path.clone())
                    .chain(
                        rev.content
                            .renames
                            .iter()
                            .flat_map(|x| [x.from.clone(), x.to.clone()]),
                    )
                    .collect()
            })
            .unwrap_or_default();
        crate::protections::views(
            t,
            ctx.slug(),
            &ctx.services
                .authorizer
                .protections_on_change(&ctx.principal, &ctx.wiki.id, &touched),
        )
    } else {
        Vec::new()
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
        contests: ctx
            .services
            .appeals
            .for_proposal(&ctx.principal, &ctx.wiki, id)
            .iter()
            .map(|c| crate::appeals::mine::contest_view(t, slug, c))
            .collect(),
        facts,
        protections,
        sources_heading: t.get(Key::SourcesHeading),
        sources: record.sources.clone(),
        signed,
        accepted: accepted_by,
        actions,
        accept,
        governance: r
            .content
            .as_ref()
            .and_then(|content| super::governance_view::view(ctx, t, content)),
        overlaps,
        changes_heading: t.get(Key::ProposalChangesHeading),
        changes: Some(changes),
        know_heading: t.get(Key::KnowHeading),
        know_notes,
        know_curate,
        know,
        touch_heading: t.get(Key::TouchHeading),
        touch_intro: t.get(Key::TouchIntro),
        touch,
        reconfirm,
        reconfirm_later,
        reconfirm_open,
        reconfirm_done,
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
        own_proposal,
        list: LinkView {
            href: paths::list(slug),
            label: t.get(Key::ProposalsBackToList),
        },
    }
    .render();
    answer(status, &head, rendered)
}
