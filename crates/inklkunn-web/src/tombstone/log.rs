//! The wiki's moderation log (RFC 006 D-8; threat model C-14f): each hide and each restoring, newest first, **with the item, the
//! category, the role and the date, never the hidden content and never a name**, to whoever may read the wiki. A person who may
//! not read the wiki is answered as for a wiki that is not there. The note is shown to the `moderate` rung only, and so is the
//! button that restores.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::appeal::AppealMark;
use inklkunn_core::tombstone::LogRow;
use inklkunn_store::{HiddenItemKind, TombstoneAct, TombstoneEventId};

use super::paths::{self, fields};
use super::{category_word, part_words};
use crate::catalog::{Key, T};
use crate::discover::Subject;
use crate::history::LinkView;
use crate::pages::{Page, PageState, answer};
use crate::proposals::{Ctx, serve};
use crate::protections::rung_word;
use crate::time::utc_minutes;
use crate::url::change_path;
use crate::wiki::Choice;
use crate::wiki::{Shell, generic_not_found, notice, services_of, with_retry_after};

/// How many rows a page of the log holds.
const PAGE_ROWS: usize = 50;

type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

struct Restore {
    action: String,
    field: &'static str,
    id: String,
    label: String,
}

struct RowView {
    head: String,
    link: Option<LinkView>,
    facts: Vec<String>,
    declined: Option<String>,
    /// For a hide that was appealed: **that it was, and how it ended**, in a sentence, and nothing else (RFC 006 D-9). For a
    /// restoring that an appeal's overturn made: that it was.
    appeal: Option<String>,
    note: Option<String>,
    restore: Option<Restore>,
}

#[derive(Template)]
#[template(path = "moderation_log.html")]
struct LogTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    none: String,
    rows: Vec<RowView>,
    older: Option<LinkView>,
}

/// What a row is about, in the reader's words: the kind and the start of the identifier. No content.
fn item_head(t: T, row: &LogRow) -> String {
    let short: String = row.item.id.chars().take(12).collect();
    let item = match row.item.kind {
        HiddenItemKind::PageRevision => t
            .msg(Key::ModerateItemPageRevision)
            .with("id", short)
            .text(),
        HiddenItemKind::ProposalRevision => t
            .msg(Key::ModerateItemProposalRevision)
            .with("seq", row.item.reference.as_deref().unwrap_or("?"))
            .with("id", short)
            .text(),
        HiddenItemKind::ProposalComment => t
            .msg(Key::ModerateItemProposalComment)
            .with("id", short)
            .text(),
        HiddenItemKind::ClaimComment => t
            .msg(Key::ModerateItemClaimComment)
            .with("id", short)
            .text(),
    };
    let key = match row.act {
        TombstoneAct::Hide => Key::LogHidden,
        TombstoneAct::Unhide => Key::LogRestored,
    };
    t.msg(key).with("item", item).text()
}

fn row_view(t: T, slug: &str, row: &LogRow, may_restore: bool) -> RowView {
    let facts = part_words(t, row.parts)
        .into_iter()
        .map(|part| match (row.act, row.category) {
            (TombstoneAct::Hide, Some(category)) => t
                .msg(Key::TombstoneFact)
                .with("part", part)
                .with("category", category_word(t, category))
                .with("role", rung_word(t, row.rung))
                .with("date", utc_minutes(row.at))
                .text(),
            _ => t
                .msg(Key::TombstoneRestoredFact)
                .with("part", part)
                .with("role", rung_word(t, row.rung))
                .with("date", utc_minutes(row.at))
                .text(),
        })
        .collect();
    let link = (row.item.kind == HiddenItemKind::PageRevision).then(|| LinkView {
        href: change_path(slug, None, &row.item.id),
        label: String::new(),
    });
    RowView {
        head: item_head(t, row),
        link,
        facts,
        declined: row.declined.then(|| t.get(Key::LogDeclined)),
        appeal: match (row.appeal, row.by_appeal) {
            (Some(mark), _) => Some(t.get(match mark {
                AppealMark::Appealed => Key::LogAppealed,
                AppealMark::Upheld => Key::LogAppealUpheld,
                AppealMark::Overturned => Key::LogAppealOverturned,
            })),
            (None, true) => Some(t.get(Key::LogRestoredByAppeal)),
            (None, false) => None,
        },
        note: row
            .note
            .as_ref()
            .map(|n| t.msg(Key::LogNote).with("note", n).text()),
        restore: (may_restore && row.act == TombstoneAct::Hide && row.in_force).then(|| Restore {
            action: paths::unhide(slug),
            field: fields::HIDE,
            id: row.id.to_string(),
            label: t.get(Key::ModerateUnhide),
        }),
    }
}

/// `GET /{slug}/-/moderation`.
pub(crate) async fn moderation_log(
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
        let before = match params.get("before") {
            None => None,
            Some(id) => match TombstoneEventId::parse(id) {
                Some(id) => Some(id),
                None => return generic_not_found(page),
            },
        };
        let rows =
            match services
                .moderation
                .log(&ctx.principal, &ctx.wiki, before.as_ref(), PAGE_ROWS + 1)
            {
                Ok(rows) => rows,
                Err(inklkunn_core::tombstone::ModerationError::NotFound)
                | Err(inklkunn_core::tombstone::ModerationError::NoSuchHiding) => {
                    return generic_not_found(page);
                }
                Err(other) => {
                    ctx.log(&format!("the moderation log could not be read: {other}"));
                    return with_retry_after(notice(
                        page,
                        &ctx.wiki,
                        StatusCode::SERVICE_UNAVAILABLE,
                        Key::OutcomeUnavailable,
                        vec![page.get(Key::ModerateUnavailable)],
                        Vec::<Choice>::new(),
                        None,
                    ));
                }
            };
        let t = page.t();
        let slug = ctx.slug();
        let may_restore = services.moderation.may_moderate(&ctx.principal, &ctx.wiki);
        let more = rows.len() > PAGE_ROWS;
        let shown: Vec<&LogRow> = rows.iter().take(PAGE_ROWS).collect();
        let older = more.then(|| shown.last()).flatten().map(|last| LinkView {
            href: paths::log_before(slug, last.id.as_str()),
            label: t.get(Key::LogOlder),
        });
        let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
        let shell = Shell::of(&ctx.wiki);
        let rendered = LogTpl {
            page: &head,
            shell: &shell,
            title: t
                .msg(Key::TitleModerationLog)
                .with("wiki", ctx.wiki.display_name.as_str())
                .product()
                .text(),
            heading: t.get(Key::LogHeading),
            intro: t.get(Key::LogIntro),
            none: t.get(Key::LogNone),
            rows: shown
                .iter()
                .map(|row| row_view(t, slug, row, may_restore))
                .collect(),
            older,
        }
        .render();
        answer(StatusCode::OK, &head, rendered)
    })
    .await
}
