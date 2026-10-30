//! The proposal form (RFC 004 D-2; WEB-03): the text of a page, a section, a part or a new page; what and why;
//! sources; and five things a person can do with it: *Preview* and *Show changes* (server round trips that
//! save nothing), *Save as draft*, *Submit*, and, to begin, choose what to change. No script.
//!
//! The same form revises a proposal that is a draft, is held for review or needs a revision (`…/edit`): it
//! then carries every page of the proposal, not only the one a person came from, and a page left as the base
//! has it is not a change.
//!
//! **What is checked, and in which order.** The form is read; its sizes are judged before anything is parsed
//! (`form::check_bounds`); each page's text is made whole (a section or part goes back into the base text it
//! came from, the page's own line endings are kept); the information at the top of each page is parsed to
//! refuse the keys the service owns; a new page's title goes through `title_to_path` and a refusal names the
//! rule, never the path. Everything wrong is said at once, on the form, with the person's text still in it.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::proposal::ProposalError;
use inklkunn_core::read::{PageProblem, Presentation, ReadError};
use inklkunn_core::{Action, Resource};
use inklkunn_store::{Base, Rename};

use super::form::for_display;
use super::parts::{self, Selection};
use super::render::{ChangesView, LinkView};
use super::{Ctx, fields, needs_account, paths, refused, see, serve};
use crate::catalog::Key;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::present::WikiLinks;
use crate::url::view_ref;
use crate::wiki::{Body, Shell, Shown, generic_not_found, present, services_of};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

// ------------------------------------------------------------------------------------ the view of a form

/// One hidden field.
pub(super) struct Hidden {
    pub(super) name: String,
    pub(super) value: String,
}

/// One page of the form.
pub(super) struct FormPage {
    /// The page's repository path; `None` for a new page.
    pub(super) path: Option<String>,
    pub(super) title: String,
    pub(super) new_page: bool,
    pub(super) removed: bool,
    /// The text as the text area shows it.
    pub(super) text: String,
}

/// One page of the form, ready to print.
pub(super) struct FieldView {
    pub(super) label: String,
    pub(super) id_text: String,
    pub(super) name_text: String,
    pub(super) id_title: String,
    pub(super) name_title: String,
    pub(super) title: String,
    pub(super) title_hint: String,
    pub(super) editable_title: bool,
    pub(super) removed_note: Option<String>,
    pub(super) text: String,
    pub(super) rows: usize,
}

pub(super) struct Button {
    pub(super) value: &'static str,
    pub(super) label: String,
}

pub(super) struct PreviewView {
    pub(super) id: String,
    pub(super) heading: String,
    pub(super) html: Body,
}

#[derive(Template)]
#[template(path = "proposal_form.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    scope: Option<String>,
    choose: Option<LinkView>,
    saved: Option<String>,
    problems: Vec<String>,
    action: String,
    hidden: Vec<Hidden>,
    fields: Vec<FieldView>,
    summary: String,
    summary_label: String,
    summary_hint: String,
    summary_max: usize,
    sources: String,
    buttons: Vec<Button>,
    op_name: &'static str,
    suggestions: Vec<String>,
    previews: Vec<PreviewView>,
    changes: Option<ChangesView>,
}

/// What a form shows.
pub(super) struct FormState {
    pub(super) action: String,
    pub(super) kind: Kind,
    pub(super) base: Base,
    pub(super) view: Option<String>,
    pub(super) pages: Vec<FormPage>,
    pub(super) selection: Selection,
    /// What the selected part begins with.
    pub(super) excerpt: String,
    pub(super) renames: Vec<Rename>,
    pub(super) summary: String,
    pub(super) sources: Vec<String>,
    pub(super) problems: Vec<String>,
    pub(super) saved: bool,
    /// May the work be saved as a draft (the proposal is a draft, or is not one yet)?
    pub(super) can_draft: bool,
    /// The proposal's state, for a form that revises one.
    pub(super) submit_key: Key,
    pub(super) suggestions: Vec<String>,
    pub(super) previews: Vec<PreviewView>,
    pub(super) changes: Option<ChangesView>,
}

/// What the form is for, for its heading.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A change to a page that exists.
    Page,
    /// A page that does not exist yet.
    NewPage,
    /// A revision (or the continuation of a draft).
    Revise,
}

fn rows_for(text: &str) -> usize {
    (text.lines().count() + 2).clamp(8, 30)
}

pub(super) fn render_form(
    page: &Page,
    ctx: &Ctx,
    state: FormState,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let shell = Shell::of(&ctx.wiki);
    let first_title = state
        .pages
        .first()
        .map(|p| p.title.clone())
        .unwrap_or_default();
    let (title_key, heading_key) = match state.kind {
        Kind::Page => (Key::TitleProposePage, Key::ProposeHeadingPage),
        Kind::NewPage => (Key::TitleProposeNew, Key::ProposeHeadingNew),
        Kind::Revise => (Key::TitleProposeRevise, Key::ProposeHeadingRevise),
    };
    let slug = ctx.slug();
    let view_name = state.view.as_deref();
    let single = state.pages.len() == 1;
    let mut hidden = vec![
        Hidden {
            name: fields::BASE_STATE.to_owned(),
            value: state.base.state_id.clone(),
        },
        Hidden {
            name: fields::BASE_BLOCK.to_owned(),
            value: state.base.block_id.clone(),
        },
        Hidden {
            name: fields::PAGES.to_owned(),
            value: state.pages.len().to_string(),
        },
        Hidden {
            name: fields::RENAMES.to_owned(),
            value: state.renames.len().to_string(),
        },
    ];
    if let Some(view) = view_name {
        hidden.push(Hidden {
            name: fields::VIEW.to_owned(),
            value: view.to_owned(),
        });
    }
    let (part_kind, part_ordinal) = match state.selection {
        Selection::Whole => (fields::PART_WHOLE, None),
        Selection::Section(n) => (fields::PART_SECTION, Some(n)),
        Selection::Block(n) => (fields::PART_BLOCK, Some(n)),
    };
    hidden.push(Hidden {
        name: fields::PART.to_owned(),
        value: part_kind.to_owned(),
    });
    if let Some(n) = part_ordinal {
        hidden.push(Hidden {
            name: fields::ORDINAL.to_owned(),
            value: n.to_string(),
        });
    }
    for (i, r) in state.renames.iter().enumerate() {
        hidden.push(Hidden {
            name: fields::rename_field(i, fields::F_FROM),
            value: r.from.clone(),
        });
        hidden.push(Hidden {
            name: fields::rename_field(i, fields::F_TO),
            value: r.to.clone(),
        });
    }
    let mut field_views = Vec::new();
    for (i, p) in state.pages.iter().enumerate() {
        let name = |what: &str| fields::page_field(i, what);
        if let Some(path) = &p.path {
            hidden.push(Hidden {
                name: name(fields::F_PATH),
                value: path.clone(),
            });
        }
        if p.new_page {
            hidden.push(Hidden {
                name: name(fields::F_NEW),
                value: "1".to_owned(),
            });
        } else {
            hidden.push(Hidden {
                name: name(fields::F_TITLE),
                value: p.title.clone(),
            });
        }
        if p.removed {
            hidden.push(Hidden {
                name: name(fields::F_REMOVED),
                value: "1".to_owned(),
            });
        }
        let label_key = match state.selection {
            Selection::Whole => Key::ProposeLabelText,
            Selection::Section(_) => Key::ProposeLabelSection,
            Selection::Block(_) => Key::ProposeLabelPart,
        };
        let shown_title = if p.title.is_empty() {
            t.get(Key::ProposeNewPageName)
        } else {
            p.title.clone()
        };
        field_views.push(FieldView {
            label: t.msg(label_key).with("title", &shown_title).text(),
            id_text: name(fields::F_TEXT),
            name_text: name(fields::F_TEXT),
            id_title: name(fields::F_TITLE),
            name_title: name(fields::F_TITLE),
            title: p.title.clone(),
            title_hint: t.get(Key::ProposeTitleHint),
            editable_title: p.new_page,
            removed_note: p.removed.then(|| {
                t.msg(Key::ProposeRemovedNote)
                    .with("title", &shown_title)
                    .text()
            }),
            text: p.text.clone(),
            rows: rows_for(&p.text),
        });
    }
    let scope = match (state.kind, state.selection) {
        (Kind::Page, Selection::Section(_)) => Some(
            t.msg(Key::ProposeScopeSection)
                .with("part", &state.excerpt)
                .text(),
        ),
        (Kind::Page, Selection::Block(_)) => Some(
            t.msg(Key::ProposeScopePart)
                .with("part", &state.excerpt)
                .text(),
        ),
        (Kind::Page, Selection::Whole) if single => Some(t.get(Key::ProposeScopeWhole)),
        _ => None,
    };
    let choose = (state.kind == Kind::Page && single)
        .then(|| {
            state.pages.first().and_then(|p| {
                p.path.is_some().then(|| LinkView {
                    href: paths::choose_part(slug, &p.title, view_name),
                    label: t.get(Key::ProposeChoose),
                })
            })
        })
        .flatten();
    let mut buttons = vec![
        Button {
            value: fields::OP_PREVIEW,
            label: t.get(Key::ProposePreview),
        },
        Button {
            value: fields::OP_CHANGES,
            label: t.get(Key::ProposeShowChanges),
        },
    ];
    if state.can_draft {
        buttons.push(Button {
            value: fields::OP_DRAFT,
            label: t.get(Key::ProposeSaveDraft),
        });
    }
    buttons.push(Button {
        value: fields::OP_SUBMIT,
        label: t.get(state.submit_key),
    });
    let rendered = FormTpl {
        page: &page.wiki_head(&ctx.wiki, Subject::Wiki),
        shell: &shell,
        title: t
            .msg(title_key)
            .with("page", &first_title)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.msg(heading_key).with("page", &first_title).text(),
        intro: t.get(Key::ProposeIntro),
        scope,
        choose,
        saved: state.saved.then(|| t.get(Key::ProposeSaved)),
        problems: state.problems,
        action: state.action,
        hidden,
        fields: field_views,
        summary: state.summary,
        summary_label: t
            .msg(Key::ProposeLabelSummary)
            .with("limit", inklkunn_core::proposal::MAX_SUMMARY_CHARS)
            .text(),
        summary_hint: t.get(Key::ProposeSummaryHint),
        summary_max: inklkunn_core::proposal::MAX_SUMMARY_CHARS,
        sources: state.sources.join("\n"),
        buttons,
        op_name: fields::OP,
        suggestions: state.suggestions,
        previews: state.previews,
        changes: state.changes,
    }
    .render();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    answer(status, &head, rendered)
}

// ------------------------------------------------------------------------------------ reading the wiki

/// A read that failed, as the page a wiki already has for it. The cause goes to the operator's log.
pub(super) fn read_failure(page: &Page, ctx: &Ctx, error: ReadError) -> Response {
    let wiki = Box::new(ctx.wiki.clone());
    let shown = match error {
        ReadError::InvalidTitle(_) => Shown::InvalidTitle { wiki },
        ReadError::NoSuchView(view) => Shown::NoSuchView {
            wiki,
            view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
        },
        ReadError::PageUnreadable(problem) => Shown::Unreadable { wiki, problem },
        ReadError::CouldNotBeReadNow(cause) => {
            ctx.log(&format!("a page could not be read for a proposal: {cause}"));
            Shown::Unavailable { wiki }
        }
    };
    present(page, shown)
}

/// The view an address names, in the repository's words; `None` for the default view, `Err` for a name that
/// cannot be a view.
pub(super) fn view_of(name: Option<&str>) -> Result<Option<String>, ()> {
    match name.filter(|n| !n.is_empty()) {
        None => Ok(None),
        Some(n) => view_ref(n).map(Some).ok_or(()),
    }
}

pub(super) fn short(full: Option<&str>) -> Option<String> {
    full.map(|v| v.strip_prefix("heads/").unwrap_or(v).to_owned())
}

// ------------------------------------------------------------------------------------ opening a form

/// `GET /{slug}/-/proposals/new`: the form for a page, a section or a part (`?page=`, `&part=`, `&ordinal=`),
/// the chooser of those (`&parts=1`), or for a new page (`?new=1&title=`).
pub(crate) async fn new_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (slug, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        open_form(page, &ctx, &params)
    })
    .await
}

fn open_form(page: &Page, ctx: &Ctx, params: &HashMap<String, String>) -> Response {
    if let Some(response) = needs_account(page, ctx) {
        return response;
    }
    if !ctx.may_wiki(Action::Propose) {
        return refused(page, ctx, &ProposalError::NotAllowed, Vec::new());
    }
    let slug = ctx.slug();
    let Ok(full_view) = view_of(params.get(fields::VIEW).map(String::as_str)) else {
        return present(
            page,
            Shown::NoSuchView {
                wiki: Box::new(ctx.wiki.clone()),
                view: params.get(fields::VIEW).cloned().unwrap_or_default(),
            },
        );
    };
    let view_name = short(full_view.as_deref());
    if params.get(fields::Q_NEW).map(String::as_str) == Some("1") {
        let base = match ctx
            .services
            .proposals
            .capture_base(&ctx.wiki, full_view.as_deref())
        {
            Ok(base) => base,
            Err(e) => return refused(page, ctx, &e, Vec::new()),
        };
        let title = params.get(fields::Q_TITLE).cloned().unwrap_or_default();
        return render_form(
            page,
            ctx,
            FormState {
                action: paths::new_action(slug),
                kind: Kind::NewPage,
                base,
                view: view_name,
                pages: vec![FormPage {
                    path: None,
                    title,
                    new_page: true,
                    removed: false,
                    text: String::new(),
                }],
                selection: Selection::Whole,
                excerpt: String::new(),
                renames: Vec::new(),
                summary: String::new(),
                sources: Vec::new(),
                problems: Vec::new(),
                saved: false,
                can_draft: true,
                submit_key: Key::ProposeSubmit,
                suggestions: Vec::new(),
                previews: Vec::new(),
                changes: None,
            },
            StatusCode::OK,
        );
    }
    let Some(title) = params.get(fields::Q_PAGE) else {
        return see(&paths::list(slug));
    };
    let raw = match ctx
        .services
        .read
        .raw(&ctx.wiki, full_view.as_deref(), title)
    {
        Ok(Some(raw)) => raw,
        Ok(None) => return crate::wiki::missing_page(page, &ctx.wiki, title),
        Err(e) => return read_failure(page, ctx, e),
    };
    let Ok(text) = String::from_utf8(raw.bytes) else {
        return read_failure(page, ctx, ReadError::PageUnreadable(PageProblem::NotUtf8));
    };
    if !ctx.may(
        Action::Propose,
        &Resource::Page {
            wiki: ctx.wiki.id.clone(),
            path: raw.path.clone(),
        },
    ) {
        return refused(page, ctx, &ProposalError::NotAllowed, Vec::new());
    }
    let Some(base) = Base::new(&raw.tip.state_id, &raw.tip.block_id) else {
        return refused(
            page,
            ctx,
            &ProposalError::BaseUnavailable("the tip's ids are not state and block ids".to_owned()),
            Vec::new(),
        );
    };
    if params.get(fields::Q_PARTS).is_some() {
        return choose(page, ctx, &raw.title, view_name.as_deref(), &text);
    }
    let selection = match (
        params.get(fields::PART).map(String::as_str),
        params
            .get(fields::ORDINAL)
            .and_then(|n| n.parse::<usize>().ok()),
    ) {
        (Some(fields::PART_SECTION), Some(n)) => Selection::Section(n),
        (Some(fields::PART_BLOCK), Some(n)) => Selection::Block(n),
        _ => Selection::Whole,
    };
    let Some(found) = parts::find(&text, selection) else {
        return see(&paths::choose_part(slug, &raw.title, view_name.as_deref()));
    };
    let shown = text.get(found.range.clone()).unwrap_or("").to_owned();
    render_form(
        page,
        ctx,
        FormState {
            action: paths::new_action(slug),
            kind: Kind::Page,
            base,
            view: view_name,
            pages: vec![FormPage {
                path: Some(raw.path),
                title: raw.title,
                new_page: false,
                removed: false,
                text: for_display(&shown),
            }],
            selection,
            excerpt: found.excerpt,
            renames: Vec::new(),
            summary: String::new(),
            sources: Vec::new(),
            problems: Vec::new(),
            saved: false,
            can_draft: true,
            submit_key: Key::ProposeSubmit,
            suggestions: Vec::new(),
            previews: Vec::new(),
            changes: None,
        },
        StatusCode::OK,
    )
}

// ------------------------------------------------------------------------------------ choosing what to change

#[derive(Template)]
#[template(path = "proposal_parts.html")]
struct PartsTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    whole: LinkView,
    sections_heading: String,
    sections: Vec<LinkView>,
    no_sections: String,
    parts_heading: String,
    parts: Vec<LinkView>,
    cut: Option<String>,
    back: LinkView,
}

fn choose(page: &Page, ctx: &Ctx, title: &str, view: Option<&str>, text: &str) -> Response {
    let t = page.t();
    let slug = ctx.slug();
    let shell = Shell::of(&ctx.wiki);
    let sections: Vec<LinkView> = parts::sections(text)
        .into_iter()
        .enumerate()
        .take(parts::MAX_LISTED)
        .map(|(n, s)| LinkView {
            href: paths::change_part(slug, title, view, fields::PART_SECTION, n),
            label: t
                .msg(Key::PartsSectionLabel)
                .with("heading", s.excerpt)
                .text(),
        })
        .collect();
    let all = parts::parts(text);
    let cut = (all.len() > parts::MAX_LISTED)
        .then(|| t.msg(Key::PartsCut).with("limit", parts::MAX_LISTED).text());
    let listed: Vec<LinkView> = all
        .into_iter()
        .enumerate()
        .take(parts::MAX_LISTED)
        .map(|(n, p)| LinkView {
            href: paths::change_part(slug, title, view, fields::PART_BLOCK, n),
            label: t
                .msg(Key::PartsPartLabel)
                .with("number", n + 1)
                .with("excerpt", p.excerpt)
                .text(),
        })
        .collect();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let rendered = PartsTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleProposeParts)
            .with("page", title)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.msg(Key::PartsHeading).with("page", title).text(),
        intro: t.get(Key::PartsIntro),
        whole: LinkView {
            href: paths::change_page(slug, title, view),
            label: t.get(Key::PartsWhole),
        },
        sections_heading: t.get(Key::PartsSectionsHeading),
        sections,
        no_sections: t.get(Key::PartsNoSections),
        parts_heading: t.get(Key::PartsPartsHeading),
        parts: listed,
        cut,
        back: LinkView {
            href: WikiLinks::new(t, slug, view).page_href(title),
            label: t.msg(Key::PartsBack).with("page", title).text(),
        },
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}
