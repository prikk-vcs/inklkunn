//! The three-way engine over sequences of units (RFC 004 D-5): align each side with the base, cut the three
//! sequences at the units all three agree are the same, and decide each stretch between those points.
//!
//! The same engine merges a page's blocks (units with `^id`s, matched by id first) and, in the measurement
//! hook, a block's lines (units with no id, matched by content and order only).
//!
//! **Alignment** of the base with one side, in three stages, each limited to what the earlier ones left:
//! 1. units with the same id, when the id is unique on both sides;
//! 2. units with the same text, when the text is unique among what is left on both sides;
//! 3. between the points found so far, the longest common subsequence of what remains, after the common
//!    start and end of each stretch (and, for blocks, a stretch of one unit on each side is one unit that
//!    changed).
//!
//! Stages 1 and 2 may only keep matches that stay in order (a unit that moved is a removal and an addition,
//! which a merge reports rather than guesses at). Stage 3 is bounded: a stretch whose table would pass
//! [`MAX_CELLS`] is left unaligned, which can only turn a clean merge into an overlap, never the reverse.

use std::collections::HashMap;
use std::ops::Range;

/// One unit of a sequence: a block or a line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Unit<'a> {
    /// The unit's identifier, when it has one.
    pub id: Option<&'a str>,
    /// What two units are compared by.
    pub text: &'a str,
}

/// The most cells of a longest-common-subsequence table the engine builds for one stretch.
pub(super) const MAX_CELLS: usize = 250_000;

/// For each unit of `base`, the index of the unit of `other` it is the same unit as.
pub(super) fn align(
    base: &[Unit<'_>],
    other: &[Unit<'_>],
    pair_changed: bool,
) -> Vec<Option<usize>> {
    let mut map: Vec<Option<usize>> = vec![None; base.len()];
    let mut taken: Vec<bool> = vec![false; other.len()];

    // 1. The same id, unique on both sides.
    let mut ids = Occurrences::default();
    for (i, u) in base.iter().enumerate() {
        if let Some(id) = u.id {
            ids.base(id, i);
        }
    }
    for (i, u) in other.iter().enumerate() {
        if let Some(id) = u.id {
            ids.other(id, i);
        }
    }
    let mut pairs: Vec<(usize, usize)> = ids.unique_pairs();
    pairs.sort_unstable();
    accept(&in_order(&pairs), &mut map, &mut taken);

    // 2. The same text, unique among what is left, and between the points already found.
    let (lo, hi) = bounds(&map, other.len());
    let mut texts = Occurrences::default();
    for (i, u) in base.iter().enumerate() {
        if map.get(i).is_some_and(Option::is_none) {
            texts.base(u.text, i);
        }
    }
    for (i, u) in other.iter().enumerate() {
        if !taken.get(i).copied().unwrap_or(true) {
            texts.other(u.text, i);
        }
    }
    let mut pairs: Vec<(usize, usize)> = texts
        .unique_pairs()
        .into_iter()
        .filter(|(b, o)| {
            let low = lo.get(*b).copied().unwrap_or(0);
            let high = hi.get(*b).copied().unwrap_or(other.len());
            *o >= low && *o < high
        })
        .collect();
    pairs.sort_unstable();
    accept(&in_order(&pairs), &mut map, &mut taken);

    // 3. What lies between the points found so far.
    let mut anchors: Vec<(usize, usize)> = map
        .iter()
        .enumerate()
        .filter_map(|(b, o)| o.map(|o| (b, o)))
        .collect();
    anchors.push((base.len(), other.len()));
    let (mut next_b, mut next_o) = (0usize, 0usize);
    for (b, o) in anchors {
        if b >= next_b && o >= next_o {
            fill(base, other, next_b..b, next_o..o, pair_changed, &mut map);
        }
        next_b = b + 1;
        next_o = o + 1;
    }
    map
}

/// How often a key occurs on one side, and where it first does.
#[derive(Default, Clone, Copy)]
struct Seen {
    count: usize,
    first: usize,
}

impl Seen {
    fn add(&mut self, at: usize) {
        if self.count == 0 {
            self.first = at;
        }
        self.count += 1;
    }
}

/// Where each key occurs in the base and in the other side.
#[derive(Default)]
struct Occurrences<'a> {
    seen: HashMap<&'a str, [Seen; 2]>,
}

impl<'a> Occurrences<'a> {
    fn base(&mut self, key: &'a str, at: usize) {
        self.seen.entry(key).or_default()[0].add(at);
    }

    fn other(&mut self, key: &'a str, at: usize) {
        self.seen.entry(key).or_default()[1].add(at);
    }

    /// `(base index, other index)` of every key that occurs once on each side.
    fn unique_pairs(&self) -> Vec<(usize, usize)> {
        self.seen
            .values()
            .filter(|[b, o]| b.count == 1 && o.count == 1)
            .map(|[b, o]| (b.first, o.first))
            .collect()
    }
}

/// The longest run of `pairs` (sorted by their first element) whose second elements increase.
fn in_order(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // Patience sorting: `tails[k]` is the pair ending the best run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; pairs.len()];
    for (i, (_, o)) in pairs.iter().enumerate() {
        let k = tails.partition_point(|t| pairs.get(*t).is_some_and(|p| p.1 < *o));
        if k > 0
            && let Some(slot) = before.get_mut(i)
        {
            *slot = tails.get(k - 1).copied();
        }
        if k == tails.len() {
            tails.push(i);
        } else if let Some(slot) = tails.get_mut(k) {
            *slot = i;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        if let Some(p) = pairs.get(i) {
            run.push(*p);
        }
        at = before.get(i).copied().flatten();
    }
    run.reverse();
    run
}

fn accept(pairs: &[(usize, usize)], map: &mut [Option<usize>], taken: &mut [bool]) {
    for (b, o) in pairs {
        if let Some(slot) = map.get_mut(*b) {
            *slot = Some(*o);
        }
        if let Some(slot) = taken.get_mut(*o) {
            *slot = true;
        }
    }
}

/// For each base index, the lowest and the highest-plus-one index of `other` a new match there may take: it
/// must lie after the match before it and before the match after it.
fn bounds(map: &[Option<usize>], other_len: usize) -> (Vec<usize>, Vec<usize>) {
    let mut lo = Vec::with_capacity(map.len());
    let mut at = 0usize;
    for m in map {
        lo.push(at);
        if let Some(o) = m {
            at = o + 1;
        }
    }
    let mut hi = vec![other_len; map.len()];
    let mut at = other_len;
    for (i, m) in map.iter().enumerate().rev() {
        if let Some(slot) = hi.get_mut(i) {
            *slot = at;
        }
        if let Some(o) = m {
            at = *o;
        }
    }
    (lo, hi)
}

/// Align the units of one stretch: its common start and end, then the longest common subsequence of the rest
/// when the table is not too large.
fn fill(
    base: &[Unit<'_>],
    other: &[Unit<'_>],
    mut b: Range<usize>,
    mut o: Range<usize>,
    pair_changed: bool,
    map: &mut [Option<usize>],
) {
    let same = |bi: usize, oi: usize| match (base.get(bi), other.get(oi)) {
        (Some(x), Some(y)) => x.text == y.text,
        _ => false,
    };
    while b.start < b.end && o.start < o.end && same(b.start, o.start) {
        if let Some(slot) = map.get_mut(b.start) {
            *slot = Some(o.start);
        }
        b.start += 1;
        o.start += 1;
    }
    while b.start < b.end && o.start < o.end && same(b.end - 1, o.end - 1) {
        if let Some(slot) = map.get_mut(b.end - 1) {
            *slot = Some(o.end - 1);
        }
        b.end -= 1;
        o.end -= 1;
    }
    let (n, m) = (b.len(), o.len());
    if n == 0 || m == 0 {
        return;
    }
    if n.checked_mul(m).is_none_or(|cells| cells > MAX_CELLS) {
        return;
    }
    // table[i][j]: the length of the longest common subsequence of b[i..] and o[j..].
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let down = table.get((i + 1) * width + j).copied().unwrap_or(0);
            let right = table.get(i * width + j + 1).copied().unwrap_or(0);
            let value = if same(b.start + i, o.start + j) {
                table.get((i + 1) * width + j + 1).copied().unwrap_or(0) + 1
            } else {
                down.max(right)
            };
            if let Some(slot) = table.get_mut(i * width + j) {
                *slot = value;
            }
        }
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut matches: Vec<(usize, usize)> = Vec::new();
    while i < n && j < m {
        if same(b.start + i, o.start + j) {
            matches.push((i, j));
            i += 1;
            j += 1;
        } else if table.get((i + 1) * width + j) >= table.get(i * width + j + 1) {
            i += 1;
        } else {
            j += 1;
        }
    }
    for (i, j) in &matches {
        if let Some(slot) = map.get_mut(b.start + i) {
            *slot = Some(o.start + j);
        }
    }
    if !pair_changed {
        return;
    }
    // One unit on each side between two matches: that unit changed.
    matches.push((n, m));
    let (mut pi, mut pj) = (0usize, 0usize);
    for (i, j) in matches {
        if i == pi + 1
            && j == pj + 1
            && let Some(slot) = map.get_mut(b.start + pi)
        {
            *slot = Some(o.start + pj);
        }
        pi = i + 1;
        pj = j + 1;
    }
}

/// A stretch of the three sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Piece {
    /// A unit all three sides have: the base's index, the tip's, the proposal's.
    Sync {
        base: usize,
        theirs: usize,
        ours: usize,
    },
    /// The units between two such points (any of the three ranges may be empty).
    Chunk {
        base: Range<usize>,
        theirs: Range<usize>,
        ours: Range<usize>,
    },
}

/// Cut the three sequences at the units that all three have.
pub(super) fn pieces(
    base: &[Unit<'_>],
    theirs: &[Unit<'_>],
    ours: &[Unit<'_>],
    pair_changed: bool,
) -> Vec<Piece> {
    let to_theirs = align(base, theirs, pair_changed);
    let to_ours = align(base, ours, pair_changed);
    let mut out = Vec::new();
    let (mut b0, mut t0, mut o0) = (0usize, 0usize, 0usize);
    for (b, (t, o)) in to_theirs.iter().zip(&to_ours).enumerate() {
        let (Some(t), Some(o)) = (t, o) else {
            continue;
        };
        if b > b0 || *t > t0 || *o > o0 {
            out.push(Piece::Chunk {
                base: b0..b,
                theirs: t0..*t,
                ours: o0..*o,
            });
        }
        out.push(Piece::Sync {
            base: b,
            theirs: *t,
            ours: *o,
        });
        (b0, t0, o0) = (b + 1, t + 1, o + 1);
    }
    if base.len() > b0 || theirs.len() > t0 || ours.len() > o0 {
        out.push(Piece::Chunk {
            base: b0..base.len(),
            theirs: t0..theirs.len(),
            ours: o0..ours.len(),
        });
    }
    out
}

/// What to do with a stretch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Both sides have the same text there (changed alike, or not at all): take it once.
    Same,
    /// The tip left it as it was: take the proposal's.
    Ours,
    /// The proposal left it as it was: take the tip's.
    Theirs,
    /// Both changed it, differently: an overlap.
    Overlap,
}

fn same_texts(a: &[Unit<'_>], b: &[Unit<'_>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.text == y.text)
}

/// Decide a stretch by what its three texts are.
pub(super) fn verdict(base: &[Unit<'_>], theirs: &[Unit<'_>], ours: &[Unit<'_>]) -> Verdict {
    if same_texts(theirs, ours) {
        Verdict::Same
    } else if same_texts(theirs, base) {
        Verdict::Ours
    } else if same_texts(ours, base) {
        Verdict::Theirs
    } else {
        Verdict::Overlap
    }
}

/// The units of `range`.
pub(super) fn slice<'a>(units: &[Unit<'a>], range: &Range<usize>) -> Vec<Unit<'a>> {
    units.get(range.clone()).unwrap_or(&[]).to_vec()
}
