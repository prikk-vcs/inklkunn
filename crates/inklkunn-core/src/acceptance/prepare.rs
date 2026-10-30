//! What an acceptance writes (RFC 004 D-4 steps 2 and 3, D-5): the revision's pages as they are when the view
//! has not moved since the proposal was made, else the revision **re-authored against the tip** by the
//! content-level merge, or the overlaps that stop it.
//!
//! *Moved* is judged by the block: the revision's base block against the tip's. When it has moved, three sides
//! are read for the pages the revision touches (and only those): the **base** (`cat` at the revision's base
//! block), **theirs** (`cat` at the tip) and **ours** (the base with the revision applied: a page's whole new
//! text, a removal, and each declared rename moving the page, with its text, from the old path to the new). The
//! merge's clean result is then turned into the change *relative to the tip*: only the pages whose text differs
//! there are written, a page the result no longer has is removed, and a declared rename the tip can still carry
//! out (it has the old path, not the new) is kept a rename (`prikk mv`), so that the page's identity survives.
//!
//! A page the tip renamed away from the path the proposal edited is not followed: the pages are matched by
//! page id when the tip still has the page at a path the proposal touches, and otherwise the proposal's page
//! is an edit of something the tip removed, which the merge reports as an overlap (a person decides). That is
//! the conservative reading, and it never loses an edit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use inklkunn_markup::{BlockKind, blocks, frontmatter};
use inklkunn_prikk::{Point, Storage, StorageError};
use inklkunn_signer::PreparedFile;
use inklkunn_store::{CommentRecord, OverlapText, RevisionContent};

use crate::merge::{MergeOutcome, Overlap, PageText, Place, merge};
use crate::proposal::MAX_PAGE_BYTES;

/// What was prepared.
#[derive(Debug)]
pub(crate) enum Prepared {
    /// The files and renames to hand the signer; `reauthored` when the merge made them.
    Change {
        files: Vec<PreparedFile>,
        renames: Vec<(PathBuf, PathBuf)>,
        reauthored: bool,
    },
    /// The merge stopped at these overlaps; nothing is to be written.
    Overlaps(Vec<OverlapText>),
    /// The tip already has everything the revision asks for.
    NothingToChange,
}

/// Why the sides could not be read.
#[derive(Debug)]
pub(crate) enum PrepareError {
    /// prikk could not be asked.
    Storage(StorageError),
    /// A page at the base or the tip is not text, or is over the page bound.
    Unreadable,
}

impl From<StorageError> for PrepareError {
    fn from(e: StorageError) -> Self {
        PrepareError::Storage(e)
    }
}

fn files_at(
    storage: &dyn Storage,
    repo: &Path,
    block: &str,
) -> Result<BTreeSet<String>, StorageError> {
    let tree = storage.tree(repo, &Point::Block(block.to_owned()), None)?;
    Ok(tree
        .entries
        .into_iter()
        .filter(|e| e.kind == "file")
        .map(|e| e.path)
        .collect())
}

/// The text of the touched pages that exist at `block`.
fn pages_at(
    storage: &dyn Storage,
    repo: &Path,
    block: &str,
    touched: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, PrepareError> {
    let present = files_at(storage, repo, block)?;
    let mut out = BTreeMap::new();
    for path in touched.iter().filter(|p| present.contains(*p)) {
        let bytes = storage.read(
            repo,
            &Point::Block(block.to_owned()),
            path,
            MAX_PAGE_BYTES as u64,
        )?;
        let text = String::from_utf8(bytes).map_err(|_| PrepareError::Unreadable)?;
        out.insert(path.clone(), text);
    }
    Ok(out)
}

fn texts(map: &BTreeMap<String, String>) -> Vec<PageText> {
    map.iter()
        .map(|(p, t)| PageText::new(p.clone(), t.clone()))
        .collect()
}

fn describe(place: &Place) -> String {
    match place {
        Place::Frontmatter { key } if key.is_empty() => "frontmatter".to_owned(),
        Place::Frontmatter { key } => format!("frontmatter {key}"),
        Place::Block { id: Some(id), .. } => format!("block ^{id}"),
        Place::Block {
            base, theirs, ours, ..
        } => match theirs.or(*ours).or(*base) {
            Some(n) => format!("block {n}"),
            None => "block".to_owned(),
        },
        Place::Page => "page".to_owned(),
        Place::Path => "path".to_owned(),
    }
}

fn overlap_text(o: Overlap) -> OverlapText {
    OverlapText {
        page: o.page.path,
        place: describe(&o.place),
        base: o.base,
        theirs: o.theirs,
        ours: o.ours,
    }
}

fn prepared_file(path: &str, text: Option<String>) -> PreparedFile {
    PreparedFile {
        path: PathBuf::from(path),
        content: text.map(String::into_bytes),
    }
}

/// The revision's files and renames as they stand, for a view that has not moved.
fn direct(content: &RevisionContent) -> Prepared {
    Prepared::Change {
        files: content
            .files
            .iter()
            .map(|f| prepared_file(&f.path, f.text.clone()))
            .collect(),
        renames: content
            .renames
            .iter()
            .map(|r| (PathBuf::from(&r.from), PathBuf::from(&r.to)))
            .collect(),
        reauthored: false,
    }
}

/// Prepare the change the revision asks for, against the tip `tip_block`.
pub(crate) fn prepare(
    storage: &dyn Storage,
    repo: &Path,
    content: &RevisionContent,
    tip_block: &str,
) -> Result<Prepared, PrepareError> {
    if content.base.block_id == tip_block {
        return Ok(direct(content));
    }
    let touched: BTreeSet<String> = content
        .files
        .iter()
        .map(|f| f.path.clone())
        .chain(
            content
                .renames
                .iter()
                .flat_map(|r| [r.from.clone(), r.to.clone()]),
        )
        .collect();
    let base = pages_at(storage, repo, &content.base.block_id, &touched)?;
    let tip = pages_at(storage, repo, tip_block, &touched)?;

    // Ours: the base with the revision applied.
    let mut ours = base.clone();
    for r in &content.renames {
        if let Some(text) = ours.remove(&r.from) {
            ours.insert(r.to.clone(), text);
        }
    }
    for f in &content.files {
        match &f.text {
            Some(text) => {
                ours.insert(f.path.clone(), text.clone());
            }
            None => {
                ours.remove(&f.path);
            }
        }
    }

    let merged = match merge(&texts(&base), &texts(&tip), &texts(&ours)) {
        MergeOutcome::Overlaps { overlaps } => {
            return Ok(Prepared::Overlaps(
                overlaps.into_iter().map(overlap_text).collect(),
            ));
        }
        MergeOutcome::Clean { pages } => pages
            .into_iter()
            .map(|p| (p.path, p.text))
            .collect::<BTreeMap<String, String>>(),
    };

    // The declared renames the tip can still carry out: it has the old path and not the new one, and the
    // merged result has the new path and not the old one.
    let renames: Vec<(String, String)> = content
        .renames
        .iter()
        .filter(|r| {
            tip.contains_key(&r.from)
                && !tip.contains_key(&r.to)
                && merged.contains_key(&r.to)
                && !merged.contains_key(&r.from)
        })
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();
    let moved_from: BTreeMap<&str, &str> = renames
        .iter()
        .map(|(f, t)| (t.as_str(), f.as_str()))
        .collect();
    let mut files: Vec<PreparedFile> = Vec::new();
    for (path, text) in &merged {
        let was = match moved_from.get(path.as_str()) {
            Some(from) => tip.get(*from),
            None => tip.get(path),
        };
        if was != Some(text) {
            files.push(prepared_file(path, Some(text.clone())));
        }
    }
    let gone: BTreeSet<&str> = renames.iter().map(|(f, _)| f.as_str()).collect();
    for path in tip.keys() {
        if !merged.contains_key(path) && !gone.contains(path.as_str()) {
            files.push(prepared_file(path, None));
        }
    }
    if files.is_empty() && renames.is_empty() {
        return Ok(Prepared::NothingToChange);
    }
    Ok(Prepared::Change {
        files,
        renames: renames
            .into_iter()
            .map(|(f, t)| (PathBuf::from(f), PathBuf::from(t)))
            .collect(),
        reauthored: true,
    })
}

/// The bytes an acceptance counts against the accepted-bytes quota: every path and every text, as a revision's
/// content counts them.
pub(crate) fn change_bytes(files: &[PreparedFile], renames: &[(PathBuf, PathBuf)]) -> u64 {
    let files: usize = files
        .iter()
        .map(|f| f.path.as_os_str().len() + f.content.as_ref().map_or(0, Vec::len))
        .sum();
    let renames: usize = renames
        .iter()
        .map(|(a, b)| a.as_os_str().len() + b.as_os_str().len())
        .sum();
    u64::try_from(files + renames).unwrap_or(u64::MAX)
}

// --- CM-5: a block a comment anchored to gets its `^id` in the same change

/// The id a comment's block is given: `c` and the last eight hex characters of the comment's id (the random
/// part of a UUID v7).
fn id_for(comment: &CommentRecord, taken: &BTreeSet<String>) -> String {
    let compact: String = comment.id.as_str().chars().filter(|c| *c != '-').collect();
    let tail: String = compact
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut id = format!("c{tail}");
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("c{tail}-{n}");
        n += 1;
    }
    id
}

/// Attach a `^id` to the block of `text` that `comment` is anchored to, if the comment names no `^id`, the
/// block can carry one, and it can be found without doubt. `None` when nothing was to be done (or could be
/// done safely): the comment keeps its quote selector.
///
/// The block is found by the quote's exact text (the block whose text holds it; when several do, the one at the
/// comment's ordinal). The id is written where the renderer reads one (the end of a paragraph's last line, or a
/// line of its own right after a table or a code block) and the page is **read again**: the change is kept only
/// if the same blocks come back and the block carries the new id. A heading, a list and a quotation are not
/// given one (an identifier on a heading line is not one, and where a list's identifier goes is a question of
/// the list); those comments stay anchored by their quote.
fn attach_one(text: &str, comment: &CommentRecord) -> Option<String> {
    let anchor = comment.anchor.as_ref()?;
    if anchor.block_id.is_some() {
        return None;
    }
    let quote = &anchor.quote.as_ref()?.exact;
    let (_, body, _) = frontmatter::split(text);
    let offset = text.len().checked_sub(body.len())?;
    let before = blocks(body);
    let holding: Vec<usize> = before
        .iter()
        .enumerate()
        .filter(|(_, b)| b.text.contains(quote.as_str()))
        .map(|(i, _)| i)
        .collect();
    let at = match holding.as_slice() {
        [only] => *only,
        many => *many
            .iter()
            .find(|i| u32::try_from(**i).ok() == Some(anchor.ordinal))?,
    };
    let block = before.get(at)?;
    if block.id.is_some() {
        return None;
    }
    let taken: BTreeSet<String> = before
        .iter()
        .filter_map(|b| b.id.map(str::to_owned))
        .collect();
    let id = id_for(comment, &taken);
    let end = offset + block.range.end;
    let insertion = match block.kind {
        BlockKind::Paragraph => format!(" ^{id}"),
        BlockKind::Table | BlockKind::CodeBlock => format!("\n^{id}"),
        _ => return None,
    };
    let mut changed = String::with_capacity(text.len() + insertion.len());
    changed.push_str(text.get(..end)?);
    changed.push_str(&insertion);
    changed.push_str(text.get(end..)?);
    // Read it again.
    let (_, new_body, _) = frontmatter::split(&changed);
    let after = blocks(new_body);
    let same_shape =
        after.len() == before.len()
            && after.iter().zip(&before).enumerate().all(|(i, (a, b))| {
                a.kind == b.kind && (i == at || (a.text == b.text && a.id == b.id))
            });
    (same_shape && after.get(at).and_then(|b| b.id) == Some(id.as_str())).then_some(changed)
}

/// Give every commented block that has no `^id` one, in the pages the change writes (CM-5), so that the comment's
/// anchor survives into the history. Returns how many were given. A comment about a page the change does not
/// write is left alone.
pub(crate) fn attach_ids(files: &mut [PreparedFile], comments: &[CommentRecord]) -> usize {
    let mut given = 0;
    for comment in comments {
        let Some(anchor) = &comment.anchor else {
            continue;
        };
        let Some(file) = files
            .iter_mut()
            .find(|f| f.path.to_str() == Some(anchor.page.as_str()))
        else {
            continue;
        };
        let Some(text) = file
            .content
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
        else {
            continue;
        };
        if let Some(changed) = attach_one(text, comment) {
            file.content = Some(changed.into_bytes());
            given += 1;
        }
    }
    given
}

#[cfg(test)]
mod tests;
