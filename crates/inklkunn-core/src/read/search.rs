//! Search over titles and aliases (RFC 003 D-8; requirements RD-9, I18N-4; internal design IQ-2).
//!
//! The index is SQLite FTS5 with the **trigram** tokenizer, held by the derived store and built **per tip** from
//! the title index (no prikk call: the titles and aliases are already there), so a tip change is a new index
//! and the old one is dropped with the rest of the past. The tokenizer matches any part of a text, which is how
//! a title in a language with no word breaks is found by a substring; the *folding* is ours and is applied to
//! the stored text and to the query alike ([`fold`]): width variants and diacritics are the same letter, case
//! is not a difference, and a voiced kana mark is never a diacritic (が is not か).
//!
//! What is shown for a hit is the title or alias that matched with the matching part marked ([`highlight`]):
//! a list of segments of **the author's text, unescaped**, which the surface escapes (and sanitizes) when it
//! writes them; nothing here makes HTML.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use inklkunn_store::{IndexEntry, SearchDoc, SearchKind, WikiRecord};
use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

use super::{ReadError, ReadService, Tip};
use crate::read::index::{Lookup, TitleIndex};

/// The most texts asked of the index for one query; a query with more hits than this says it was cut.
const CANDIDATES: usize = 400;
/// The most characters of a query that are searched for.
const QUERY_CHARS: usize = 200;
/// The most pages a search returns.
const RESULTS: usize = 50;

/// The text a search compares: NFKC (width variants, compatibility forms), diacritics removed except the
/// kana voicing marks, recomposed, lower-cased, whitespace runs collapsed to one space and trimmed.
pub fn fold(text: &str) -> String {
    let stripped: String = text
        .nfkc()
        .nfd()
        .filter(|c| !is_combining_mark(*c) || matches!(c, '\u{3099}' | '\u{309A}'))
        .nfc()
        .collect();
    let mut out = String::with_capacity(stripped.len());
    let mut pending_space = false;
    for c in stripped.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// One run of a text, marked if it is what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The author's text, as written. A surface escapes it.
    pub text: String,
    /// Is it the part that matched?
    pub matched: bool,
}

/// `text` cut into the part that matched the already-folded `folded_query` and the parts around it. The
/// segments always add up to the text; when the match cannot be located letter by letter (a folding that
/// joins letters, such as a half-width kana and its voicing mark) the text is one unmarked segment: less
/// said, nothing wrong.
pub fn highlight(text: &str, folded_query: &str) -> Vec<Segment> {
    let whole = || {
        vec![Segment {
            text: text.to_owned(),
            matched: false,
        }]
    };
    if folded_query.is_empty() {
        return whole();
    }
    // Each letter folded by itself, remembering where its folded form sits in the folded text.
    let mut folded = String::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for c in text.chars() {
        let start = folded.len();
        if c.is_whitespace() {
            folded.push(' ');
        } else {
            folded.push_str(&fold(&c.to_string()));
        }
        spans.push((start, folded.len()));
    }
    let Some(found) = folded.find(folded_query) else {
        return whole();
    };
    let end = found + folded_query.len();
    let first = spans.iter().position(|&(s, e)| e > found && s < end);
    let last = spans.iter().rposition(|&(s, e)| e > found && s < end);
    let (Some(first), Some(last)) = (first, last) else {
        return whole();
    };
    let chars: Vec<char> = text.chars().collect();
    let part = |from: usize, to: usize| -> String {
        chars
            .iter()
            .skip(from)
            .take(to.saturating_sub(from))
            .collect()
    };
    [
        (part(0, first), false),
        (part(first, last + 1), true),
        (part(last + 1, chars.len()), false),
    ]
    .into_iter()
    .filter(|(t, _)| !t.is_empty())
    .map(|(text, matched)| Segment { text, matched })
    .collect()
}

/// A page the search found, by the text that matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// The page.
    pub entry: IndexEntry,
    /// Did its title match, or one of its aliases?
    pub kind: SearchKind,
    /// The text that matched, cut into the matching part and the rest.
    pub segments: Vec<Segment>,
}

/// What a search came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutcome {
    /// The tip searched.
    pub tip: Tip,
    /// The query as searched (trimmed, cut to a length).
    pub query: String,
    /// The pages, best first: exact title, exact alias, title begins with, alias begins with, title contains,
    /// alias contains; shorter first within a class.
    pub hits: Vec<SearchHit>,
    /// Does the query name a page exactly (a title, an alias or a redirect)? Then there is nothing to create.
    pub names_a_page: bool,
    /// More matched than are listed.
    pub truncated: bool,
}

impl ReadService {
    /// Search the view's tip (the default view when `None`) for `query`. The first search at a new tip builds
    /// its index from the title index, in memory and once; a search index that cannot be stored or read is
    /// answered from the title index itself, never as a failure.
    pub fn search(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        query: &str,
    ) -> Result<SearchOutcome, ReadError> {
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        let query: String = query.trim().chars().take(QUERY_CHARS).collect();
        let folded = fold(&query);
        if folded.is_empty() {
            return Ok(SearchOutcome {
                tip,
                query,
                hits: Vec::new(),
                names_a_page: false,
                truncated: false,
            });
        }
        let found = self.find_texts(wiki, &tip, &index, &folded);
        let truncated = found.len() >= CANDIDATES;
        // The best text of each page.
        let mut best: HashMap<String, (u8, usize, inklkunn_store::SearchHit)> = HashMap::new();
        for hit in found {
            let text = fold(&hit.text);
            let class = rank(hit.kind, &text, &folded);
            let candidate = (class, text.chars().count());
            let keep = best
                .get(&hit.path)
                .is_none_or(|(c, l, _)| candidate < (*c, *l));
            if keep {
                best.insert(hit.path.clone(), (candidate.0, candidate.1, hit));
            }
        }
        let mut ranked: Vec<(u8, usize, inklkunn_store::SearchHit)> = best.into_values().collect();
        ranked.sort_by(|a, b| {
            (a.0, a.1, &a.2.text, &a.2.path).cmp(&(b.0, b.1, &b.2.text, &b.2.path))
        });
        let hits = ranked
            .into_iter()
            .filter_map(|(_, _, hit)| {
                let entry = index.entry(&hit.path)?.clone();
                Some(SearchHit {
                    segments: highlight(&hit.text, &folded),
                    kind: hit.kind,
                    entry,
                })
            })
            .take(RESULTS)
            .collect();
        let names_a_page = !matches!(index.lookup(&query), Lookup::Missing);
        Ok(SearchOutcome {
            tip,
            query,
            hits,
            names_a_page,
            truncated,
        })
    }

    /// The texts of the tip that contain the folded query: from the store's index (built first if the tip
    /// has none), else by scanning the title index.
    fn find_texts(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        index: &TitleIndex,
        folded: &str,
    ) -> Vec<inklkunn_store::SearchHit> {
        let ask = || {
            self.derived
                .search(&wiki.id, &tip.state_id, folded, CANDIDATES)
        };
        let stored = match ask() {
            Ok(Some(hits)) => Some(hits),
            Ok(None) => {
                let docs = search_docs(index);
                if self
                    .derived
                    .put_search_index(
                        &wiki.id,
                        &tip.state_id,
                        &docs,
                        inklkunn_store::record::now_secs(),
                    )
                    .is_err()
                {
                    self.store_failures.fetch_add(1, Ordering::Relaxed);
                    None
                } else {
                    ask().ok().flatten()
                }
            }
            Err(_) => {
                self.store_failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        };
        stored.unwrap_or_else(|| {
            search_docs(index)
                .into_iter()
                .filter(|d| d.folded.contains(folded))
                .take(CANDIDATES)
                .map(|d| inklkunn_store::SearchHit {
                    path: d.path,
                    kind: d.kind,
                    text: d.text,
                })
                .collect()
        })
    }
}

/// Every title and alias of the index, with the text the search compares.
fn search_docs(index: &TitleIndex) -> Vec<SearchDoc> {
    let mut docs = Vec::new();
    for entry in index.entries() {
        docs.push(SearchDoc {
            path: entry.path.clone(),
            kind: SearchKind::Title,
            text: entry.title.clone(),
            folded: fold(&entry.title),
        });
        for alias in &entry.aliases {
            docs.push(SearchDoc {
                path: entry.path.clone(),
                kind: SearchKind::Alias,
                text: alias.clone(),
                folded: fold(alias),
            });
        }
    }
    docs
}

/// The class of a match: lower is better.
fn rank(kind: SearchKind, folded_text: &str, folded_query: &str) -> u8 {
    let base = if folded_text == folded_query {
        0
    } else if folded_text.starts_with(folded_query) {
        2
    } else {
        4
    };
    base + u8::from(kind == SearchKind::Alias)
}

#[cfg(test)]
mod tests;
