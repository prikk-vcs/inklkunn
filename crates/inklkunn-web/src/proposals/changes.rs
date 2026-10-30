//! What a proposal changes, page by page and part by part (RFC 004 D-3; WEB-04), computed from the page as the
//! proposal was written against it (the base) and the text the proposal holds.
//!
//! **Parts are the markup crate's blocks** (`inklkunn_markup::blocks`, the same segmentation the merge and the
//! comments' anchors use, so what is shown as one part is what is merged as one unit). The two lists of parts
//! are aligned by `^id` where a part has one and by its text otherwise, with a longest-common-subsequence over
//! what is left; parts between two matches that are not matched are paired in order as *changed* and the rest
//! are *added* or *removed*. Within a changed part the lines are compared again, and that line diff is written
//! as a unified hunk and read back by the same `linediff` the history's differences use (its word marks
//! included), so a proposal and a history show a change the same way.
//!
//! **Bounded work.** The alignment is quadratic in the number of parts, so it is run only on what the common
//! start and end of the two lists do not settle, and only up to [`MAX_CELLS`] cells; beyond that the middle is
//! shown as removed and added, which is true. The same holds for the lines of one part.

use inklkunn_core::read::{HunkView, MAX_DIFF_LINES};
use inklkunn_markup::{Block, blocks, frontmatter};

/// The most cells of a table of alignment one comparison may fill.
const MAX_CELLS: usize = 4_000_000;

/// What became of one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// The proposal brings the page.
    Added,
    /// The proposal removes the page.
    Removed,
    /// The page's text differs.
    Modified,
    /// The text is the page's own.
    Unchanged,
}

/// How one part differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum How {
    /// Present on both sides, different.
    Changed,
    /// Only in the new text.
    Added,
    /// Only in the old text.
    Removed,
}

/// One part that differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartDelta {
    pub(crate) how: How,
    /// Its position among the new text's parts (for added and changed parts).
    pub(crate) ordinal: Option<usize>,
    /// Its position among the old text's parts (for removed and changed parts).
    pub(crate) old_ordinal: Option<usize>,
    pub(crate) old: Option<String>,
    pub(crate) new: Option<String>,
}

/// How a page differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageDelta {
    pub(crate) status: Status,
    /// The information at the top of the page, when it differs: the old and the new text of it.
    pub(crate) info: Option<(String, String)>,
    /// The parts that differ, in order.
    pub(crate) parts: Vec<PartDelta>,
    /// How many parts are the same on both sides.
    pub(crate) unchanged: usize,
}

/// The body of a page and its information, as separate texts.
fn split(text: &str) -> (&str, &str) {
    let (info, body, _) = frontmatter::split(text);
    (info.unwrap_or(""), body)
}

/// How `new` differs from `old`; `None` on either side is a page that is not there.
pub(crate) fn delta(old: Option<&str>, new: Option<&str>) -> PageDelta {
    let status = match (old, new) {
        (None, Some(_)) => Status::Added,
        (Some(_), None) => Status::Removed,
        (Some(a), Some(b)) if a != b => Status::Modified,
        _ => Status::Unchanged,
    };
    let (old_info, old_body) = split(old.unwrap_or(""));
    let (new_info, new_body) = split(new.unwrap_or(""));
    let info = (old_info != new_info).then(|| (old_info.to_owned(), new_info.to_owned()));
    let a = blocks(old_body);
    let b = blocks(new_body);
    let matched = align(&a, &b);
    let mut parts = Vec::new();
    let mut unchanged = 0;
    let (mut i, mut j) = (0, 0);
    let gap = |from: (usize, usize), to: (usize, usize), parts: &mut Vec<PartDelta>| {
        let removed = a.get(from.0..to.0).unwrap_or(&[]);
        let added = b.get(from.1..to.1).unwrap_or(&[]);
        for k in 0..removed.len().max(added.len()) {
            let (r, n) = (removed.get(k), added.get(k));
            parts.push(PartDelta {
                how: match (r, n) {
                    (Some(_), Some(_)) => How::Changed,
                    (None, _) => How::Added,
                    (_, None) => How::Removed,
                },
                ordinal: n.map(|_| from.1 + k),
                old_ordinal: r.map(|_| from.0 + k),
                old: r.map(|b| b.text.to_owned()),
                new: n.map(|b| b.text.to_owned()),
            });
        }
    };
    for (mi, mj) in matched {
        gap((i, j), (mi, mj), &mut parts);
        if let (Some(x), Some(y)) = (a.get(mi), b.get(mj)) {
            if x.text == y.text {
                unchanged += 1;
            } else {
                parts.push(PartDelta {
                    how: How::Changed,
                    ordinal: Some(mj),
                    old_ordinal: Some(mi),
                    old: Some(x.text.to_owned()),
                    new: Some(y.text.to_owned()),
                });
            }
        }
        (i, j) = (mi + 1, mj + 1);
    }
    gap((i, j), (a.len(), b.len()), &mut parts);
    PageDelta {
        status,
        info,
        parts,
        unchanged,
    }
}

/// Do these two parts correspond: the same `^id`, or the same text?
fn same(x: &Block<'_>, y: &Block<'_>) -> bool {
    match (x.id, y.id) {
        (Some(p), Some(q)) if p == q => true,
        _ => x.text == y.text,
    }
}

/// The pairs `(old index, new index)` of corresponding parts, in order.
fn align(a: &[Block<'_>], b: &[Block<'_>]) -> Vec<(usize, usize)> {
    let start = a.iter().zip(b).take_while(|(x, y)| same(x, y)).count();
    let end = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(a.len().min(b.len()) - start)
        .take_while(|(x, y)| same(x, y))
        .count();
    let mut out: Vec<(usize, usize)> = (0..start).map(|k| (k, k)).collect();
    let (ma, mb) = (
        a.get(start..a.len() - end).unwrap_or(&[]),
        b.get(start..b.len() - end).unwrap_or(&[]),
    );
    if !ma.is_empty() && !mb.is_empty() && ma.len().saturating_mul(mb.len()) <= MAX_CELLS {
        out.extend(
            lcs(ma, mb, |x, y| same(x, y))
                .into_iter()
                .map(|(i, j)| (i + start, j + start)),
        );
    }
    out.extend((0..end).map(|k| (a.len() - end + k, b.len() - end + k)));
    out
}

/// The longest common subsequence of `a` and `b` under `eq`, as pairs of indexes, by the table of lengths.
fn lcs<T>(a: &[T], b: &[T], eq: impl Fn(&T, &T) -> bool) -> Vec<(usize, usize)> {
    let width = b.len() + 1;
    let mut table = vec![0_u32; (a.len() + 1) * width];
    let at = |i: usize, j: usize| i * width + j;
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            let (Some(x), Some(y)) = (a.get(i), b.get(j)) else {
                continue;
            };
            let value = if eq(x, y) {
                table.get(at(i + 1, j + 1)).copied().unwrap_or(0) + 1
            } else {
                table
                    .get(at(i + 1, j))
                    .copied()
                    .unwrap_or(0)
                    .max(table.get(at(i, j + 1)).copied().unwrap_or(0))
            };
            if let Some(cell) = table.get_mut(at(i, j)) {
                *cell = value;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        let (Some(x), Some(y)) = (a.get(i), b.get(j)) else {
            break;
        };
        if eq(x, y) {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table.get(at(i + 1, j)).copied().unwrap_or(0)
            >= table.get(at(i, j + 1)).copied().unwrap_or(0)
        {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// A line of a hunk before it is written.
enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// The lines of `text`: none for the empty text (so that a part that is only added has no removed lines).
fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// `old` against `new` as one unified hunk over all their lines, read back by the history's `linediff` (with its
/// word marks), and whether lines were left out for the bound.
pub(crate) fn hunk(old: &str, new: &str) -> (HunkView, bool) {
    let (a, b) = (lines(old), lines(new));
    let start = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let end = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(a.len().min(b.len()) - start)
        .take_while(|(x, y)| x == y)
        .count();
    let ma = a.get(start..a.len() - end).unwrap_or(&[]);
    let mb = b.get(start..b.len() - end).unwrap_or(&[]);
    let mut ops: Vec<Line<'_>> = a.iter().take(start).map(|l| Line::Same(l)).collect();
    if ma.len().saturating_mul(mb.len()) <= MAX_CELLS {
        let pairs = lcs(ma, mb, |x, y| x == y);
        let (mut i, mut j) = (0, 0);
        for (pi, pj) in pairs {
            ops.extend(ma.iter().take(pi).skip(i).map(|l| Line::Removed(l)));
            ops.extend(mb.iter().take(pj).skip(j).map(|l| Line::Added(l)));
            if let Some(l) = ma.get(pi) {
                ops.push(Line::Same(l));
            }
            (i, j) = (pi + 1, pj + 1);
        }
        ops.extend(ma.iter().skip(i).map(|l| Line::Removed(l)));
        ops.extend(mb.iter().skip(j).map(|l| Line::Added(l)));
    } else {
        ops.extend(ma.iter().map(|l| Line::Removed(l)));
        ops.extend(mb.iter().map(|l| Line::Added(l)));
    }
    ops.extend(a.iter().skip(a.len() - end).map(|l| Line::Same(l)));
    let truncated = ops.len() > MAX_DIFF_LINES;
    let mut raw = format!("@@ -1,{} +1,{} @@\n", a.len(), b.len());
    for op in ops.iter().take(MAX_DIFF_LINES) {
        let (sign, text) = match op {
            Line::Same(l) => (' ', *l),
            Line::Removed(l) => ('-', *l),
            Line::Added(l) => ('+', *l),
        };
        raw.push(sign);
        raw.push_str(text);
        raw.push('\n');
    }
    (HunkView::of(&raw), truncated)
}

#[cfg(test)]
mod tests;
