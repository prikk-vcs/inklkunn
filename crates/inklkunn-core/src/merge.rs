//! The content-level merge (RFC 004 D-5; internal design MRG-1 … MRG-3): the function that re-authors a
//! proposal whose base moved, exact and pure.
//!
//! Three sides, each a set of pages: the **base** (the pages as the proposal's author saw them), **theirs** (the
//! pages at the view's tip now) and **ours** (the pages the proposal asks for). A page absent from a side is
//! removed on that side.
//!
//! - **Pages are matched by their page id** (the `id` of the frontmatter), so a rename on one side and an
//!   edit on the other land together: the edit is on the renamed file. A page with no id is matched by its
//!   path. A page removed on one side and changed (edited, or renamed) on the other is an overlap; so are two
//!   different new paths for one page, and two pages at one path.
//! - **The frontmatter is merged key by key.** A key one side changed is taken; a key both sides changed
//!   alike is taken once; a key both changed differently is an overlap.
//! - **The body is merged block by block**, the blocks being `inklkunn-markup`'s top-level blocks
//!   ([`inklkunn_markup::blocks`]; internal design MK-5) compared with the whitespace around them trimmed.
//!   Blocks are matched across the three sides by `^id`, then by content, then by a longest-common-subsequence
//!   alignment of what remains. A block changed, added or removed on one side only is taken; one
//!   changed alike on both sides is taken once; **one changed on both sides differently is an overlap**.
//!   Two sides that each add a different block at the same place overlap too: which comes first is a
//!   decision for a person.
//! - **An overlap is never resolved here** (MRG-2, INT-7). The outcome carries both texts and where they are;
//!   the proposal's author revises.
//! - **Within a block nothing is merged.** The measurement hook only counts how many overlaps a line-level
//!   merge of the block would have declared clean ([`LineLevel`]); the number is for the decision RFC 004
//!   defers to M3, and never changes an outcome.
//!
//! Pure: texts in, an outcome out; no I/O, no clock, no randomness. Work is linear in the text apart from the
//! alignment of unmatched blocks, which is bounded (a stretch whose table would pass 250 000 cells is left unaligned).

mod engine;
mod line;
mod page;

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_markup::parse_metadata;

use page::merge_text;

/// Which block of the page text `other` is the same block as each block of the page text `base` (their top-level
/// blocks, [`inklkunn_markup::blocks`]), by **the alignment the merge itself makes** between a base and a side: the same
/// `^id`, then the same unique text, then a bounded longest-common-subsequence of what remains, where a stretch of one
/// block on each side is one block that changed. `None`: the block has no counterpart (removed). The merge is not
/// run and nothing is decided; this is for measurement tools that must use the product's own matching (spike C),
/// never a copy of it.
pub fn align_blocks(base: &str, other: &str) -> Vec<Option<usize>> {
    page::align_blocks(base, other)
}

/// A page's path and text, as one side of a merge has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageText {
    /// The repository path, `pages/Title.md`.
    pub path: String,
    /// The file's text.
    pub text: String,
}

impl PageText {
    /// A page from its path and text.
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        PageText {
            path: path.into(),
            text: text.into(),
        }
    }
}

/// A page of the merged result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedPage {
    /// Where the page is to be.
    pub path: String,
    /// What it is to say.
    pub text: String,
}

/// Which page an overlap is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRef {
    /// The page id, when the page has one.
    pub id: Option<String>,
    /// Its path: the proposal's, else the tip's, else the base's.
    pub path: String,
}

/// Where in a page two changes overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A frontmatter key; the empty key is the text before the first key, or the whole frontmatter when it
    /// could not be read key by key.
    Frontmatter {
        /// The key.
        key: String,
    },
    /// A block, or a run of blocks that cannot be told apart from one. The numbers count the page's body
    /// blocks from 1 on each side, and are `None` where that side has none there (a block it removed, or one
    /// only the other side added).
    Block {
        /// The block's `^id`, when it has one.
        id: Option<String>,
        /// Its number in the base.
        base: Option<usize>,
        /// Its number in the tip's page.
        theirs: Option<usize>,
        /// Its number in the proposal's page.
        ours: Option<usize>,
    },
    /// The page itself: removed on one side, changed on the other.
    Page,
    /// The page's path: moved to two places, or two pages at one path.
    Path,
}

/// What a line-level three-way merge of an overlapping block would have made of it (the measurement hook;
/// never applied).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineLevel {
    /// Clean: the two sides changed different lines.
    WouldBeClean,
    /// The two sides changed the same lines, or lines that touch.
    WouldConflict,
    /// The overlap is not about text (a removed page, a path).
    NotText,
}

/// Two changes that cannot both be taken: both texts, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    /// The page.
    pub page: PageRef,
    /// Where in the page.
    pub place: Place,
    /// The text there in the base; `None` when the base has none (something both sides added).
    pub base: Option<String>,
    /// The text there at the tip; `None` when the tip removed it. For a page overlap the whole page, for a
    /// path overlap the path.
    pub theirs: Option<String>,
    /// The text there in the proposal; `None` when the proposal removed it.
    pub ours: Option<String>,
    /// The measurement hook's answer.
    pub line_level: LineLevel,
}

/// The result of a merge, in the words of RFC 004 D-12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Clean: the pages the merged change leaves, by path. A page of the base that is not here is removed.
    Clean {
        /// The merged pages, in path order.
        pages: Vec<MergedPage>,
    },
    /// *Stopped: changes overlap.* Nothing is merged; every overlap is reported, in page and place order.
    Overlaps {
        /// The overlaps, each with both texts.
        overlaps: Vec<Overlap>,
    },
}

impl MergeOutcome {
    /// Whether the merge was clean.
    pub fn is_clean(&self) -> bool {
        matches!(self, MergeOutcome::Clean { .. })
    }

    /// How many of the overlaps a line-level merge of the block would have declared clean: the measurement
    /// hook's count (RFC 004 D-5). Zero for a clean merge. It never changes the outcome.
    pub fn line_level_clean(&self) -> usize {
        match self {
            MergeOutcome::Clean { .. } => 0,
            MergeOutcome::Overlaps { overlaps } => overlaps
                .iter()
                .filter(|o| o.line_level == LineLevel::WouldBeClean)
                .count(),
        }
    }
}

/// How a page is told apart across the three sides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    /// Its page id.
    Id(String),
    /// Its path, for a page with no usable id.
    Path(String),
}

/// A side's pages by identity. An id that two pages of the side share names neither: both fall back to
/// their paths.
fn identify(pages: &[PageText]) -> BTreeMap<Identity, &PageText> {
    let ids: Vec<Option<String>> = pages
        .iter()
        .map(|p| {
            parse_metadata(&p.text)
                .ok()
                .and_then(|m| m.id)
                .filter(|id| !id.is_empty())
        })
        .collect();
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for id in ids.iter().flatten() {
        *count.entry(id.as_str()).or_default() += 1;
    }
    let mut out = BTreeMap::new();
    for (page, id) in pages.iter().zip(&ids) {
        let key = match id {
            Some(id) if count.get(id.as_str()) == Some(&1) => Identity::Id(id.clone()),
            _ => Identity::Path(page.path.clone()),
        };
        // The first of two pages at one path wins the key; the caller's sides name each path once.
        out.entry(key).or_insert(page);
    }
    out
}

/// The three-way merge of a set of pages.
pub fn merge(base: &[PageText], theirs: &[PageText], ours: &[PageText]) -> MergeOutcome {
    let (b, t, o) = (identify(base), identify(theirs), identify(ours));
    let identities: BTreeSet<&Identity> = b.keys().chain(t.keys()).chain(o.keys()).collect();
    let mut merged: Vec<(Identity, MergedPage)> = Vec::new();
    let mut overlaps: Vec<Overlap> = Vec::new();
    for identity in identities {
        let (pb, pt, po) = (b.get(identity), t.get(identity), o.get(identity));
        let page = |p: Option<&&PageText>| p.map(|p| p.path.clone());
        let reference = PageRef {
            id: match identity {
                Identity::Id(id) => Some(id.clone()),
                Identity::Path(_) => None,
            },
            path: page(po)
                .or_else(|| page(pt))
                .or_else(|| page(pb))
                .unwrap_or_default(),
        };
        let whole =
            |why: Place, bt: Option<&PageText>, tt: Option<&PageText>, ot: Option<&PageText>| {
                Overlap {
                    page: reference.clone(),
                    place: why,
                    base: bt.map(|p| p.text.clone()),
                    theirs: tt.map(|p| p.text.clone()),
                    ours: ot.map(|p| p.text.clone()),
                    line_level: LineLevel::NotText,
                }
            };
        match (pb.copied(), pt.copied(), po.copied()) {
            // Gone from every side.
            (_, None, None) => {}
            // Added on one side only (or by both, identically below).
            (None, Some(x), None) | (None, None, Some(x)) => {
                merged.push((
                    identity.clone(),
                    MergedPage {
                        path: x.path.clone(),
                        text: x.text.clone(),
                    },
                ));
            }
            // Removed on one side: that is taken if the other side left the page as it was.
            (Some(base), Some(x), None) | (Some(base), None, Some(x)) => {
                if x.path == base.path && x.text == base.text {
                    continue;
                }
                let (tip, mine) = if pt.is_some() {
                    (Some(x), None)
                } else {
                    (None, Some(x))
                };
                overlaps.push(whole(Place::Page, Some(base), tip, mine));
            }
            // On both sides.
            (base, Some(tip), Some(mine)) => {
                let path = match base {
                    _ if tip.path == mine.path => Some(tip.path.clone()),
                    Some(base) if tip.path == base.path => Some(mine.path.clone()),
                    Some(base) if mine.path == base.path => Some(tip.path.clone()),
                    _ => None,
                };
                let Some(path) = path else {
                    overlaps.push(Overlap {
                        page: reference.clone(),
                        place: Place::Path,
                        base: base.map(|p| p.path.clone()),
                        theirs: Some(tip.path.clone()),
                        ours: Some(mine.path.clone()),
                        line_level: LineLevel::NotText,
                    });
                    continue;
                };
                // A side that left the text as it was changed nothing in it; the engine would say the same.
                let base_text = base.map_or("", |p| p.text.as_str());
                let text = if tip.text == mine.text || mine.text == base_text {
                    Ok(tip.text.clone())
                } else if tip.text == base_text {
                    Ok(mine.text.clone())
                } else {
                    merge_text(base_text, &tip.text, &mine.text)
                };
                match text {
                    Ok(text) => merged.push((identity.clone(), MergedPage { path, text })),
                    Err(found) => overlaps.extend(found.into_iter().map(|f| f.on(&reference))),
                }
            }
        }
    }

    // Two pages at one path.
    let mut at: BTreeMap<&str, Vec<&Identity>> = BTreeMap::new();
    for (identity, page) in &merged {
        at.entry(page.path.as_str()).or_default().push(identity);
    }
    for (path, identities) in at {
        if identities.len() < 2 {
            continue;
        }
        let text_at =
            |side: &[PageText]| side.iter().find(|p| p.path == path).map(|p| p.text.clone());
        overlaps.push(Overlap {
            page: PageRef {
                id: match identities.first() {
                    Some(Identity::Id(id)) => Some(id.clone()),
                    _ => None,
                },
                path: path.to_owned(),
            },
            place: Place::Path,
            base: text_at(base).map(|_| path.to_owned()),
            theirs: text_at(theirs),
            ours: text_at(ours),
            line_level: LineLevel::NotText,
        });
    }

    if overlaps.is_empty() {
        let mut pages: Vec<MergedPage> = merged.into_iter().map(|(_, p)| p).collect();
        pages.sort_by(|a, b| a.path.cmp(&b.path));
        MergeOutcome::Clean { pages }
    } else {
        MergeOutcome::Overlaps { overlaps }
    }
}

#[cfg(test)]
mod tests;
