//! The merge of one page's text: its frontmatter key by key, then its body block by block (RFC 004 D-5).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

use inklkunn_markup::frontmatter::{self, Entry};
use inklkunn_markup::{Block, blocks};

use super::engine::{Piece, Unit, Verdict, pieces, slice, verdict};
use super::line::line_level;
use super::{Overlap, PageRef, Place};

/// The result of merging one page's three texts: the merged text, or why there is none.
pub(super) type TextMerge = Result<String, Vec<Found>>;

/// An overlap before it knows which page it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Found {
    pub place: Place,
    pub base: Option<String>,
    pub theirs: Option<String>,
    pub ours: Option<String>,
}

impl Found {
    /// The overlap, placed on `page`, with the measurement made.
    pub(super) fn on(self, page: &PageRef) -> Overlap {
        let line_level = line_level(
            &self.place,
            self.base.as_deref(),
            self.theirs.as_deref(),
            self.ours.as_deref(),
        );
        Overlap {
            page: page.clone(),
            place: self.place,
            base: self.base,
            theirs: self.theirs,
            ours: self.ours,
            line_level,
        }
    }
}

/// Merge three texts of one page. `base` is `""` for a page the base does not have.
pub(super) fn merge_text(base: &str, theirs: &str, ours: &str) -> TextMerge {
    let (b, t, o) = (Parts::of(base), Parts::of(theirs), Parts::of(ours));
    let mut found = Vec::new();
    let (mut out, fenced) = merge_front(&b, &t, &o, &mut found);
    let body = merge_body(&b, &t, &o, &mut found);
    // A closing fence on the last line has no line break to keep the body off it.
    if fenced && !body.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&body);
    if found.is_empty() {
        Ok(out)
    } else {
        Err(found)
    }
}

/// Of three versions of a piece of layout (whitespace, a fence line), the one a side changed. Layout is never
/// a conflict; when both sides changed it differently the tip's stays.
fn pick<'a>(base: &str, theirs: &'a str, ours: &'a str) -> &'a str {
    if theirs == ours {
        theirs
    } else if theirs == base {
        ours
    } else {
        theirs
    }
}

/// A page cut into the parts that are merged apart.
struct Parts<'a> {
    /// Whether the text starts with a frontmatter.
    has_front: bool,
    /// The byte-order mark, if any.
    bom: &'a str,
    /// The byte-order mark and the opening fence line (empty when there is no frontmatter).
    open: &'a str,
    /// The text between the fences.
    yaml: &'a str,
    /// The closing fence line.
    close: &'a str,
    /// The body's top-level blocks.
    blocks: Vec<Block<'a>>,
    /// The whitespace before each block.
    gaps: Vec<&'a str>,
    /// The whitespace after the last block (the whole body when it has none).
    tail: &'a str,
}

impl<'a> Parts<'a> {
    fn of(text: &'a str) -> Parts<'a> {
        let bom_len = text.len() - text.strip_prefix('\u{feff}').unwrap_or(text).len();
        let bom = text.get(..bom_len).unwrap_or("");
        let (open, yaml, close, body, has_front) = match frontmatter::locate(text) {
            Some(l) => (
                text.get(l.open.clone()).unwrap_or(""),
                text.get(l.yaml.clone()).unwrap_or(""),
                text.get(l.close.clone()).unwrap_or(""),
                text.get(l.close.end..).unwrap_or(""),
                true,
            ),
            None => ("", "", "", text.get(bom_len..).unwrap_or(""), false),
        };
        let found = blocks(body);
        let mut gaps = Vec::with_capacity(found.len());
        let mut end = 0usize;
        for b in &found {
            gaps.push(body.get(end..b.range.start).unwrap_or(""));
            end = b.range.end;
        }
        Parts {
            has_front,
            bom,
            open,
            yaml,
            close,
            blocks: found,
            gaps,
            tail: body.get(end..).unwrap_or(""),
        }
    }

    fn units(&self) -> Vec<Unit<'a>> {
        self.blocks
            .iter()
            .map(|b| Unit {
                id: b.id,
                text: b.text,
            })
            .collect()
    }

    fn gap(&self, i: usize) -> &'a str {
        self.gaps.get(i).copied().unwrap_or("")
    }

    fn block_text(&self, i: usize) -> &'a str {
        self.blocks.get(i).map_or("", |b| b.text)
    }
}

// ------------------------------------------------------------------------------------------ frontmatter

/// The entries of one side's frontmatter; the whole of it as one entry when `whole`.
fn entries_of<'a>(p: &Parts<'a>, whole: bool) -> Option<Vec<Entry<'a>>> {
    if !p.has_front {
        Some(Vec::new())
    } else if whole {
        Some(vec![Entry {
            key: String::new(),
            text: p.yaml,
        }])
    } else {
        frontmatter::entries(p.yaml)
    }
}

fn keys_of<'e>(entries: &'e [Entry<'_>]) -> Vec<&'e str> {
    entries.iter().map(|e| e.key.as_str()).collect()
}

fn by_key<'e, 'a>(entries: &'e [Entry<'a>]) -> HashMap<&'e str, &'a str> {
    entries.iter().map(|e| (e.key.as_str(), e.text)).collect()
}

/// Merge the frontmatters key by key: a key one side changed is taken, a key both sides changed alike is
/// taken once, a key both changed differently is an overlap. Returns the merged frontmatter, fences included,
/// and whether it has fences (it does not when the page has no frontmatter, and then it is a byte-order mark
/// or nothing).
fn merge_front(
    b: &Parts<'_>,
    t: &Parts<'_>,
    o: &Parts<'_>,
    found: &mut Vec<Found>,
) -> (String, bool) {
    // One side the parser does not read as a mapping makes the frontmatter one unit on every side.
    let cut = |whole: bool| {
        Some((
            entries_of(b, whole)?,
            entries_of(t, whole)?,
            entries_of(o, whole)?,
        ))
    };
    let (eb, et, eo) = cut(false).or_else(|| cut(true)).unwrap_or_default();
    let (mb, mt, mo) = (by_key(&eb), by_key(&et), by_key(&eo));

    let keys: BTreeSet<&str> = eb
        .iter()
        .chain(&et)
        .chain(&eo)
        .map(|e| e.key.as_str())
        .collect();
    let mut kept: HashMap<&str, &str> = HashMap::new();
    for key in keys {
        let (kb, kt, ko) = (
            mb.get(key).copied(),
            mt.get(key).copied(),
            mo.get(key).copied(),
        );
        let alike = |x: Option<&str>, y: Option<&str>| x.map(str::trim) == y.map(str::trim);
        let chosen = if alike(kt, ko) {
            // The same on both sides: the side that changed the layout keeps it.
            Some(if ko == kb { kt } else { ko })
        } else if alike(kt, kb) {
            Some(ko)
        } else if alike(ko, kb) {
            Some(kt)
        } else {
            None
        };
        match chosen {
            Some(Some(text)) => {
                kept.insert(key, text);
            }
            Some(None) => {}
            None => found.push(Found {
                place: Place::Frontmatter {
                    key: key.to_owned(),
                },
                base: kb.map(str::to_owned),
                theirs: kt.map(str::to_owned),
                ours: ko.map(str::to_owned),
            }),
        }
    }

    // The frontmatter's shape (its fences, and the order of its keys) is the proposal's when the tip did not
    // touch it, and the tip's otherwise; a key only the other side has goes after the key before it there.
    let same_shape = |x: &Parts<'_>, kx: &[Entry<'_>], y: &Parts<'_>, ky: &[Entry<'_>]| {
        x.has_front == y.has_front
            && x.open == y.open
            && x.close == y.close
            && keys_of(kx) == keys_of(ky)
    };
    let (skeleton, primary, secondary) = if same_shape(t, &et, b, &eb) {
        (o, &eo, &et)
    } else {
        (t, &et, &eo)
    };
    let primary_keys = keys_of(primary);
    let in_primary: HashSet<&str> = primary_keys.iter().copied().collect();
    let mut after: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
    let mut anchor: Option<&str> = None;
    for key in keys_of(secondary) {
        if in_primary.contains(key) {
            anchor = Some(key);
        } else if kept.contains_key(key) {
            after.entry(anchor).or_default().push(key);
        }
    }
    // A key only the other side has goes after the keys the primary side added there, before the next key
    // both sides know: two sides that each add a key at the end of the list leave the tip's first.
    let mut yaml = String::new();
    let in_secondary: HashSet<&str> = keys_of(secondary).into_iter().collect();
    let mut pending: Vec<&str> = after.get(&None).cloned().unwrap_or_default();
    let write = |keys: &[&str], yaml: &mut String| {
        for key in keys {
            if let Some(text) = kept.get(key) {
                yaml.push_str(text);
            }
        }
    };
    for key in primary_keys {
        if in_secondary.contains(key) {
            write(&pending, &mut yaml);
            pending.clear();
        }
        write(&[key], &mut yaml);
        if in_secondary.contains(key) {
            pending = after.get(&Some(key)).cloned().unwrap_or_default();
        }
    }
    write(&pending, &mut yaml);

    if skeleton.has_front || !kept.is_empty() {
        let donor = [skeleton, o, t, b].into_iter().find(|p| p.has_front);
        let (open, close) = donor.map_or(("---\n", "---\n"), |p| (p.open, p.close));
        (format!("{open}{yaml}{close}"), true)
    } else {
        (pick(b.bom, t.bom, o.bom).to_owned(), false)
    }
}

// ------------------------------------------------------------------------------------------------ body

/// The body being written: blocks with the whitespace that came with them.
#[derive(Default)]
struct Written {
    text: String,
    any: bool,
}

impl Written {
    fn block(&mut self, gap: &str, text: &str) {
        // Two blocks always have a line break between them, whatever side the gap came from.
        if self.any && !gap.contains('\n') {
            self.text.push_str("\n\n");
        } else {
            self.text.push_str(gap);
        }
        self.text.push_str(text);
        self.any = true;
    }
}

/// The blocks of `range`, with the gaps before them, from the side `from`.
fn take(out: &mut Written, from: &Parts<'_>, range: Range<usize>) {
    for i in range {
        out.block(from.gap(i), from.block_text(i));
    }
}

/// The texts of a run of blocks as one text, `None` for no blocks.
fn joined(from: &Parts<'_>, range: &Range<usize>) -> Option<String> {
    if range.is_empty() {
        return None;
    }
    let texts: Vec<&str> = range.clone().map(|i| from.block_text(i)).collect();
    Some(texts.join("\n\n"))
}

fn merge_body(b: &Parts<'_>, t: &Parts<'_>, o: &Parts<'_>, found: &mut Vec<Found>) -> String {
    let (ub, ut, uo) = (b.units(), t.units(), o.units());
    let mut out = Written::default();
    for piece in pieces(&ub, &ut, &uo, true) {
        let (rb, rt, ro) = match piece {
            Piece::Sync { base, theirs, ours } => {
                (base..base + 1, theirs..theirs + 1, ours..ours + 1)
            }
            Piece::Chunk { base, theirs, ours } => (base, theirs, ours),
        };
        let (sb, st, so) = (slice(&ub, &rb), slice(&ut, &rt), slice(&uo, &ro));
        match verdict(&sb, &st, &so) {
            Verdict::Same | Verdict::Ours => {
                if rb.len() == 1 && rt.len() == 1 && ro.len() == 1 {
                    // One block on each side: its layout is the one a side changed.
                    let gap = pick(b.gap(rb.start), t.gap(rt.start), o.gap(ro.start));
                    out.block(gap, o.block_text(ro.start));
                } else {
                    take(&mut out, o, ro);
                }
            }
            Verdict::Theirs => {
                if rb.len() == 1 && rt.len() == 1 && ro.len() == 1 {
                    let gap = pick(b.gap(rb.start), t.gap(rt.start), o.gap(ro.start));
                    out.block(gap, t.block_text(rt.start));
                } else {
                    take(&mut out, t, rt);
                }
            }
            Verdict::Overlap => {
                let id = sb
                    .iter()
                    .chain(&st)
                    .chain(&so)
                    .find_map(|u| u.id)
                    .map(str::to_owned);
                let first = |r: &Range<usize>| (!r.is_empty()).then_some(r.start + 1);
                found.push(Found {
                    place: Place::Block {
                        id,
                        base: first(&rb),
                        theirs: first(&rt),
                        ours: first(&ro),
                    },
                    base: joined(b, &rb),
                    theirs: joined(t, &rt),
                    ours: joined(o, &ro),
                });
            }
        }
    }
    out.text.push_str(pick(b.tail, t.tail, o.tail));
    out.text
}
