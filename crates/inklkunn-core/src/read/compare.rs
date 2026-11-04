//! Comparing two views (RFC 005 part 2 D-9; handoff 005 K-T8): **claim by claim and page by page, where two views
//! differ**, and nothing about which is right.
//!
//! ```text
//! compare  ─▶ the tip of each view ─▶ (same block? nothing differs)
//!          ─▶ the knowledge index of each tip (held, stored, or built from the one extract of the tip)
//!          ─▶ ONE `diff` between the two tip blocks, whole tree ─▶ the pages that differ, their changed lines counted
//! page     ─▶ ONE `diff --path <the page's path>` between the two tip blocks ─▶ the page's own diff, as RFC 003's
//! ```
//!
//! **The cost is the history page's.** prikk's `diff --path` filters only the output ("the work is whole-tree
//! either way", ledger), so one `diff` of the whole tree costs what a page's own diff costs, and it is asked once
//! per comparison; it goes through the driver's admission, deadline and memory ceiling like every other read. The
//! indexes are the ones the claim and title pages read. The lists are **bounded** ([`MAX_COMPARED_PAGES`],
//! [`MAX_COMPARED_CLAIMS`]); what is left out is counted, never silently dropped.
//!
//! **Both views must be open views of the wiki** (a view that is not is [`ReadError::NoSuchView`], the answer for a
//! name that was never a view), and the surface decides whether the visitor may read the wiki before it asks.
//! Nothing here names a view better than the other: the two are `a` and `b`, in the order the caller gave.

use inklkunn_markup::path::{path_to_title, title_to_path};
use inklkunn_prikk::Point;
use inklkunn_store::WikiRecord;

use super::history::{DiffShape, DiffStatus, shape_diff, status_of};
use super::linediff::Hunk;
use super::{ReadError, ReadService, Tip};
use crate::knowledge::compare::{ClaimDifference, compare};
use crate::knowledge::index::{KnowledgeIndex, PRESENT};
use inklkunn_store::KnowledgeAnchorRow;

/// The most pages a comparison lists.
pub const MAX_COMPARED_PAGES: usize = 200;

/// The most claims a comparison lists.
pub const MAX_COMPARED_CLAIMS: usize = 200;

/// A page the two views have differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDifference {
    /// The page's title (the one its path spells).
    pub title: String,
    /// The page's path.
    pub path: String,
    /// `Added`: only the second view has it. `Removed`: only the first. `Modified`: both, and they differ.
    pub status: DiffStatus,
    /// Lines the second view has that the first does not, when prikk gave lines.
    pub added: u64,
    /// Lines the first view has that the second does not, when prikk gave lines.
    pub removed: u64,
}

/// A claim that differs, and where it is said (a page's title), so that a reader can tell it from another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparedClaim {
    /// How it differs.
    pub difference: ClaimDifference,
    /// The title of the first page that says it now, in the first view (else the second).
    pub said_in: Option<String>,
}

/// Two views, compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewComparison {
    /// The first view's tip.
    pub a: Tip,
    /// The second view's tip.
    pub b: Tip,
    /// The two tips are one and the same change: nothing differs.
    pub same: bool,
    /// At most [`MAX_COMPARED_PAGES`] pages that differ, in path order.
    pub pages: Vec<PageDifference>,
    /// How many more pages differ and are not listed.
    pub pages_omitted: usize,
    /// At most [`MAX_COMPARED_CLAIMS`] claims that differ, in claim order.
    pub claims: Vec<ComparedClaim>,
    /// How many more claims differ and are not listed.
    pub claims_omitted: usize,
}

/// One page's diff between two views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewPageOutcome {
    /// The page, as the two views have it.
    Found(Box<ViewPageDiff>),
    /// Neither view has a page at that title.
    Absent,
}

/// A page's diff from the first view's tip to the second's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewPageDiff {
    /// The title as asked.
    pub title: String,
    /// The page's path.
    pub path: String,
    /// The first view's tip.
    pub a: Tip,
    /// The second view's tip.
    pub b: Tip,
    /// Its shape, as RFC 003's page diff has it.
    pub shape: ViewPageShape,
}

/// What a page's diff between two views came to (RFC 003's bounds: at most
/// [`MAX_DIFF_LINES`](super::MAX_DIFF_LINES) lines are kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewPageShape {
    /// How the page differs.
    pub status: DiffStatus,
    /// The hunks, as lines with the word marks.
    pub hunks: Vec<super::HunkView>,
    /// Lines added, over all hunks prikk gave.
    pub added: u64,
    /// Lines removed.
    pub removed: u64,
    /// More lines were given than are kept.
    pub truncated: bool,
    /// prikk gave no lines for a page that differs.
    pub no_lines: bool,
}

impl From<DiffShape> for ViewPageShape {
    fn from(s: DiffShape) -> Self {
        ViewPageShape {
            status: s.status,
            hunks: s.hunks,
            added: s.added,
            removed: s.removed,
            truncated: s.truncated,
            no_lines: s.no_lines,
        }
    }
}

/// The title of the first page that says the claim, at a block it really has, in the first index that has one.
fn said_in(id: &str, indexes: [&KnowledgeIndex; 2]) -> Option<String> {
    indexes.into_iter().find_map(|index| {
        index
            .anchors_of(id)
            .filter(|a: &&KnowledgeAnchorRow| a.state == PRESENT)
            .find_map(|a| a.page_path.as_deref().and_then(path_to_title))
    })
}

impl ReadService {
    /// Where the views `a` and `b` (the wiki's default view when `None`) differ: the claims, and the pages. One
    /// `diff` of the two tips, whole tree, and the two knowledge indexes; none of it when the tips are one change.
    pub fn compare_views(
        &self,
        wiki: &WikiRecord,
        a: Option<&str>,
        b: Option<&str>,
    ) -> Result<ViewComparison, ReadError> {
        let (tip_a, tip_b) = (self.tip(wiki, a)?, self.tip(wiki, b)?);
        if tip_a.block_id == tip_b.block_id {
            return Ok(ViewComparison {
                a: tip_a,
                b: tip_b,
                same: true,
                pages: Vec::new(),
                pages_omitted: 0,
                claims: Vec::new(),
                claims_omitted: 0,
            });
        }
        let (index_a, index_b) = (
            self.knowledge_index(wiki, a)?,
            self.knowledge_index(wiki, b)?,
        );
        let diff = self
            .storage
            .diff(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip_a.block_id.clone()),
                &Point::Block(tip_b.block_id.clone()),
                &[],
            )
            .map_err(|e| self.storage_error(wiki, e))?;

        let mut pages = Vec::new();
        let mut pages_omitted = 0;
        for entry in &diff.entries {
            let Some(title) = path_to_title(&entry.path) else {
                continue;
            };
            if pages.len() >= MAX_COMPARED_PAGES {
                pages_omitted += 1;
                continue;
            }
            // Only the lines are counted: a list of pages does not pay for the word marks of each.
            let (added, removed) = entry
                .hunks
                .iter()
                .filter_map(|raw| Hunk::parse(raw))
                .fold((0_u64, 0_u64), |(a, r), h| (a + h.added(), r + h.removed()));
            pages.push(PageDifference {
                title,
                path: entry.path.clone(),
                status: status_of(Some(entry.status.as_str())),
                added,
                removed,
            });
        }

        let mut differing = compare(&index_a, &index_b);
        let claims_omitted = differing.len().saturating_sub(MAX_COMPARED_CLAIMS);
        differing.truncate(MAX_COMPARED_CLAIMS);
        let claims = differing
            .into_iter()
            .map(|difference| ComparedClaim {
                said_in: said_in(&difference.id, [&index_a, &index_b]),
                difference,
            })
            .collect();
        Ok(ViewComparison {
            a: tip_a,
            b: tip_b,
            same: false,
            pages,
            pages_omitted,
            claims,
            claims_omitted,
        })
    }

    /// How the page titled `title` differs between the tips of the views `a` and `b`: **one `diff` with the page's
    /// path**, lines as RFC 003's page diff has them, bounded as it is. A page neither view has is
    /// [`ViewPageOutcome::Absent`].
    pub fn compare_page(
        &self,
        wiki: &WikiRecord,
        a: Option<&str>,
        b: Option<&str>,
        title: &str,
    ) -> Result<ViewPageOutcome, ReadError> {
        let path = title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let (tip_a, tip_b) = (self.tip(wiki, a)?, self.tip(wiki, b)?);
        let (index_a, index_b) = (self.title_index(wiki, a)?, self.title_index(wiki, b)?);
        if index_a.entry(&path).is_none() && index_b.entry(&path).is_none() {
            return Ok(ViewPageOutcome::Absent);
        }
        let diff = self
            .storage
            .diff(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip_a.block_id.clone()),
                &Point::Block(tip_b.block_id.clone()),
                &[path.as_str()],
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        Ok(ViewPageOutcome::Found(Box::new(ViewPageDiff {
            title: title.trim().to_owned(),
            shape: shape_diff(&diff, &path).into(),
            path,
            a: tip_a,
            b: tip_b,
        })))
    }
}
