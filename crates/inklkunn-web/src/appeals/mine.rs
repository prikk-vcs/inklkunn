//! *My appeals* (RFC 006 D-9; handoff G-T5): the appeals a person made in a wiki, **their own and nobody else's**, with what each
//! said and how it ended, and what they may contest now, each with its *contest this* link. The page is the appellant's: it is the
//! one place besides the deciders' page where an appeal's words are read.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::appeal::{AppealRow, Contestable};

use super::paths::{self, fields};
use super::words;
use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, serve};
use crate::time::utc_minutes;
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// What a person may contest: the sentences, the link to the proposal when it is about one, and the link that contests it (or
/// the sentence that says they have).
pub(crate) struct ContestView {
    pub(crate) lines: Vec<String>,
    pub(crate) proposal: Option<LinkView>,
    pub(crate) contest: Option<LinkView>,
    pub(crate) filed: Option<String>,
}

/// One of the person's appeals.
struct AppealView {
    head: String,
    facts: Vec<Rich>,
    said: Vec<String>,
}

#[derive(Template)]
#[template(path = "appeals_mine.html")]
struct MineTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    note: Option<String>,
    contest_heading: String,
    contest_none: String,
    contestable: Vec<ContestView>,
    appeals_heading: String,
    appeals_none: String,
    appeals: Vec<AppealView>,
    decide: Option<LinkView>,
}

pub(crate) fn contest_view(t: T, slug: &str, c: &Contestable) -> ContestView {
    let (lines, proposal) = words::what_lines(t, slug, &c.what);
    ContestView {
        lines,
        proposal,
        contest: c
            .filed
            .is_none()
            .then(|| words::contest_link(t, slug, &c.contest)),
        filed: c.filed.as_ref().map(|_| t.get(Key::ContestFiled)),
    }
}

fn appeal_view(t: T, slug: &str, row: &AppealRow) -> AppealView {
    let mut facts = vec![crate::catalog::plain(
        &t.msg(Key::MineFiledOn)
            .with("kind", words::kind_label(t, row.action.kind))
            .with("date", utc_minutes(row.at))
            .text(),
    )];
    match &row.decision {
        Some(decision) => {
            facts.push(crate::catalog::plain(&words::decision_line(t, decision)));
            facts.push(words::decision_authority(t, slug, decision));
        }
        None => facts.push(crate::catalog::plain(&t.get(Key::MineOpen))),
    }
    AppealView {
        head: words::kind_label(t, row.action.kind),
        facts,
        said: words::text_lines(&row.text),
    }
}

/// `GET /{slug}/-/appeals/mine`.
pub(crate) async fn mine_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: PathName,
    params: Params,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        let t = page.t();
        let view = match services.appeals.mine(&ctx.principal, &ctx.wiki) {
            Ok(view) => view,
            Err(error) => return super::refused(page, &ctx, &error, Vec::<Choice>::new()),
        };
        let slug = ctx.slug();
        let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
        let shell = Shell::of(&ctx.wiki);
        let rendered = MineTpl {
            page: &head,
            shell: &shell,
            title: t
                .msg(Key::TitleAppealsMine)
                .with("wiki", ctx.wiki.display_name.as_str())
                .product()
                .text(),
            heading: t.get(Key::AppealsMineLink),
            intro: t.get(Key::MineIntro),
            note: (params.get(fields::DONE).map(String::as_str) == Some(fields::DONE_FILED))
                .then(|| t.get(Key::ContestDoneFiled)),
            contest_heading: t.get(Key::MineContestHeading),
            contest_none: t.get(Key::MineContestNone),
            contestable: view
                .contestable
                .iter()
                .map(|c| contest_view(t, slug, c))
                .collect(),
            appeals_heading: t.get(Key::MineAppealsHeading),
            appeals_none: t.get(Key::MineAppealsNone),
            appeals: view
                .appeals
                .iter()
                .map(|row| appeal_view(t, slug, row))
                .collect(),
            decide: services
                .appeals
                .may_decide(&ctx.principal, &ctx.wiki)
                .then(|| LinkView {
                    href: paths::page(slug),
                    label: t.get(Key::AppealsHeading),
                }),
        }
        .render();
        answer(StatusCode::OK, &head, rendered)
    })
    .await
}
