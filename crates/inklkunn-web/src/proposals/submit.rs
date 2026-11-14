//! Sending the proposal form (RFC 004 D-2): what *Preview*, *Show changes*, *Save as draft* and *Submit* do,
//! for a proposal that is being opened (`…/proposals/new`) and for one that exists (`…/proposals/<id>/edit`:
//! a draft, a proposal held for review, a proposal that needs a revision).
//!
//! A post is read ([`super::form`]), its sizes judged before anything is parsed, and each page made whole
//! ([`prepare`]): a section or part goes back into the base text it was taken from, and the page's own line
//! endings are kept. What is wrong with it is collected and said **on the form, with the person's text still
//! in it**; the proposal service is called only when nothing is. A proposal that is opened by *Submit* is
//! checked for a summary and for a change **before** it is created, so a refused submission leaves no draft
//! behind.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::knowledge::compose::page_id_of;
use inklkunn_core::proposal::{Invalid, ProposalEdit, ProposalError};
use inklkunn_core::read::ReadError;
use inklkunn_markup::path::{PathError, is_inside_pages, path_to_title, title_to_path};
use inklkunn_markup::{LintContext, Page as MarkupPage, lint};
use inklkunn_store::{Base, FileChange, ProposalId, ProposalRecord, ProposalState};

use super::claims::{self, Failure as ClaimFailure, Held, Lost, Work};
use super::form::{
    Eol, FormError, Front, Markup, Op, Submitted, SubmittedPage, check_bounds, check_owned_keys,
    for_display, read,
};
use super::knowledge::{Place, file_lines};
use super::new::{
    FormPage, FormState, Kind, KnowledgeState, PreviewView, read_failure, render_form, short,
    view_of,
};
use super::parts::{self, Selection};
use super::render::{self, Alone, PageInput};
use super::{
    Ctx, Stop, fields, fields_of, from_this_site, needs_account, notice_page, parse_id, paths,
    refused, refused_offering, see, serve, words,
};
use crate::catalog::{Key, T};
use crate::csrf::Decided;
use crate::pages::{Page, PageState};
use crate::present::WikiLinks;
use crate::wiki::{Body, Choice, generic_not_found, services_of};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;
type Fields = super::urlencoded::Raw;

// ------------------------------------------------------------------------------------ the entry points

/// `POST /{slug}/-/proposals/new`.
pub(crate) async fn new_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    slug: Result<Path<String>, PathRejection>,
    form: Fields,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path(slug)), Some(services)) = (slug, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        match fields_of(page, &ctx, &headers, form) {
            Ok(fields) => post(page, &ctx, &fields, None),
            Err(response) => *response,
        }
    })
    .await
}

/// `GET /{slug}/-/proposals/{id}/edit`: the form with the proposal's own text.
pub(crate) async fn edit_form(
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
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        let existing = match Existing::load(&ctx, &id) {
            Ok(existing) => existing,
            Err(error) => return refused(page, &ctx, &error, Vec::new()),
        };
        let saved = params.contains_key(fields::Q_SAVED);
        let place = Place {
            view: Some(&existing.record.view),
            base: &existing.content.base,
            renames: &existing.content.renames,
        };
        let carried = match file_lines(&ctx, page.t(), &place, &existing.content.files) {
            Ok(known) => known.lines,
            Err(e) => return read_failure(page, &ctx, e),
        };
        match existing.state(&ctx, saved, carried) {
            Ok(state) => render_form(page, &ctx, state, StatusCode::OK),
            Err(e) => read_failure(page, &ctx, e),
        }
    })
    .await
}

/// `POST /{slug}/-/proposals/{id}/edit`.
pub(crate) async fn edit_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    path: Result<Path<(String, String)>, PathRejection>,
    form: Fields,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(Path((slug, id))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        let Some(id) = parse_id(&id) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        match fields_of(page, &ctx, &headers, form) {
            Ok(fields) => post(page, &ctx, &fields, Some(id)),
            Err(response) => *response,
        }
    })
    .await
}

// ------------------------------------------------------------------------------------ a proposal that exists

/// A proposal the principal may revise: its record and the text it holds now.
struct Existing {
    record: ProposalRecord,
    content: inklkunn_store::RevisionContent,
}

impl Existing {
    /// The proposal if it is the principal's own and can still be written on.
    fn load(ctx: &Ctx, id: &ProposalId) -> Result<Existing, ProposalError> {
        let service = &ctx.services.proposals;
        let record = service.get(&ctx.principal, &ctx.wiki, id)?;
        if ctx.account() != Some(&record.author) {
            return Err(ProposalError::NotAllowed);
        }
        let content = match record.state {
            ProposalState::Draft => service.draft(&ctx.principal, &ctx.wiki, id)?,
            ProposalState::HeldForReview | ProposalState::NeedsRevision => {
                service
                    .revisions(&ctx.principal, &ctx.wiki, id)?
                    .pop()
                    .ok_or(ProposalError::Changed)?
                    .content
            }
            state => {
                return Err(ProposalError::NotInThisState {
                    op: inklkunn_store::ProposalOp::Revise,
                    state,
                });
            }
        };
        Ok(Existing { record, content })
    }

    /// The form that shows it. The proposal's claim and source files are not text for a person to edit: they are
    /// carried forward and listed.
    fn state(
        &self,
        ctx: &Ctx,
        saved: bool,
        carried: Vec<super::knowledge::Line>,
    ) -> Result<FormState, inklkunn_core::read::ReadError> {
        // The markers of the paragraphs others rely on are held, those the proposal makes among them.
        let work = Work::new(
            ctx,
            Some(&self.record.view),
            &self.content.base.block_id,
            &self.content.files,
        );
        let own = work.own();
        let mut held = Held::None;
        let mut pages = Vec::new();
        for f in self
            .content
            .files
            .iter()
            .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_none())
        {
            let text = f.text.as_deref().unwrap_or("");
            let known = claims::known(ctx, Some(&self.record.view), text, &own)?;
            let (shown, mark) = claims::show(text, &known, Selection::Whole);
            if mark != Held::None {
                held = Held::Many;
            }
            pages.push(FormPage {
                path: Some(f.path.clone()),
                title: path_to_title(&f.path).unwrap_or_else(|| f.path.clone()),
                new_page: false,
                removed: f.text.is_none(),
                text: for_display(&shown),
            });
        }
        Ok(FormState {
            action: paths::act(ctx.slug(), self.record.id.as_str(), paths::ACT_EDIT),
            kind: Kind::Revise,
            base: self.content.base.clone(),
            view: short(Some(&self.record.view))
                .filter(|_| self.record.view != ctx.wiki.default_view),
            pages,
            selection: Selection::Whole,
            excerpt: String::new(),
            renames: self.content.renames.clone(),
            summary: self.record.summary.clone(),
            sources: self.record.sources.clone(),
            problems: Vec::new(),
            saved,
            can_draft: self.record.state == ProposalState::Draft,
            submit_key: if self.record.state == ProposalState::Draft {
                Key::ProposeSubmit
            } else {
                Key::ProposeSubmitRevision
            },
            suggestions: Vec::new(),
            previews: Vec::new(),
            changes: None,
            knowledge: KnowledgeState {
                held,
                carried,
                ..KnowledgeState::default()
            },
        })
    }
}

// ------------------------------------------------------------------------------------ preparing the pages

/// One page of a post, made whole.
struct Whole {
    path: String,
    title: String,
    old: Option<String>,
    new: Option<String>,
}

/// What a post comes to once its pages are whole.
struct Prepared {
    pages: Vec<Whole>,
    problems: Vec<String>,
    excerpt: String,
    /// Whether markers of claim paragraphs were held for the person.
    held: Held,
    /// The paragraphs others rely on that the text leaves out, with the id of the page they are in.
    lost: Vec<(String, Lost)>,
    /// Where the part the person edited ended up in the page (a form for one part).
    region: Option<Range<usize>>,
}

fn title_problem(t: T, why: &PathError) -> String {
    t.get(match why {
        PathError::Empty => Key::ProposeTitleEmpty,
        PathError::Reserved(_) => Key::ProposeTitleReserved,
        PathError::TooLong(_) => Key::ProposeTitleTooLong,
        PathError::PathTooLong(_) => Key::ProposeTitleTooDeep,
    })
}

/// Make the pages of `sub` whole against the base. A read that fails is the page that says so; what is wrong
/// with what the person wrote is collected in `problems`.
fn prepare(
    page: &Page,
    ctx: &Ctx,
    sub: &Submitted,
    base: &Base,
    view: Option<&str>,
    started: &BTreeMap<String, String>,
    own: &[inklkunn_core::knowledge::Claim],
) -> Result<Prepared, Stop> {
    let t = page.t();
    let mut out = Prepared {
        pages: Vec::new(),
        problems: Vec::new(),
        excerpt: String::new(),
        held: Held::None,
        lost: Vec::new(),
        region: None,
    };
    let unreadable = |e| Box::new(read_failure(page, ctx, e));
    for (i, p) in sub.pages.iter().enumerate() {
        let (path, title, old) = match page_base(page, ctx, p, base, view, &mut out.problems)? {
            Some(found) => found,
            None => continue,
        };
        if p.removed {
            out.pages.push(Whole {
                path,
                title,
                old,
                new: None,
            });
            continue;
        }
        let new = match (&old, sub.selection, sub.pages.len()) {
            (Some(old_text), Selection::Section(_) | Selection::Block(_), 1) if i == 0 => {
                match parts::find(old_text, sub.selection) {
                    Some(found) => {
                        out.excerpt = found.excerpt.clone();
                        let eol = Eol::of(old_text);
                        // The markers of the paragraphs others rely on are the form's: put back.
                        let slice = old_text.get(found.range.clone()).unwrap_or("");
                        let known = claims::known(ctx, view, old_text, own).map_err(unreadable)?;
                        let restored =
                            claims::restore(slice, &eol.apply(&p.text), &known, sub.selection);
                        out.held = claims::show(slice, &known, sub.selection).1;
                        let at = found.range.start;
                        out.region = Some(at..at + restored.text.len());
                        let page_id = page_id_of(old_text).unwrap_or_default();
                        out.lost
                            .extend(restored.lost.into_iter().map(|l| (page_id.clone(), l)));
                        parts::splice(old_text, &found, &restored.text)
                    }
                    None => {
                        out.problems.push(t.get(Key::InvalidIncomplete));
                        continue;
                    }
                }
            }
            (Some(old_text), _, _) => {
                let eol = Eol::of(old_text);
                // A revision starts from the text the proposal has, not from the base.
                let from = started.get(&path).map_or(old_text.as_str(), String::as_str);
                let known = claims::known(ctx, view, from, own).map_err(unreadable)?;
                let restored = claims::restore(from, &eol.apply(&p.text), &known, Selection::Whole);
                if claims::show(from, &known, Selection::Whole).1 != Held::None {
                    out.held = Held::Many;
                }
                let page_id = page_id_of(from).unwrap_or_default();
                out.lost
                    .extend(restored.lost.into_iter().map(|l| (page_id.clone(), l)));
                Some(restored.text)
            }
            (None, _, _) => Some(Eol::Lf.apply(&p.text)),
        };
        let Some(new) = new else {
            out.problems.push(t.get(Key::InvalidIncomplete));
            continue;
        };
        match check_owned_keys(&Markup, old.as_deref(), &new, || {
            ctx.may_wiki(Action::Admin)
        }) {
            Ok(()) => {}
            Err(Front::Unreadable) => out.problems.push(t.get(Key::ProposeFrontUnreadable)),
            Err(Front::Owned(keys)) => out.problems.push(
                t.msg(Key::ProposeFrontOwned)
                    .with("keys", keys.join(", "))
                    .text(),
            ),
        }
        out.pages.push(Whole {
            path,
            title,
            old,
            new: Some(new),
        });
    }
    Ok(out)
}

/// The path, title and base text of a submitted page. `Ok(None)` when the page cannot be made (the reason is
/// in `problems`).
#[allow(clippy::type_complexity)]
fn page_base(
    page: &Page,
    ctx: &Ctx,
    p: &SubmittedPage,
    base: &Base,
    view: Option<&str>,
    problems: &mut Vec<String>,
) -> Result<Option<(String, String, Option<String>)>, Stop> {
    let t = page.t();
    let read = &ctx.services.read;
    match &p.path {
        None => {
            let title = p.title.trim().to_owned();
            let path = match title_to_path(&title) {
                Ok(path) => path,
                Err(why) => {
                    problems.push(title_problem(t, &why));
                    return Ok(None);
                }
            };
            let tip = read
                .tip(&ctx.wiki, view)
                .map_err(|e| Box::new(read_failure(page, ctx, e)))?;
            let exists = read
                .source_at(&ctx.wiki, &tip.block_id, &path)
                .map_err(|e| Box::new(read_failure(page, ctx, e)))?
                .is_some();
            if exists {
                problems.push(t.msg(Key::ProposeTitleTaken).with("title", &title).text());
                return Ok(None);
            }
            Ok(Some((path, title, None)))
        }
        Some(path) => {
            if !is_inside_pages(path) {
                problems.push(t.get(Key::InvalidIncomplete));
                return Ok(None);
            }
            let old = read
                .source_at(&ctx.wiki, &base.block_id, path)
                .map_err(|e| Box::new(read_failure(page, ctx, e)))?;
            let title = if p.title.is_empty() {
                path_to_title(path).unwrap_or_else(|| path.clone())
            } else {
                p.title.clone()
            };
            Ok(Some((path.clone(), title, old)))
        }
    }
}

// ------------------------------------------------------------------------------------ a post

fn bounds_failure(page: &Page, ctx: &Ctx, why: FormError) -> Response {
    let t = page.t();
    let invalid = match why {
        FormError::PageTooLarge => Invalid::PageTooLarge,
        _ => Invalid::TooLarge,
    };
    notice_page(
        page,
        ctx,
        StatusCode::PAYLOAD_TOO_LARGE,
        Key::RefusedInvalidHeading,
        vec![words::invalid_text(t, invalid)],
        vec![Choice {
            label: t.get(Key::ProposalsBackToList),
            href: paths::list(ctx.slug()),
        }],
    )
}

/// What a post does. `existing` is the proposal being revised, `None` for one being opened.
fn post(
    page: &Page,
    ctx: &Ctx,
    fields: &HashMap<String, String>,
    existing: Option<ProposalId>,
) -> Response {
    let t = page.t();
    let slug = ctx.slug();
    let sub = match read(fields) {
        Ok(sub) => sub,
        Err(_) => return incomplete(page, ctx),
    };
    // The sizes first: nothing below parses a byte of a text that is too large.
    if let Err(why) = check_bounds(&sub) {
        return bounds_failure(page, ctx, why);
    }
    let held = match &existing {
        Some(id) => match Existing::load(ctx, id) {
            Ok(held) => Some(held),
            Err(error) => return refused(page, ctx, &error, Vec::new()),
        },
        None => None,
    };
    let (base, full_view) = match (&held, &sub.base, view_of(sub.view.as_deref())) {
        (Some(h), _, _) => (h.content.base.clone(), Some(h.record.view.clone())),
        (None, Some(base), Ok(view)) => (base.clone(), view),
        _ => return incomplete(page, ctx),
    };
    if held.is_none() && !ctx.may_wiki(Action::Propose) {
        return refused(page, ctx, &ProposalError::NotAllowed, Vec::new());
    }
    // The claim and source files a revision already has are carried forward; this post may add to them.
    let carried: Vec<FileChange> = held
        .as_ref()
        .map_or_else(Vec::new, |h| h.content.files.clone());
    let started: BTreeMap<String, String> = carried
        .iter()
        .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_none())
        .filter_map(|f| Some((f.path.clone(), f.text.clone()?)))
        .collect();
    let mut work = Work::new(ctx, full_view.as_deref(), &base.block_id, &carried);
    let mut prepared = match prepare(
        page,
        ctx,
        &sub,
        &base,
        full_view.as_deref(),
        &started,
        &work.own(),
    ) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let mut problems = prepared.problems.clone();
    // A paragraph others rely on that the text leaves out is an act the person confirms (RFC 005 part 1 D-1 b).
    let mut dropping: Vec<Lost> = Vec::new();
    for (page_id, lost) in &prepared.lost {
        dropping.push(lost.clone());
        if sub.drops.contains(&lost.id) {
            if let Err(e) = work.drop_expression(page_id, &lost.id) {
                return read_failure(page, ctx, e);
            }
        } else {
            problems.push(
                t.msg(Key::ProposeClaimLost)
                    .with("excerpt", &lost.excerpt)
                    .text(),
            );
        }
    }
    // A source attached to the paragraph the form edits, in the same proposal as the page edit.
    if let Some(what) = &sub.attach {
        match (prepared.pages.as_mut_slice(), prepared.region.clone()) {
            ([only], Some(region)) if only.old.is_some() => {
                let text = only.new.clone().unwrap_or_default();
                match work.attach(&text, &region, what) {
                    Ok(changed) => only.new = Some(changed),
                    Err(ClaimFailure::Read(e)) => return read_failure(page, ctx, e),
                    Err(ClaimFailure::Mistake(m)) => problems.push(claims::mistake_text(t, m)),
                }
            }
            _ => problems.push(t.get(Key::InvalidIncomplete)),
        }
    }
    let knowledge_files = work.files();
    let prepared = prepared;
    let changed: Vec<&Whole> = prepared.pages.iter().filter(|w| w.new != w.old).collect();
    // A draft keeps every page it was given; a submission sends what changes. The claim and source files go with
    // either: they are part of the same proposal.
    let files = |only_changed: bool| -> Vec<FileChange> {
        prepared
            .pages
            .iter()
            .filter(|w| !only_changed || w.new != w.old)
            .map(|w| FileChange {
                path: w.path.clone(),
                text: w.new.clone(),
            })
            .chain(knowledge_files.iter().cloned())
            .collect()
    };
    // The lines read the pages as this proposal leaves them (K34), so the files are all of it, not the claims' alone.
    let place = Place {
        view: full_view.as_deref(),
        base: &base,
        renames: &sub.renames,
    };
    let (carried_lines, carried_curated) = match file_lines(ctx, t, &place, &files(true)) {
        Ok(found) => (found.lines, found.curated),
        Err(e) => return read_failure(page, ctx, e),
    };
    let view_name = short(full_view.as_deref())
        .filter(|_| full_view.as_deref() != Some(ctx.wiki.default_view.as_str()));
    let state = |problems: Vec<String>| -> FormState {
        FormState {
            action: match &existing {
                Some(id) => paths::act(slug, id.as_str(), paths::ACT_EDIT),
                None => paths::new_action(slug),
            },
            kind: match (&held, sub.pages.first()) {
                (Some(_), _) => Kind::Revise,
                (None, Some(p)) if p.path.is_none() => Kind::NewPage,
                _ => Kind::Page,
            },
            base: base.clone(),
            view: view_name.clone(),
            pages: sub
                .pages
                .iter()
                .enumerate()
                .map(|(i, p)| FormPage {
                    path: p.path.clone(),
                    title: prepared
                        .pages
                        .get(i)
                        .map(|w| w.title.clone())
                        .filter(|_| p.path.is_some())
                        .unwrap_or_else(|| p.title.clone()),
                    new_page: p.path.is_none(),
                    removed: p.removed,
                    text: for_display(&p.text),
                })
                .collect(),
            selection: sub.selection,
            excerpt: prepared.excerpt.clone(),
            renames: sub.renames.clone(),
            summary: sub.summary.clone(),
            sources: sub.sources.clone(),
            problems,
            saved: false,
            can_draft: held
                .as_ref()
                .is_none_or(|h| h.record.state == ProposalState::Draft),
            submit_key: match &held {
                Some(h) if h.record.state != ProposalState::Draft => Key::ProposeSubmitRevision,
                _ => Key::ProposeSubmit,
            },
            suggestions: Vec::new(),
            previews: Vec::new(),
            changes: None,
            knowledge: KnowledgeState {
                held: prepared.held,
                attach: sub.attach.clone().map(|a| (a, prepared.held == Held::One)),
                drops: dropping.clone(),
                confirmed: sub.drops.clone(),
                carried: carried_lines.clone(),
                curate: Default::default(),
            },
        }
    };
    match sub.op {
        Op::Preview | Op::Changes => {
            let mut shown = state(problems.clone());
            let status = if problems.is_empty() {
                StatusCode::OK
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            if sub.op == Op::Preview {
                match previews(
                    page,
                    ctx,
                    &prepared,
                    full_view.as_deref(),
                    view_name.as_deref(),
                ) {
                    Ok((previews, suggestions)) => {
                        shown.previews = previews;
                        shown.suggestions = suggestions;
                    }
                    Err(response) => return *response,
                }
            } else {
                let inputs: Vec<PageInput<'_>> = prepared
                    .pages
                    .iter()
                    .map(|w| PageInput {
                        path: &w.path,
                        title: w.title.clone(),
                        old: w.old.as_deref(),
                        new: w.new.as_deref(),
                    })
                    .collect();
                shown.changes = Some(render::build(t, &inputs, &Alone));
                shown.suggestions = prepared
                    .pages
                    .iter()
                    .filter_map(|w| w.new.as_deref())
                    .flat_map(|text| suggestions(t, text))
                    .collect();
            }
            render_form(page, ctx, shown, status)
        }
        Op::Draft | Op::Submit => {
            if sub.op == Op::Submit {
                if sub.summary.is_empty() {
                    problems.push(words::invalid_text(t, Invalid::SummaryMissing));
                }
                if changed.is_empty() && knowledge_files.is_empty() && sub.renames.is_empty() {
                    problems.push(words::invalid_text(t, Invalid::NothingProposed));
                }
            }
            if !problems.is_empty() {
                return render_form(page, ctx, state(problems), StatusCode::UNPROCESSABLE_ENTITY);
            }
            let edit = |only_changed: bool, base: Option<Base>| ProposalEdit {
                summary: sub.summary.clone(),
                sources: sub.sources.clone(),
                files: files(only_changed),
                renames: sub.renames.clone(),
                minor: false,
                automated: false,
                base,
            };
            let outcome = match (&existing, &held, sub.op) {
                (None, _, Op::Draft) => {
                    create(ctx, full_view.as_deref(), edit(false, Some(base.clone())))
                        .map(|id| paths::edit_saved(slug, id.as_str()))
                }
                (None, _, _) => create(ctx, full_view.as_deref(), edit(true, Some(base.clone())))
                    .and_then(|id| {
                        ctx.services
                            .proposals
                            .submit(&ctx.principal, &ctx.wiki, &id)
                            .map(|_| paths::proposal(slug, id.as_str()))
                            .map_err(|error| Failure::Late {
                                id: id.clone(),
                                error,
                            })
                    }),
                (Some(id), Some(h), Op::Draft) if h.record.state == ProposalState::Draft => ctx
                    .services
                    .proposals
                    .save_draft(&ctx.principal, &ctx.wiki, id, edit(false, None))
                    .map(|_| paths::edit_saved(slug, id.as_str()))
                    .map_err(Failure::Plain),
                (Some(id), Some(h), _) if h.record.state == ProposalState::Draft => ctx
                    .services
                    .proposals
                    .save_draft(&ctx.principal, &ctx.wiki, id, edit(true, None))
                    .and_then(|_| ctx.services.proposals.submit(&ctx.principal, &ctx.wiki, id))
                    .map(|_| paths::proposal(slug, id.as_str()))
                    .map_err(Failure::Plain),
                (Some(id), _, _) => ctx
                    .services
                    .proposals
                    .revise(&ctx.principal, &ctx.wiki, id, edit(true, None))
                    .map(|_| paths::proposal(slug, id.as_str()))
                    .map_err(Failure::Plain),
            };
            match outcome {
                Ok(to) => see(&to),
                Err(Failure::Plain(ProposalError::Invalid(why))) => {
                    let mut problems = vec![words::invalid_text(t, why)];
                    // What no form makes is said, and is a reviewer's (K44).
                    if matches!(why, Invalid::KnowledgeCurated)
                        && crate::knowledge::curate::beyond_the_form(&carried_curated)
                    {
                        problems.push(t.get(Key::KnowNotByForm));
                    }
                    let mut shown = state(problems);
                    // A change that needs a curation proposal leads to where one is made (K39).
                    if matches!(why, Invalid::KnowledgeCurated) {
                        match curate_offers(ctx, t, full_view.as_deref(), &carried_curated) {
                            Ok(offered) => shown.knowledge.curate = offered,
                            Err(e) => return read_failure(page, ctx, e),
                        }
                    }
                    render_form(page, ctx, shown, StatusCode::UNPROCESSABLE_ENTITY)
                }
                Err(Failure::Plain(error)) => refused(page, ctx, &error, Vec::new()),
                // The draft was made and could not be sent: say why, and leave the person on the draft.
                Err(Failure::Late { id, error }) => {
                    let to = paths::act(slug, id.as_str(), paths::ACT_EDIT);
                    let choices = vec![Choice {
                        label: t.get(Key::ProposeContinueDraft),
                        href: to,
                    }];
                    let mut offers = Default::default();
                    if matches!(error, ProposalError::Invalid(Invalid::KnowledgeCurated)) {
                        match curate_offers(ctx, t, full_view.as_deref(), &carried_curated) {
                            Ok(offered) => offers = offered,
                            Err(e) => return read_failure(page, ctx, e),
                        }
                    }
                    let noting =
                        if matches!(error, ProposalError::Invalid(Invalid::KnowledgeCurated))
                            && crate::knowledge::curate::beyond_the_form(&carried_curated)
                        {
                            vec![t.get(Key::KnowNotByForm)]
                        } else {
                            Vec::new()
                        };
                    refused_offering(page, ctx, &error, choices, offers, noting)
                }
            }
        }
    }
}

/// The way to the curation form of each claim a refused change touches (K39), for the view the proposal is for
/// (`None`: the default view).
fn curate_offers(
    ctx: &Ctx,
    t: crate::catalog::T,
    full_view: Option<&str>,
    curated: &[inklkunn_core::knowledge::check::Refusal],
) -> Result<crate::knowledge::curate::Offers, inklkunn_core::read::ReadError> {
    let view = full_view.unwrap_or(ctx.wiki.default_view.as_str());
    crate::knowledge::curate::offers(ctx, t, view, curated)
}

/// Why a write failed: plainly, or after the draft it began with was made.
enum Failure {
    Plain(ProposalError),
    Late {
        id: ProposalId,
        error: ProposalError,
    },
}

fn create(ctx: &Ctx, view: Option<&str>, edit: ProposalEdit) -> Result<ProposalId, Failure> {
    ctx.services
        .proposals
        .create(&ctx.principal, &ctx.wiki, view, edit)
        .map(|record| record.id)
        .map_err(Failure::Plain)
}

fn incomplete(page: &Page, ctx: &Ctx) -> Response {
    notice_page(
        page,
        ctx,
        StatusCode::UNPROCESSABLE_ENTITY,
        Key::RefusedInvalidHeading,
        vec![page.get(Key::InvalidIncomplete)],
        vec![Choice {
            label: page.get(Key::ProposalsBackToList),
            href: paths::list(ctx.slug()),
        }],
    )
}

// ------------------------------------------------------------------------------------ preview and suggestions

fn previews(
    page: &Page,
    ctx: &Ctx,
    prepared: &Prepared,
    view: Option<&str>,
    view_name: Option<&str>,
) -> Result<(Vec<PreviewView>, Vec<String>), Stop> {
    let t = page.t();
    let links = WikiLinks::new(t, ctx.slug(), view_name);
    let mut views = Vec::new();
    let mut hints = Vec::new();
    for (i, w) in prepared.pages.iter().enumerate().take(MAX_PREVIEWS) {
        let Some(text) = &w.new else { continue };
        match ctx
            .services
            .read
            .preview(&ctx.wiki, view, &w.path, &w.title, text, &links)
        {
            Ok(preview) => views.push(PreviewView {
                id: format!("preview-{i}"),
                heading: t
                    .msg(Key::ProposePreviewHeading)
                    .with("title", &w.title)
                    .text(),
                html: Body(preview.html),
            }),
            Err(ReadError::PageUnreadable(_)) => {}
            Err(e) => return Err(Box::new(read_failure(page, ctx, e))),
        }
        hints.extend(suggestions(t, text));
    }
    Ok((views, hints))
}

/// The most pages a preview draws.
const MAX_PREVIEWS: usize = 8;

/// The linter's findings on `text`, in the catalog's words (PR-9: suggestions, never blockers).
fn suggestions(t: T, text: &str) -> Vec<String> {
    let Ok(parsed) = MarkupPage::parse(text) else {
        return Vec::new();
    };
    lint(&parsed, text, LintContext::default())
        .into_iter()
        .map(|finding| {
            t.msg(super::lints::lint_key(finding.code))
                .with("guideline", finding.guideline)
                .text()
        })
        .collect()
}
