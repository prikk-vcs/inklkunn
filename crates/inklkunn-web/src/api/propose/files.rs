//! The pages of a proposal sent as JSON (RFC 004 D-2; threat model C-4a … C-4d): read from the body, **sized
//! before anything is parsed**, addressed, and checked for the information at the top of the page — the same
//! rules as the proposal form, in the same order, so a client can do nothing the form cannot.
//!
//! A file is `{"path": "pages/Home.md", "text": "…"}` for a page that exists (or `"text": null` to remove it), or
//! `{"title": "New page", "text": "…"}` for a page that does not exist yet (its path is made from the title by
//! the wiki's own rule). The text is the page's whole new text, carried as it is: no line ending is changed and
//! nothing is trimmed, because the text is data.

use inklkunn_core::Action;
use inklkunn_core::proposal::{
    Invalid, MAX_PAGE_BYTES, MAX_PROPOSAL_BYTES, MAX_SOURCES, ProposalEdit,
};
use inklkunn_markup::path::{PathError, is_inside_pages, title_to_path};
use inklkunn_store::{Base, FileChange, Rename};

use crate::api::problem::Problem;
use crate::api::write::Request;
use crate::api::write::body::Fields;
use crate::catalog::Key;
use crate::proposals::form::{Front, MAX_FORM_PAGES, MAX_FORM_RENAMES, Markup, check_owned_keys};

/// One file of the body.
#[derive(Debug)]
struct File {
    /// The page's path, or `None` for a page that does not exist yet.
    path: Option<String>,
    title: String,
    /// The whole new text; `None` removes the page.
    text: Option<String>,
}

/// What the body says about a proposal's text, before it is held to a base.
#[derive(Debug)]
pub(super) struct Sent {
    summary: String,
    sources: Vec<String>,
    files: Vec<File>,
    renames: Vec<Rename>,
    minor: bool,
    automated: bool,
}

/// The members a proposal's text is made of; the caller takes its own (`view`, `submit`) first.
pub(super) fn read(fields: &mut Fields) -> Result<Sent, Problem> {
    let summary = fields.text("summary")?;
    let sources = fields.texts("sources", MAX_SOURCES + 1)?;
    let mut files = Vec::new();
    for mut file in fields.objects("files", MAX_FORM_PAGES)? {
        let path = file.opt_text("path")?;
        let title = file.opt_text("title")?;
        let text = file.text_or_null("text")?;
        file.end()?;
        files.push(match (path, title, &text) {
            (Some(path), None, _) => File {
                path: Some(path),
                title: String::new(),
                text,
            },
            // A page that does not exist yet has text.
            (None, Some(title), Some(_)) => File {
                path: None,
                title,
                text,
            },
            _ => return Err(crate::api::write::body::invalid("files")),
        });
    }
    let mut renames = Vec::new();
    for mut rename in fields.objects("renames", MAX_FORM_RENAMES)? {
        let from = rename.text("from")?;
        let to = rename.text("to")?;
        rename.end()?;
        renames.push(Rename { from, to });
    }
    Ok(Sent {
        summary,
        sources,
        files,
        renames,
        minor: fields.opt_flag("minor")?,
        automated: fields.opt_flag("automated")?,
    })
}

/// A page of the proposal made whole: its path, its text at the base and its new text.
struct Whole {
    path: String,
    old: Option<String>,
    new: Option<String>,
}

/// The sizes, judged before any text is parsed: no page over the bound, and the texts, paths and renames
/// together within the proposal's.
fn check_sizes(sent: &Sent) -> Result<(), Invalid> {
    if sent
        .files
        .iter()
        .any(|f| f.text.as_ref().is_some_and(|t| t.len() > MAX_PAGE_BYTES))
    {
        return Err(Invalid::PageTooLarge);
    }
    let files: usize = sent
        .files
        .iter()
        .map(|f| {
            f.text.as_ref().map_or(0, String::len)
                + f.path.as_ref().map_or(0, String::len)
                + f.title.len()
        })
        .sum();
    let renames: usize = sent.renames.iter().map(|r| r.from.len() + r.to.len()).sum();
    if u64::try_from(files + renames).unwrap_or(u64::MAX) > MAX_PROPOSAL_BYTES {
        return Err(Invalid::TooLarge);
    }
    Ok(())
}

fn title_problem(req: &Request, why: &PathError) -> Problem {
    let t = req.t;
    let key = match why {
        PathError::Empty => Key::ProposeTitleEmpty,
        PathError::Reserved(_) => Key::ProposeTitleReserved,
        PathError::TooLong(_) => Key::ProposeTitleTooLong,
        PathError::PathTooLong(_) => Key::ProposeTitleTooDeep,
    };
    req.unacceptable_with("title-invalid", t.get(key))
}

/// Make the files whole against `base` and judge them. `Ok` carries the edit to give the service: every page
/// (`keep_all`, for a draft) or those that change (a submission).
pub(super) fn prepare(
    req: &Request,
    sent: Sent,
    base: &Base,
    keep_all: bool,
) -> Result<ProposalEdit, Problem> {
    check_sizes(&sent).map_err(|why| req.unacceptable(why))?;
    let t = req.t;
    let mut whole = Vec::with_capacity(sent.files.len());
    for file in &sent.files {
        // A page is read only for a caller who may propose to it: what the base holds is not told otherwise.
        let read = |path: &str| {
            let resource = inklkunn_core::Resource::Page {
                wiki: req.wiki.id.clone(),
                path: path.to_owned(),
            };
            if !req
                .services
                .authorizer
                .authorize(&req.principal, Action::Propose, &resource)
                .is_allowed()
            {
                return Err(Problem::new(crate::api::problem::Code::Forbidden));
            }
            req.services
                .read
                .source_at(&req.wiki, &base.block_id, path)
                .map_err(|error| read_failure(req, error))
        };
        let (path, old) = match &file.path {
            Some(path) => {
                if !is_inside_pages(path) {
                    return Err(
                        req.unacceptable_with("path-outside-pages", t.get(Key::InvalidIncomplete))
                    );
                }
                (path.clone(), read(path)?)
            }
            None => {
                let title = file.title.trim();
                let path = title_to_path(title).map_err(|why| title_problem(req, &why))?;
                if read(&path)?.is_some() {
                    return Err(req.unacceptable_with(
                        "title-taken",
                        t.msg(Key::ProposeTitleTaken).with("title", title).text(),
                    ));
                }
                (path, None)
            }
        };
        if let Some(new) = &file.text {
            let judged = check_owned_keys(&Markup, old.as_deref(), new, || {
                req.services
                    .authorizer
                    .authorize(
                        &req.principal,
                        Action::Admin,
                        &inklkunn_core::Resource::Wiki {
                            wiki: req.wiki.id.clone(),
                        },
                    )
                    .is_allowed()
            });
            match judged {
                Ok(()) => {}
                Err(Front::Unreadable) => {
                    return Err(req.unacceptable_with(
                        "front-unreadable",
                        t.get(Key::ProposeFrontUnreadable),
                    ));
                }
                Err(Front::Owned(keys)) => {
                    return Err(req.unacceptable_with(
                        "front-owned",
                        t.msg(Key::ProposeFrontOwned)
                            .with("keys", keys.join(", "))
                            .text(),
                    ));
                }
            }
        }
        whole.push(Whole {
            path,
            old,
            new: file.text.clone(),
        });
    }
    Ok(ProposalEdit {
        summary: sent.summary,
        sources: sent.sources,
        files: whole
            .into_iter()
            .filter(|w| keep_all || w.new != w.old)
            .map(|w| FileChange {
                path: w.path,
                text: w.new,
            })
            .collect(),
        renames: sent.renames,
        minor: sent.minor,
        automated: sent.automated,
        base: None,
    })
}

/// A read of the base that failed: the repository could not be asked, or the path cannot be read.
fn read_failure(req: &Request, error: inklkunn_core::read::ReadError) -> Problem {
    use inklkunn_core::read::ReadError;
    match error {
        ReadError::InvalidTitle(_) | ReadError::PageUnreadable(_) | ReadError::NoSuchView(_) => {
            req.unacceptable(Invalid::BadBase)
        }
        ReadError::CouldNotBeReadNow(cause) => {
            req.log(&format!("a proposal's base could not be read now: {cause}"));
            Problem::new(crate::api::problem::Code::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests;
