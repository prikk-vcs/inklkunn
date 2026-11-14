//! **No two subjects read alike** (handoff 005 corrections, third round, K24). A claim is named by the beginning of a
//! paragraph it is said in and a source by its name, and neither is unique: two claims may be said in one paragraph,
//! two paragraphs may begin alike and two sources may be called the same. A proposal that changes the one must never
//! read as one that changes the other, and a page cannot know which other subjects the wiki holds, so **every subject
//! a line names carries its identifier** (except a subject already told by it), shortened by one rule: the first
//! [`HEAD`] characters, `…`, and the last characters, [`TAIL`] of them or as many more as it takes for no two
//! identifiers of the same kind that the page names to be shortened alike, the whole identifier when that is what it
//! takes.
//!
//! The page is read twice: once to learn the identifiers every line names ([`Names::note`]), once to print them
//! ([`Names::short`]); [`Names::settle`] stands between.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// How many characters of an identifier's start an abbreviation keeps.
const HEAD: usize = 4;

/// The fewest characters of an identifier's end an abbreviation keeps.
const TAIL: usize = 4;

/// The kinds of subject whose names are told apart.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Claim,
    Source,
}

/// What the lines of a page have named so far.
#[derive(Default)]
pub(super) struct Names {
    seen: RefCell<Seen>,
}

#[derive(Default)]
struct Seen {
    /// Every identifier named, by kind.
    ids: BTreeMap<Kind, BTreeSet<String>>,
    /// The abbreviation of each identifier named ([`Names::settle`]).
    short: BTreeMap<(Kind, String), String>,
}

/// The shortest forms of `ids`, such that no two differ in nothing but their form: [`HEAD`] characters, `…`, and the
/// last characters (at least [`TAIL`]), one more at a time until all differ; an identifier the form would not make
/// shorter is kept whole.
pub(super) fn abbreviations(ids: &BTreeSet<String>) -> BTreeMap<String, String> {
    let longest = ids.iter().map(|id| id.chars().count()).max().unwrap_or(0);
    let mut tail = TAIL;
    loop {
        let short: BTreeMap<String, String> = ids
            .iter()
            .map(|id| (id.clone(), shorten(id, tail)))
            .collect();
        let forms: BTreeSet<&String> = short.values().collect();
        if forms.len() == short.len() || HEAD + tail >= longest {
            return short;
        }
        tail += 1;
    }
}

fn shorten(id: &str, tail: usize) -> String {
    let len = id.chars().count();
    if HEAD + tail >= len {
        return id.to_owned();
    }
    let head: String = id.chars().take(HEAD).collect();
    let end: String = id.chars().skip(len - tail).collect();
    format!("{head}…{end}")
}

impl Names {
    /// A line names the subject `id`.
    pub(super) fn note(&self, kind: Kind, id: &str) {
        self.seen
            .borrow_mut()
            .ids
            .entry(kind)
            .or_default()
            .insert(id.to_owned());
    }

    /// Every line has been seen once: fix the abbreviations.
    pub(super) fn settle(&self) {
        let mut seen = self.seen.borrow_mut();
        let mut short = BTreeMap::new();
        for (kind, ids) in &seen.ids {
            for (id, form) in abbreviations(ids) {
                short.insert((*kind, id), form);
            }
        }
        seen.short = short;
    }

    /// The abbreviation of `id` (`None` before [`Names::settle`], or for an identifier no line named).
    pub(super) fn short(&self, kind: Kind, id: &str) -> Option<String> {
        self.seen
            .borrow()
            .short
            .get(&(kind, id.to_owned()))
            .cloned()
    }
}
