//! A wiki's proposals, and a page's discussion (RFC 004 D-3; WEB-04, WEB-05).
//!
//! The list shows what the visitor may see: every proposal that was sent for review, and the visitor's own
//! drafts (a draft is its author's alone, so to anyone else it is not in the list at all). The discussion of a
//! page is what has been said about it so far: the proposals that touch it, each with its thread. A talk page
//! of its own (threads that belong to no proposal, resolvable) needs a record this release does not keep; the
//! page says what it shows and does not pretend to more.

use askama::Template;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::proposal::ProposalService;
use inklkunn_markup::path::title_to_path;
use inklkunn_store::{CommentRecord, ProposalFilter, ProposalRecord, ProposalState};

use super::render::{CommentView, LinkView};
use super::{Ctx, paths, serve, words};
use crate::catalog::{Key, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::time::utc_minutes;
use crate::wiki::{Shell, generic_not_found, services_of};

/// The most proposals a list or a discussion goes through.
const MAX_LISTED: usize = 500;
/// The most proposals a discussion reads the threads of.
const MAX_DISCUSSED: usize = 100;

struct Row {
    href: String,
    summary: String,
    state: String,
    byline: String,
}

struct Section {
    id: &'static str,
    heading: String,
    rows: Vec<Row>,
}

#[derive(Template)]
#[template(path = "proposals.html")]
struct ListTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    empty: Option<String>,
    sections: Vec<Section>,
    propose: Option<LinkView>,
}

fn row(ctx: &Ctx, t: T, p: &ProposalRecord) -> Row {
    Row {
        href: paths::proposal(ctx.slug(), p.id.as_str()),
        summary: if p.summary.is_empty() {
            t.get(Key::ProposalNoSummary)
        } else {
            p.summary.clone()
        },
        state: t.get(words::state_label(p.state)),
        byline: t
            .msg(Key::ProposalsRowBy)
            .with("author", ctx.person(t, &p.author))
            .with("when", utc_minutes(p.updated_at))
            .text(),
    }
}

/// `GET /{slug}/-/proposals`.
pub(crate) async fn list(
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
        let mut all =
            match ctx
                .services
                .proposals
                .list(&ctx.principal, &ctx.wiki, &ProposalFilter::default())
            {
                Ok(all) => all,
                Err(error) => return super::refused(page, &ctx, &error, Vec::new()),
            };
        all.sort_by_key(|p| std::cmp::Reverse((p.updated_at, p.id.to_string())));
        all.truncate(MAX_LISTED);
        let mine = ctx.account();
        let section = |id, key, pick: &dyn Fn(&ProposalRecord) -> bool| {
            let rows: Vec<Row> = all
                .iter()
                .filter(|p| pick(p))
                .map(|p| row(&ctx, t, p))
                .collect();
            (!rows.is_empty()).then(|| Section {
                id,
                heading: t.get(key),
                rows,
            })
        };
        let sections: Vec<Section> = [
            section("drafts", Key::ProposalsDrafts, &|p| {
                p.state == ProposalState::Draft && mine == Some(&p.author)
            }),
            section("open", Key::ProposalsOpen, &|p| {
                matches!(
                    p.state,
                    ProposalState::HeldForReview | ProposalState::NeedsRevision
                )
            }),
            section("closed", Key::ProposalsClosed, &|p| {
                matches!(
                    p.state,
                    ProposalState::Accepted
                        | ProposalState::Declined
                        | ProposalState::Withdrawn
                        | ProposalState::Superseded
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        let shell = Shell::of(&ctx.wiki);
        let head = page.wiki_head(&ctx.wiki, Subject::Surface("-/proposals"));
        let rendered = ListTpl {
            page: &head,
            shell: &shell,
            title: t
                .msg(Key::TitleProposals)
                .with("wiki", ctx.wiki.display_name.as_str())
                .product()
                .text(),
            heading: t.get(Key::ProposalsHeading),
            intro: t
                .msg(Key::ProposalsIntro)
                .with("wiki", ctx.wiki.display_name.as_str())
                .text(),
            empty: sections.is_empty().then(|| t.get(Key::ProposalsEmpty)),
            sections,
            propose: ctx.may_wiki(Action::Propose).then(|| LinkView {
                href: paths::new_page(ctx.slug(), None),
                label: t.get(Key::ProposalsNewPage),
            }),
        }
        .render();
        answer(StatusCode::OK, &head, rendered)
    })
    .await
}

// ------------------------------------------------------------------------------------ a page's discussion

struct Discussed {
    heading: LinkView,
    state: String,
    comments: Vec<CommentView>,
}

#[derive(Template)]
#[template(path = "discussion.html")]
struct DiscussionTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    discussed: Vec<Discussed>,
    empty: Option<String>,
    cut: Option<String>,
    page_link: LinkView,
    propose: Option<LinkView>,
}

/// Does a proposal touch the page at `path`?
fn touches(service: &ProposalService, ctx: &Ctx, p: &ProposalRecord, path: &str) -> bool {
    let content = if p.state == ProposalState::Draft {
        service.draft(&ctx.principal, &ctx.wiki, &p.id).ok()
    } else {
        service
            .revisions(&ctx.principal, &ctx.wiki, &p.id)
            .ok()
            .and_then(|mut r| r.pop())
            .map(|r| r.content)
    };
    content.is_some_and(|c| {
        c.files.iter().any(|f| f.path == path)
            || c.renames.iter().any(|r| r.from == path || r.to == path)
    })
}

fn flatten(ctx: &Ctx, t: T, comments: &[CommentRecord]) -> Vec<CommentView> {
    comments
        .iter()
        .map(|c| CommentView {
            id: format!("d-{}", c.id),
            depth: usize::from(c.parent.is_some()),
            author: ctx.person(t, &c.author),
            meta: t
                .msg(Key::CommentMeta)
                .with("when", utc_minutes(c.created_at))
                .with("number", c.revision)
                .text(),
            body: c.body.clone(),
            location: None,
            quote: None,
            orphaned: None,
            reply: None,
        })
        .collect()
}

/// `GET /{slug}/-/discussion/{*title}`.
pub(crate) async fn discussion(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Response {
    let (Ok(Path((slug, title))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let t = page.t();
        let Ok(path) = title_to_path(title.trim()) else {
            return crate::wiki::present(
                page,
                crate::wiki::Shown::InvalidTitle {
                    wiki: Box::new(ctx.wiki.clone()),
                },
            );
        };
        let service = &ctx.services.proposals;
        let mut all = match service.list(&ctx.principal, &ctx.wiki, &ProposalFilter::default()) {
            Ok(all) => all,
            Err(error) => return super::refused(page, &ctx, &error, Vec::new()),
        };
        all.sort_by_key(|p| std::cmp::Reverse((p.updated_at, p.id.to_string())));
        let cut = all.len() > MAX_DISCUSSED;
        let mut discussed = Vec::new();
        for p in all.iter().take(MAX_DISCUSSED) {
            if !touches(service, &ctx, p, &path) {
                continue;
            }
            let comments = service
                .comments(&ctx.principal, &ctx.wiki, &p.id)
                .unwrap_or_default();
            discussed.push(Discussed {
                heading: LinkView {
                    href: paths::proposal(ctx.slug(), p.id.as_str()),
                    label: if p.summary.is_empty() {
                        t.get(Key::ProposalNoSummary)
                    } else {
                        p.summary.clone()
                    },
                },
                state: t.get(words::state_label(p.state)),
                comments: flatten(&ctx, t, &comments),
            });
        }
        let shell = Shell::of(&ctx.wiki);
        let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
        let title = title.trim().to_owned();
        let rendered = DiscussionTpl {
            page: &head,
            shell: &shell,
            title: t
                .msg(Key::TitleDiscussion)
                .with("page", &title)
                .with("wiki", ctx.wiki.display_name.as_str())
                .product()
                .text(),
            heading: t.msg(Key::DiscussionHeading).with("page", &title).text(),
            intro: t.get(Key::DiscussionIntro),
            empty: discussed.is_empty().then(|| t.get(Key::DiscussionEmpty)),
            discussed,
            cut: cut.then(|| {
                t.msg(Key::DiscussionCut)
                    .with("limit", MAX_DISCUSSED)
                    .text()
            }),
            page_link: LinkView {
                href: format!("/{}/{}", ctx.slug(), crate::url::encode_title(&title)),
                label: t.msg(Key::DiscussionPage).with("page", &title).text(),
            },
            propose: ctx.may_wiki(Action::Propose).then(|| LinkView {
                href: paths::change_page(ctx.slug(), &title, None),
                label: t.get(Key::DiscussionPropose),
            }),
        }
        .render();
        answer(StatusCode::OK, &head, rendered)
    })
    .await
}
