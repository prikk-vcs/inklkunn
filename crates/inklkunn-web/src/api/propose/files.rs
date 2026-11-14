//! The pages of a proposal sent as JSON (RFC 004 D-2; threat model C-4a … C-4d): read from the body, **sized
//! before anything is parsed**, addressed, and checked for the information at the top of the page — the same
//! rules as the proposal form, in the same order, so a client can do nothing the form cannot.
//!
//! A file is `{"path": "pages/Home.md", "text": "…"}` for a page that exists (or `"text": null` to remove it), or
//! `{"title": "New page", "text": "…"}` for a page that does not exist yet (its path is made from the title by
//! the wiki's own rule). The text is the page's whole new text, carried as it is: no line ending is changed and
//! nothing is trimmed, because the text is data.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_core::Action;
use inklkunn_core::knowledge::compose::{page_id_of, without_expressions};
use inklkunn_core::knowledge::marker::words_with_id;
use inklkunn_core::knowledge::onpage::{claim_blocks, claims_at};
use inklkunn_core::knowledge::{AnchorState, Claim, anchor_state, claim_path};
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
use crate::proposals::parts::excerpt;

/// The most paragraphs one body names in `drops`.
const MAX_DROPS: usize = 50;

/// One file of the body.
#[derive(Debug)]
struct File {
    /// The page's path, or `None` for a page that does not exist yet.
    path: Option<String>,
    title: String,
    /// The whole new text; `None` removes the page.
    text: Option<String>,
}

/// A paragraph the caller names as removed: the page (its path, as in `files`) and the `^id` of the paragraph on it. A
/// bare `^id` would stand for the paragraphs of that name on every page of the proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Dropped {
    path: String,
    block: String,
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
    /// The paragraphs that others rely on and that the text leaves out, each named with its page by the caller: the explicit
    /// act of removing them (RFC 005 part 1 D-1 b; the form's confirmation box).
    drops: Vec<Dropped>,
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
    let mut drops = Vec::new();
    for mut drop in fields.objects("drops", MAX_DROPS)? {
        let path = drop.text("path")?;
        let block = drop.text("marker")?;
        drop.end()?;
        drops.push(Dropped { path, block });
    }
    Ok(Sent {
        summary,
        sources,
        files,
        renames,
        minor: fields.opt_flag("minor")?,
        automated: fields.opt_flag("automated")?,
        drops,
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
    view: Option<&str>,
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
    // A paragraph others rely on that the text leaves out is an act the caller names (RFC 005 part 1 D-1 b; K-T11): the
    // claims said there stop being said there, in the same proposal, and the reviewer sees it.
    let removals = drop_changes(req, view, base, &sent.drops, &whole)?;
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
            .chain(removals)
            .collect(),
        renames: sent.renames,
        minor: sent.minor,
        automated: sent.automated,
        base: None,
    })
}

/// The claim files a proposal comes to when it removes paragraphs that claims are said in.
///
/// **A removal is explicit.** For each page that exists at the base and has claims said in it (the tip's derived index
/// of the view), a paragraph whose `^id` the page's new text no longer has (or a page the proposal removes) must be named
/// in `drops` with its page, or the proposal is refused naming the page and the marker; a drop that is not such a
/// paragraph of that page is refused too, so that the flag never stands for something else, and a name on one page never
/// stands for the paragraph of that name on another. A named paragraph's claims are written without that expression, as
/// the form writes them; the claim stays, said nowhere, for a reviewer to decide about.
fn drop_changes(
    req: &Request,
    view: Option<&str>,
    base: &Base,
    drops: &[Dropped],
    whole: &[Whole],
) -> Result<Vec<FileChange>, Problem> {
    let t = req.t;
    let read = &req.services.read;
    let failed = |e| read_failure(req, e);
    let mut claims: BTreeMap<String, Claim> = BTreeMap::new();
    let mut removed: BTreeSet<(String, String)> = BTreeSet::new();
    for page in whole {
        let Some(old) = &page.old else { continue };
        let Some(page_id) = page_id_of(old) else {
            continue;
        };
        for block in claim_blocks(read, &req.wiki, view, old).map_err(failed)? {
            let gone = match &page.new {
                None => true,
                Some(new) => anchor_state(new, &block) == AnchorState::BlockMissing,
            };
            if !gone {
                continue;
            }
            let named = Dropped {
                path: page.path.clone(),
                block: block.clone(),
            };
            if !drops.contains(&named) {
                let words = words_with_id(old, &block, true).unwrap_or_default();
                return Err(req.unacceptable_with(
                    "claim-paragraph-removed",
                    t.msg(Key::ApiClaimLost)
                        .with("path", &page.path)
                        .with("block", &block)
                        .with("excerpt", excerpt(&words))
                        .text(),
                ));
            }
            let gone_blocks = BTreeSet::from([block.clone()]);
            for claim in claims_at(read, &req.wiki, view, &base.block_id, &page_id, &block)
                .map_err(failed)?
            {
                let current = claims
                    .get(claim.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| claim.clone());
                claims.insert(
                    claim.id.to_string(),
                    without_expressions(&current, &page_id, &gone_blocks),
                );
            }
            removed.insert((named.path, named.block));
        }
    }
    if let Some(unused) = drops
        .iter()
        .find(|d| !removed.contains(&(d.path.clone(), d.block.clone())))
    {
        return Err(req.unacceptable_with(
            "drop-not-removed",
            t.msg(Key::ApiDropUnused)
                .with("path", &unused.path)
                .with("block", &unused.block)
                .text(),
        ));
    }
    Ok(claims
        .into_values()
        .map(|claim| FileChange {
            path: claim_path(&claim.id),
            text: Some(claim.to_yaml()),
        })
        .collect())
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
