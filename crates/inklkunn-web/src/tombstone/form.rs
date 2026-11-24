//! The form of the `moderate` rung that hides parts of an item, the act that restores what a hide hid, and the act that shows
//! hidden content (RFC 006 D-8, D-12; handoff G-T4). **Web only**: moderation is a deliberate human act, and there is no write
//! API for it.
//!
//! **The order of a request is the order of every write form.** A form from this site, the CSRF token of the session (decided
//! once by the layer), a wiki the visitor may read (an absent one and a forbidden one are one page), a person with an account
//! and the role that moderates, and only then the fields. The acts are the core's
//! ([`ModerationService`](inklkunn_core::tombstone::ModerationService)): they authorize `moderate` again, in the audit trail,
//! write the event of the act before the record, and record. **Showing hidden content is a POST**: it writes an event in the
//! audit trail, so it is never a link a crawler, a prefetch or a pasted address can follow.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use inklkunn_core::read::{LaterHolders, MAX_HOLDER_CHECKS, MAX_HOLDER_PATHS};
use inklkunn_core::tombstone::{Item, MAX_NOTE_CHARS, ModerationError, Preview, Revealed};
use inklkunn_markup::path::path_to_title;
use inklkunn_store::{HiddenParts, ReasonCategory, TombstoneEventId};

use super::paths::{self, fields};
use super::{category_word, mark_facts};
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::history::LinkView;
use crate::pages::{Page, PageState, answer};
use crate::proposals::{Ctx, SMALL_FORM_LIMIT, from_this_site, needs_account, see, serve};
use crate::time::utc_minutes;
use crate::url::{at_path, change_path};
use crate::wiki::{Choice, Shell, generic_not_found, notice, services_of, with_retry_after};

type Raw = crate::proposals::urlencoded::Raw;
type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// The body bound of the forms: an item's address, a category and a note.
pub(crate) const FORM_LIMIT: usize = SMALL_FORM_LIMIT;

/// How a request that did not end at the form ended, in the fixed cases the words are written for.
enum Outcome {
    /// The visitor does not hold the role that moderates.
    NotAllowed,
    /// The records could not be used; nothing was changed.
    Unavailable,
}

fn outcome_page(page: &Page, ctx: &Ctx, outcome: &Outcome) -> Response {
    let t = page.t();
    let (status, heading, key) = match outcome {
        Outcome::NotAllowed => (
            StatusCode::FORBIDDEN,
            Key::OutcomeNotAllowed,
            Key::ModerateNotAllowed,
        ),
        Outcome::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            Key::OutcomeUnavailable,
            Key::ModerateUnavailable,
        ),
    };
    let response = notice(
        page,
        &ctx.wiki,
        status,
        heading,
        vec![t.get(key)],
        vec![Choice {
            label: t.get(Key::ModerateBack),
            href: paths::log(ctx.slug()),
        }],
        None,
    );
    match outcome {
        Outcome::Unavailable => with_retry_after(response),
        Outcome::NotAllowed => response,
    }
}

/// What was typed, kept to show the form again.
#[derive(Default, Clone)]
struct Typed {
    parts: HiddenParts,
    category: Option<ReasonCategory>,
    note: String,
}

struct Box1 {
    value: &'static str,
    label: String,
    checked: bool,
    disabled: bool,
}

struct Cat {
    value: &'static str,
    label: String,
    checked: bool,
}

/// What the form says of the later changes that still hold the text of the change (J-4).
struct Later {
    /// The sentence over the list, when there is a list.
    intro: Option<String>,
    /// The listed changes: a link to the page as the change left it.
    items: Vec<LinkView>,
    /// *and N more*, when more hold the lines than are listed.
    more: Option<String>,
    /// What was skipped or could not be said, one sentence each.
    notes: Vec<String>,
}

/// The later changes that hold the text, in words and links (never the text, never a name).
fn later_of(t: T, slug: &str, view: Option<&str>, later: &LaterHolders) -> Option<Later> {
    if later.is_silent() {
        return None;
    }
    let items: Vec<LinkView> = later
        .listed
        .iter()
        .map(|h| {
            let title = path_to_title(&h.path);
            LinkView {
                href: match &title {
                    Some(title) => at_path(slug, view, &h.block, title),
                    None => change_path(slug, view, &h.block),
                },
                label: match h.first_seen {
                    Some(at) => t
                        .msg(Key::ModerateLaterItem)
                        .with("path", &h.path)
                        .with("when", utc_minutes(at))
                        .text(),
                    None => h.path.clone(),
                },
            }
        })
        .collect();
    let mut notes = Vec::new();
    if later.unsettled {
        notes.push(t.get(Key::ModerateLaterUnsettled));
    }
    if later.hidden > 0 {
        notes.push(
            t.msg(Key::ModerateLaterHidden)
                .n(later.hidden as u64)
                .text(),
        );
    }
    if later.unchecked > 0 {
        notes.push(
            t.msg(if later.unchecked_at_least {
                Key::ModerateLaterUncheckedAtLeast
            } else {
                Key::ModerateLaterUnchecked
            })
            .n(later.unchecked as u64)
            .with("limit", MAX_HOLDER_CHECKS)
            .text(),
        );
    }
    if later.paths_unchecked > 0 {
        notes.push(
            t.msg(Key::ModerateLaterPagesUnchecked)
                .n(later.paths_unchecked as u64)
                .with("limit", MAX_HOLDER_PATHS)
                .text(),
        );
    }
    Some(Later {
        intro: (!items.is_empty()).then(|| t.get(Key::ModerateLaterIntro)),
        items,
        more: (later.more > 0).then(|| {
            t.msg(Key::ModerateLaterMore)
                .with("more", later.more)
                .text()
        }),
        notes,
    })
}

struct InForce {
    facts: Vec<String>,
    id: String,
}

#[derive(Template)]
#[template(path = "moderate_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    still_held: String,
    later_change: Option<String>,
    problems: Vec<String>,
    item_line: String,
    in_force_heading: String,
    in_force: Vec<InForce>,
    unhide_action: String,
    unhide_label: String,
    show_action: String,
    show_label: String,
    show_note: String,
    hide_field: &'static str,
    tip_notice: Option<String>,
    tip_link: Option<LinkView>,
    survives: Option<String>,
    later: Option<Later>,
    declines: Option<String>,
    action: String,
    item_fields: Vec<(&'static str, String)>,
    parts_legend: String,
    parts: Vec<Box1>,
    category_legend: String,
    category_field: &'static str,
    categories: Vec<Cat>,
    note_label: String,
    note_field: &'static str,
    typed_note: String,
    note_max: usize,
    hide_label: String,
    back: Choice,
}

/// The fields that name an item, to be posted again with every button of the page.
fn item_fields(item: &Item) -> Vec<(&'static str, String)> {
    match item {
        Item::PageRevision { block, view } => {
            let mut out = vec![
                (fields::KIND, fields::KIND_PAGE_REVISION.to_owned()),
                (fields::BLOCK, block.clone()),
            ];
            if let Some(view) = view {
                out.push((fields::VIEW, view.clone()));
            }
            out
        }
        Item::ProposalRevision { proposal, seq } => vec![
            (fields::KIND, fields::KIND_PROPOSAL_REVISION.to_owned()),
            (fields::PROPOSAL, proposal.to_string()),
            (fields::SEQ, seq.to_string()),
        ],
        Item::ProposalComment { proposal, comment } => vec![
            (fields::KIND, fields::KIND_PROPOSAL_COMMENT.to_owned()),
            (fields::PROPOSAL, proposal.to_string()),
            (fields::COMMENT, comment.to_string()),
        ],
        Item::ClaimComment { claim, comment } => vec![
            (fields::KIND, fields::KIND_CLAIM_COMMENT.to_owned()),
            (fields::CLAIM, claim.clone()),
            (fields::COMMENT, comment.to_string()),
        ],
    }
}

/// What the form is about, in a sentence.
fn item_line(t: T, item: &Item) -> String {
    let short = |id: &str| id.chars().take(12).collect::<String>();
    match item {
        Item::PageRevision { block, .. } => t
            .msg(Key::ModerateItemPageRevision)
            .with("id", short(block))
            .text(),
        Item::ProposalRevision { proposal, seq } => t
            .msg(Key::ModerateItemProposalRevision)
            .with("seq", *seq)
            .with("id", short(proposal.as_str()))
            .text(),
        Item::ProposalComment { comment, .. } => t
            .msg(Key::ModerateItemProposalComment)
            .with("id", short(comment.as_str()))
            .text(),
        Item::ClaimComment { comment, .. } => t
            .msg(Key::ModerateItemClaimComment)
            .with("id", short(comment.as_str()))
            .text(),
    }
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    item: &Item,
    preview: &Preview,
    typed: &Typed,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let slug = ctx.slug();
    let (tip_notice, tip_link) = match (&preview.tip, item) {
        (Some(ModerationError::TipContent { view }), Item::PageRevision { block, view: shown }) => {
            (
                Some(match view {
                    Some(view) => t.msg(Key::ModerateTipRefused).with("view", view).text(),
                    None => t.get(Key::ModerateTipUnchecked),
                }),
                Some(LinkView {
                    href: change_path(slug, shown.as_deref(), block),
                    label: t.get(Key::ModerateTipLink),
                }),
            )
        }
        _ => (None, None),
    };
    let parts = vec![
        Box1 {
            value: fields::PART_CONTENT,
            label: t.get(Key::ModerateLabelContent),
            checked: typed.parts.content && preview.tip.is_none(),
            disabled: preview.tip.is_some(),
        },
        Box1 {
            value: fields::PART_SUMMARY,
            label: t.get(Key::ModerateLabelSummary),
            checked: typed.parts.summary,
            disabled: !preview.summary,
        },
        Box1 {
            value: fields::PART_AUTHOR,
            label: t.get(Key::ModerateLabelAuthor),
            checked: typed.parts.author,
            disabled: false,
        },
    ];
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleModerate)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::ModerateHeading),
        intro: t.get(Key::ModerateIntro),
        still_held: t.get(Key::ModerateStillHeld),
        // What a hide of a change covers (H-5): this change, not text that a later change writes again.
        later_change: matches!(item, Item::PageRevision { .. })
            .then(|| t.get(Key::ModerateLaterChange)),
        problems,
        item_line: item_line(t, item),
        in_force_heading: t.get(Key::ModerateInForceHeading),
        in_force: preview
            .marks
            .iter()
            .map(|mark| InForce {
                facts: mark_facts(t, mark),
                id: mark.event.to_string(),
            })
            .collect(),
        unhide_action: paths::unhide(slug),
        unhide_label: t.get(Key::ModerateUnhide),
        show_action: paths::show(slug),
        show_label: t.get(Key::ModerateShow),
        show_note: t.get(Key::ModerateShowNote),
        hide_field: fields::HIDE,
        tip_notice,
        tip_link,
        survives: preview.survives.map(|s| {
            t.msg(if s.at_least {
                Key::ModerateSurvivesAtLeast
            } else {
                Key::ModerateSurvives
            })
            .n(s.lines as u64)
            .text()
        }),
        later: preview.later.as_ref().and_then(|later| {
            let view = match item {
                Item::PageRevision { view, .. } => view.as_deref(),
                _ => None,
            };
            later_of(t, slug, view, later)
        }),
        declines: preview.declines.then(|| t.get(Key::ModerateDeclines)),
        action: paths::form(slug),
        item_fields: item_fields(item),
        parts_legend: t.get(Key::ModeratePartsLegend),
        parts,
        category_legend: t.get(Key::ModerateCategoryLegend),
        category_field: fields::CATEGORY,
        categories: ReasonCategory::ALL
            .into_iter()
            .map(|c| Cat {
                value: c.as_str(),
                label: category_word(t, c),
                checked: typed.category == Some(c),
            })
            .collect(),
        note_label: t.get(Key::ModerateNoteLabel),
        note_field: fields::NOTE,
        typed_note: typed.note.clone(),
        note_max: MAX_NOTE_CHARS,
        hide_label: t.get(Key::ModerateHide),
        back: Choice {
            label: t.get(Key::ModerateBack),
            href: paths::log(slug),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// The item a request names, or the page every missing address gets.
fn item_of_params(params: &HashMap<String, String>) -> Option<Item> {
    paths::item_of(&|name| params.get(name).cloned())
}

/// What the core said of the item, or the page for it.
fn previewed(page: &Page, ctx: &Ctx, item: &Item) -> Result<Preview, Box<Response>> {
    ctx.services
        .moderation
        .preview(&ctx.principal, &ctx.wiki, item)
        .map_err(|e| Box::new(refused(page, ctx, &e)))
}

/// The page for a refusal that ends a request: no form to show again.
fn refused(page: &Page, ctx: &Ctx, error: &ModerationError) -> Response {
    match error {
        ModerationError::NotFound | ModerationError::NoSuchItem | ModerationError::NoSuchHiding => {
            generic_not_found(page)
        }
        ModerationError::NotAllowed | ModerationError::Restricted(_) => {
            outcome_page(page, ctx, &Outcome::NotAllowed)
        }
        other => {
            ctx.log(&format!("moderation could not be done: {other}"));
            outcome_page(page, ctx, &Outcome::Unavailable)
        }
    }
}

/// `GET /{slug}/-/moderate?kind=…`.
pub(crate) async fn moderate_form(
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
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !services.moderation.may_moderate(&ctx.principal, &ctx.wiki) {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        let Some(item) = item_of_params(&params) else {
            return generic_not_found(page);
        };
        let preview = match previewed(page, &ctx, &item) {
            Ok(p) => p,
            Err(response) => return *response,
        };
        form_page(
            page,
            &ctx,
            &item,
            &preview,
            &Typed::default(),
            Vec::new(),
            StatusCode::OK,
        )
    })
    .await
}

fn form_fields(
    page: &Page,
    ctx: &Ctx,
    headers: &HeaderMap,
    form: Raw,
) -> Result<HashMap<String, String>, Box<Response>> {
    crate::proposals::urlencoded::fields(headers, form).map_err(|status| {
        let key = if status == StatusCode::PAYLOAD_TOO_LARGE {
            Key::InvalidFormTooLarge
        } else {
            Key::InvalidIncomplete
        };
        Box::new(notice(
            page,
            &ctx.wiki,
            status,
            Key::RefusedInvalidHeading,
            vec![page.get(key)],
            vec![Choice {
                label: page.get(Key::ModerateBack),
                href: paths::log(ctx.slug()),
            }],
            None,
        ))
    })
}

/// `POST /{slug}/-/moderate`: hide.
pub(crate) async fn moderate_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !services.moderation.may_moderate(&ctx.principal, &ctx.wiki) {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let Some(item) = item_of_params(&given) else {
            return generic_not_found(page);
        };
        let chosen = |name: &str| given.get(name).is_some_and(|v| v == "1");
        let typed = Typed {
            parts: HiddenParts {
                content: chosen(fields::PART_CONTENT),
                summary: chosen(fields::PART_SUMMARY),
                author: chosen(fields::PART_AUTHOR),
            },
            category: given
                .get(fields::CATEGORY)
                .and_then(|c| ReasonCategory::parse(c)),
            note: given.get(fields::NOTE).cloned().unwrap_or_default(),
        };
        hide(page, &ctx, &item, &typed)
    })
    .await
}

fn hide(page: &Page, ctx: &Ctx, item: &Item, typed: &Typed) -> Response {
    let t = page.t();
    let again = |problem: String, status: StatusCode| match previewed(page, ctx, item) {
        Ok(preview) => form_page(page, ctx, item, &preview, typed, vec![problem], status),
        Err(response) => *response,
    };
    let Some(category) = typed.category else {
        return again(
            t.get(Key::ModerateProblemNoPart),
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    };
    match ctx.services.moderation.hide(
        &ctx.principal,
        &ctx.wiki,
        item,
        typed.parts,
        category,
        &typed.note,
    ) {
        Ok(made) => {
            let key = if made.declined.is_some() {
                Key::ModerateDoneDeclined
            } else {
                Key::ModerateDoneHidden
            };
            notice(
                page,
                &ctx.wiki,
                StatusCode::OK,
                Key::ModerateHeading,
                vec![t.get(key)],
                vec![Choice {
                    label: t.get(Key::ModerateBack),
                    href: paths::log(ctx.slug()),
                }],
                None,
            )
        }
        Err(ModerationError::NoPart) => again(
            t.get(Key::ModerateProblemNoPart),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ModerationError::NoSuchPart) => again(
            t.get(Key::ModerateProblemNoSuchPart),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ModerationError::AlreadyHidden) => {
            again(t.get(Key::ModerateProblemAlready), StatusCode::CONFLICT)
        }
        Err(ModerationError::Note) => again(
            t.msg(Key::ModerateProblemNote)
                .with("limit", MAX_NOTE_CHARS)
                .text(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(ModerationError::Changed) => {
            again(t.get(Key::ModerateProblemChanged), StatusCode::CONFLICT)
        }
        // The tip's text is told by the form itself (its sentence, and the link to the change), above the same choices.
        Err(ModerationError::TipContent { .. }) => {
            again(t.get(Key::ModerateProblemNoSuchPart), StatusCode::CONFLICT)
        }
        Err(other) => refused(page, ctx, &other),
    }
}

/// `POST /{slug}/-/moderate/unhide`: restore what one hide hid.
pub(crate) async fn unhide_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !services.moderation.may_moderate(&ctx.principal, &ctx.wiki) {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let Some(hide) = given
            .get(fields::HIDE)
            .and_then(|id| TombstoneEventId::parse(id))
        else {
            return generic_not_found(page);
        };
        match services.moderation.unhide(&ctx.principal, &ctx.wiki, &hide) {
            Ok(_) => see(&paths::log(ctx.slug())),
            Err(other) => refused(page, &ctx, &other),
        }
    })
    .await
}

/// What *show hidden content* shows.
struct FileTpl {
    path: String,
    text: String,
}

#[derive(Template)]
#[template(path = "moderation_show.html")]
struct ShowTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    facts: Vec<String>,
    text_label: String,
    text: Option<String>,
    author_label: String,
    author: Option<String>,
    files_label: String,
    files: Vec<FileTpl>,
    note: Option<String>,
    nothing: Option<String>,
    back: Choice,
}

/// `POST /{slug}/-/moderate/show`: the audited act that shows hidden content to the `moderate` rung. The answer is never
/// stored by a cache.
pub(crate) async fn show_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: PathName,
    form: Raw,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !services.moderation.may_moderate(&ctx.principal, &ctx.wiki) {
            return outcome_page(page, &ctx, &Outcome::NotAllowed);
        }
        let given = match form_fields(page, &ctx, &headers, form) {
            Ok(f) => f,
            Err(response) => return *response,
        };
        let (Some(item), Some(hide)) = (
            item_of_params(&given),
            given
                .get(fields::HIDE)
                .and_then(|id| TombstoneEventId::parse(id)),
        ) else {
            return generic_not_found(page);
        };
        match services
            .moderation
            .show_hidden(&ctx.principal, &ctx.wiki, &item, &hide)
        {
            Ok(revealed) => {
                let marks = services
                    .moderation
                    .preview(&ctx.principal, &ctx.wiki, &item)
                    .map(|p| p.marks)
                    .unwrap_or_default();
                let mut response = reveal_page(page, &ctx, &revealed, &marks, &hide);
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response
            }
            Err(other) => refused(page, &ctx, &other),
        }
    })
    .await
}

fn reveal_page(
    page: &Page,
    ctx: &Ctx,
    revealed: &Revealed,
    marks: &[inklkunn_core::tombstone::Mark],
    hide: &TombstoneEventId,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let nothing = revealed.text.is_none()
        && revealed.author.is_none()
        && revealed.files.is_empty()
        && revealed.note.is_none();
    let rendered = ShowTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleModerationShow)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::ShowHeading),
        intro: t.get(Key::ShowIntro),
        facts: marks
            .iter()
            .filter(|m| &m.event == hide)
            .flat_map(|m| mark_facts(t, m))
            .collect(),
        text_label: t.get(Key::ShowTextLabel),
        text: revealed.text.clone(),
        author_label: t.get(Key::ShowAuthorLabel),
        author: revealed.author.clone(),
        files_label: t.get(Key::ShowFilesLabel),
        files: revealed
            .files
            .iter()
            .map(|f| FileTpl {
                path: f.path.clone(),
                text: f.text.clone(),
            })
            .collect(),
        note: revealed
            .note
            .as_ref()
            .map(|n| t.msg(Key::LogNote).with("note", n).text()),
        nothing: nothing.then(|| t.get(Key::ShowNothing)),
        back: Choice {
            label: t.get(Key::ModerateBack),
            href: paths::log(ctx.slug()),
        },
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}
