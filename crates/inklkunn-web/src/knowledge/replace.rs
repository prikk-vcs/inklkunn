//! The form that replaces a knowledge file the wiki cannot read, or retires it (RFC 006 D-10; handoff G-T6): the `knowledge_invalid`
//! state of 0.3.0 is no longer a dead end.
//!
//! A file under `knowledge/` that is not in the form the wiki reads (a hand edit, an import) holds something no person can say: what
//! it held is not known. So **the people who may moderate the wiki** replace it, by a curation proposal that writes a file the wiki
//! reads in its place, or retires it. The form is `/{slug}/-/knowledge/replace?file=<the file's place>`, aimed at one file by the list of
//! files that cannot be read (*Wiki health*), and it is the `moderate` rung's: the proposal service asks the same rung of whoever sends a
//! proposal that writes over, or takes away, a file the base cannot read (so a restricted account, capped below it, and a person
//! whom the rules of the wiki hold below it, are refused, here and wherever else such a proposal could be made).
//!
//! * **The file keeps its name, and so its identifier.** *Retiring* it writes the smallest file that says so (a claim, or a source, with
//!   nothing said of what the unreadable one held, `lifecycle: retired` and the reason given); the identifier is the file's name, which an
//!   unreadable file still has, so every file that names it keeps naming it.
//! * **The proposal is judged as every change to the knowledge files is**: a curation proposal (nothing but knowledge files), with the
//!   checks of the acceptance. What is written here is read by the wiki's own reader first, and what is wrong with it is said on the form.
//! * **A protection of a title is a page's.** A knowledge file is no page, so no protection of a title applies to it; the rung that
//!   accepts is the rung that accepts any proposal of this wiki.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::knowledge::compose::clean_text;
use inklkunn_core::knowledge::{
    Claim, ClaimId, Family, Lifecycle, Source, SourceId, SourceLife, family_of, is_keep, read_file,
};
use inklkunn_core::proposal::{Invalid, ProposalEdit, ProposalError};
use inklkunn_store::{Base, FileChange};

use crate::catalog::Key;
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::{
    Ctx, SMALL_FORM_LIMIT, fields_of, from_this_site, needs_account, notice_page, refused, see,
    serve, words,
};
use crate::url::view_ref;
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

mod fields;
mod paths;

pub(crate) use paths::replace as address;
use paths::{files_that_cannot_be_read, name_of};

type PathName = Result<Path<String>, PathRejection>;
type Params = Result<Query<HashMap<String, String>>, QueryRejection>;
type Raw = crate::proposals::urlencoded::Raw;

/// The body bound of the form: a whole knowledge file, and a few words.
pub(crate) const FORM_LIMIT: usize = SMALL_FORM_LIMIT;

/// What was typed, kept to show the form again.
#[derive(Default, Clone)]
struct Typed {
    choice: String,
    text: String,
    reason: String,
    summary: String,
}

/// The file the form is about, as the wiki holds it at its tip.
struct Subject_ {
    file: String,
    family: Family,
    view: Option<String>,
    base: Base,
}

/// What looking for the file the form is about came to.
enum Looked {
    /// A file this form is for.
    Found(Subject_),
    /// It is not one: the wiki can read it, or it is no knowledge file.
    Nothing,
    /// The wiki could not be read now.
    Unavailable,
}

/// The one rule that says which files this form is for: a knowledge file the index of the view cannot read.
fn subject_of(ctx: &Ctx, file: &str, view: Option<&str>) -> Looked {
    let full = match view.filter(|v| !v.is_empty()) {
        None => None,
        Some(name) => match view_ref(name) {
            Some(full) => Some(full),
            None => return Looked::Nothing,
        },
    };
    let Some(family) = family_of(file).filter(|_| !is_keep(file)) else {
        return Looked::Nothing;
    };
    let index = match ctx
        .services
        .read
        .knowledge_index(&ctx.wiki, full.as_deref())
    {
        Ok(index) => index,
        Err(e) => {
            ctx.log(&format!("the knowledge files could not be read now: {e}"));
            return Looked::Unavailable;
        }
    };
    if !index.invalid().iter().any(|row| row.path == file) {
        return Looked::Nothing;
    }
    let base = match ctx
        .services
        .proposals
        .capture_base(&ctx.wiki, full.as_deref())
    {
        Ok(base) => base,
        Err(e) => {
            ctx.log(&format!("a base could not be read now: {e}"));
            return Looked::Unavailable;
        }
    };
    Looked::Found(Subject_ {
        file: file.to_owned(),
        family,
        view: view.filter(|v| !v.is_empty()).map(str::to_owned),
        base,
    })
}

/// The page that says the wiki could not be read now.
fn unavailable(page: &Page, ctx: &Ctx) -> Response {
    crate::wiki::with_retry_after(crate::wiki::notice(
        page,
        &ctx.wiki,
        StatusCode::SERVICE_UNAVAILABLE,
        Key::UnavailableHeading,
        vec![page.get(Key::UnavailableBody)],
        Vec::new(),
        None,
    ))
}

#[derive(Template)]
#[template(path = "replace_file.html")]
struct FormTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    problems: Vec<String>,
    intro: String,
    action: String,
    file_field: &'static str,
    file: String,
    view_field: &'static str,
    view: Option<String>,
    base_state_field: &'static str,
    base_state: String,
    base_block_field: &'static str,
    base_block: String,
    choice_field: &'static str,
    choice_legend: String,
    write_value: &'static str,
    write_label: String,
    write_checked: bool,
    retire_value: &'static str,
    retire_label: String,
    retire_checked: bool,
    text_field: &'static str,
    text_label: String,
    text_hint: String,
    text: String,
    reason_field: &'static str,
    reason_label: String,
    reason_hint: String,
    retire_hint: String,
    reason: String,
    summary_field: &'static str,
    summary_label: String,
    summary_hint: String,
    summary_max: usize,
    summary: String,
    send: String,
    back: Choice,
}

fn form_page(
    page: &Page,
    ctx: &Ctx,
    subject: &Subject_,
    typed: &Typed,
    problems: Vec<String>,
    status: StatusCode,
) -> Response {
    let t = page.t();
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let rendered = FormTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitlePage)
            .with("page", t.get(Key::ReplaceHeading))
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::ReplaceHeading),
        problems,
        intro: t.msg(Key::ReplaceIntro).with("file", &subject.file).text(),
        action: address(ctx.slug(), &subject.file, subject.view.as_deref()),
        file_field: fields::FILE,
        file: subject.file.clone(),
        view_field: fields::VIEW,
        view: subject.view.clone(),
        base_state_field: fields::BASE_STATE,
        base_state: subject.base.state_id.clone(),
        base_block_field: fields::BASE_BLOCK,
        base_block: subject.base.block_id.clone(),
        choice_field: fields::CHOICE,
        choice_legend: t.get(Key::ReplaceChoiceLegend),
        write_value: fields::CHOICE_WRITE,
        write_label: t.get(Key::ReplaceChoiceWrite),
        write_checked: typed.choice == fields::CHOICE_WRITE,
        retire_value: fields::CHOICE_RETIRE,
        retire_label: t.get(Key::ReplaceChoiceRetire),
        retire_checked: typed.choice == fields::CHOICE_RETIRE,
        text_field: fields::TEXT,
        text_label: t.get(Key::ReplaceLabelText),
        text_hint: t.get(Key::ReplaceTextHint),
        text: typed.text.clone(),
        reason_field: fields::REASON,
        reason_label: t.get(Key::CurateLabelRetireReason),
        reason_hint: t.get(Key::CurateRetireReasonHint),
        retire_hint: t.get(Key::ReplaceRetireHint),
        reason: typed.reason.clone(),
        summary_field: fields::SUMMARY,
        summary_label: t
            .msg(Key::ProposeLabelSummary)
            .with("limit", inklkunn_core::proposal::MAX_SUMMARY_CHARS)
            .text(),
        summary_hint: t.get(Key::ProposeSummaryHint),
        summary_max: inklkunn_core::proposal::MAX_SUMMARY_CHARS,
        summary: typed.summary.clone(),
        send: t.get(Key::ReplaceSend),
        back: Choice {
            label: t.get(Key::HealthBack),
            href: files_that_cannot_be_read(ctx.slug()),
        },
    }
    .render();
    answer(status, &head, rendered)
}

/// The page that says the file is not one this form is for.
fn nothing_to_replace(page: &Page, ctx: &Ctx) -> Response {
    notice_page(
        page,
        ctx,
        StatusCode::NOT_FOUND,
        Key::ReplaceHeading,
        vec![page.get(Key::ReplaceProblemNothing)],
        vec![Choice {
            label: page.get(Key::HealthBack),
            href: files_that_cannot_be_read(ctx.slug()),
        }],
    )
}

/// Whoever asks must be signed in and hold the `moderate` rung: a person who may not is told why, as any refusal of a proposal says it.
fn may_moderate(page: &Page, ctx: &Ctx) -> Option<Response> {
    if let Some(response) = needs_account(page, ctx) {
        return Some(response);
    }
    if ctx.may_wiki(Action::Moderate) {
        return None;
    }
    Some(refused(
        page,
        ctx,
        &ctx.refusal_wiki(Action::Moderate),
        Vec::new(),
    ))
}

/// `GET /{slug}/-/knowledge/replace?file=`.
pub(crate) async fn replace_form(
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
        if let Some(response) = may_moderate(page, &ctx) {
            return response;
        }
        let file = params.get(fields::FILE).map_or("", String::as_str);
        match subject_of(&ctx, file, params.get(fields::VIEW).map(String::as_str)) {
            Looked::Found(subject) => form_page(
                page,
                &ctx,
                &subject,
                &Typed {
                    choice: fields::CHOICE_WRITE.to_owned(),
                    ..Typed::default()
                },
                Vec::new(),
                StatusCode::OK,
            ),
            Looked::Nothing => nothing_to_replace(page, &ctx),
            Looked::Unavailable => unavailable(page, &ctx),
        }
    })
    .await
}

/// The text a retirement writes in place of a file the wiki cannot read: the smallest file that says so, with the identifier the
/// file's name gives.
fn retired_file(family: Family, file: &str, reason: &str) -> Option<String> {
    let id = name_of(file)?;
    Some(match family {
        Family::Claim => {
            let mut claim = Claim::new(ClaimId::parse(id)?);
            claim.lifecycle = Lifecycle::Retired;
            claim.retired_reason = Some(reason.to_owned());
            claim.to_yaml()
        }
        Family::Source => {
            let mut source = Source::new(SourceId::parse(id)?);
            source.lifecycle = SourceLife::Retired;
            source.retired_reason = Some(reason.to_owned());
            source.to_yaml()
        }
    })
}

/// `POST /{slug}/-/knowledge/replace`.
pub(crate) async fn replace_post(
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
        if let Some(response) = may_moderate(page, &ctx) {
            return response;
        }
        let given = match fields_of(page, &ctx, &headers, form) {
            Ok(given) => given,
            Err(response) => return *response,
        };
        let field = |name: &str| given.get(name).map_or("", String::as_str);
        let subject = match subject_of(&ctx, field(fields::FILE), Some(field(fields::VIEW))) {
            Looked::Found(mut subject) => {
                // The base the form was opened on is the one the proposal is made against (as the curation form's).
                if let Some(base) = Base::new(field(fields::BASE_STATE), field(fields::BASE_BLOCK))
                {
                    subject.base = base;
                }
                subject
            }
            Looked::Nothing => return nothing_to_replace(page, &ctx),
            Looked::Unavailable => return unavailable(page, &ctx),
        };
        let typed = Typed {
            choice: field(fields::CHOICE).to_owned(),
            text: field(fields::TEXT).to_owned(),
            reason: field(fields::REASON).trim().to_owned(),
            summary: field(fields::SUMMARY).trim().to_owned(),
        };
        make(page, &ctx, &subject, &typed)
    })
    .await
}

fn make(page: &Page, ctx: &Ctx, subject: &Subject_, typed: &Typed) -> Response {
    let t = page.t();
    let again = |problems: Vec<String>| {
        form_page(
            page,
            ctx,
            subject,
            typed,
            problems,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
    };
    let text = match typed.choice.as_str() {
        fields::CHOICE_WRITE => {
            // What is written is read by the wiki's own reader first; what is wrong is said, by the rule, and never by what was written.
            if let Err(fault) = read_file(&subject.file, typed.text.as_bytes()) {
                return again(vec![
                    t.msg(Key::ReplaceProblemText)
                        .with("rule", t.get(crate::health::rule_key(fault.rule)))
                        .text(),
                ]);
            }
            typed.text.clone()
        }
        fields::CHOICE_RETIRE => {
            let Ok(Some(reason)) = clean_text(&typed.reason, "retired_reason") else {
                return again(vec![t.get(Key::ReplaceProblemReason)]);
            };
            let Some(text) = retired_file(subject.family, &subject.file, &reason) else {
                return nothing_to_replace(page, ctx);
            };
            text
        }
        _ => return again(vec![t.get(Key::ReplaceProblemChoice)]),
    };
    let edit = ProposalEdit {
        summary: typed.summary.clone(),
        files: vec![FileChange {
            path: subject.file.clone(),
            text: Some(text),
        }],
        base: Some(subject.base.clone()),
        ..ProposalEdit::default()
    };
    let full = subject.view.as_deref().and_then(view_ref);
    // Made and sent in one step, as every curation proposal is: a refusal leaves nothing behind.
    match ctx
        .services
        .proposals
        .create_and_submit(&ctx.principal, &ctx.wiki, full.as_deref(), edit)
    {
        Ok(made) => see(&crate::proposals::paths::proposal(
            ctx.slug(),
            made.id.as_str(),
        )),
        Err(ProposalError::Invalid(why)) => match why {
            Invalid::SummaryMissing => again(vec![words::invalid_text(t, why)]),
            other => again(vec![words::invalid_text(t, other)]),
        },
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}
