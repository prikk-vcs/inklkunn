//! Following each paragraph of a page back through the page's earlier versions (RFC 005 part 2 D-8).
//!
//! **Pure and streaming.** A [`Tracer`] begins with the page as it is and is fed the page's earlier versions one at
//! a time, newest first, each with the number of the change that made the newer version out of it; it keeps the
//! two texts of the step it is on and a small record per paragraph, never the versions already passed. The matching
//! is the merge's own ([`crate::merge::alignment`]: the same `^id`, then the same unique text, then a bounded
//! longest-common-subsequence), and the segmentation is the product's ([`inklkunn_markup::blocks`]).
//!
//! **What a step can know.** A paragraph whose block carries an `^id` that is the one block with that id on both
//! sides of the step is followed *by its anchor*. A paragraph followed in any other way, even for a single step,
//! is followed *by matching the text*, and the chain says so: a text that was reworded between two paragraphs that
//! were reworded together can be matched to the wrong one, and no anchor is there to say it was not.

use std::collections::HashMap;

use inklkunn_markup::Block;

use crate::merge::{alignment, blocks_of_page};

/// The most characters of a paragraph's beginning that name it.
pub const EXCERPT_CHARS: usize = 120;

/// How a paragraph was followed back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// Through its `^id`, at every step: one block with that id on both sides of each.
    Anchor,
    /// By matching the text at one step or more: no anchor carried it through them all.
    Text,
}

/// What became known of a paragraph's past. The numbers are of the steps fed to the [`Tracer`] (the first fed is 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chain {
    /// The newest step at which the paragraph's wording differs from the one before, or at which it was written:
    /// `None` when no step fed changed it.
    pub last_changed: Option<usize>,
    /// The step at which the paragraph was written (it had no counterpart before), `None` when the steps fed
    /// ended while it still had one.
    pub first_written: Option<usize>,
    /// How the paragraph was followed.
    pub basis: Basis,
}

/// A paragraph of the page as it is: where it is, its `^id` and how it begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// Its place among the page's blocks, from 1.
    pub ordinal: usize,
    /// Its `^id` without the caret, when it has one.
    pub id: Option<String>,
    /// How it begins: its text on one line, the `^id` left out, at most [`EXCERPT_CHARS`] characters.
    pub excerpt: String,
}

#[derive(Debug, Clone)]
struct Track {
    /// The block of the newer text of the step this paragraph is at, `None` once it was written.
    at: Option<usize>,
    last_changed: Option<usize>,
    first_written: Option<usize>,
    anchored: bool,
}

/// Follows the paragraphs of one page back through its versions.
#[derive(Debug)]
pub struct Tracer {
    newer: String,
    paragraphs: Vec<Paragraph>,
    tracks: Vec<Track>,
    /// The blocks of the page the tracer began with that were left out because there were more than it follows.
    left_out: usize,
}

/// A paragraph's text on one line, the trailing `^id` left out, cut to [`EXCERPT_CHARS`] characters.
fn excerpt(block: &Block<'_>) -> String {
    let text = match block.id {
        Some(id) => block
            .text
            .trim_end()
            .strip_suffix(id)
            .and_then(|t| t.trim_end().strip_suffix('^'))
            .unwrap_or(block.text),
        None => block.text,
    };
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= EXCERPT_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(EXCERPT_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// The ids of a page's blocks, each with whether more than one block carries it: such an id names no paragraph.
fn repeated_ids<'a>(blocks: &[Block<'a>]) -> HashMap<&'a str, bool> {
    let mut seen: HashMap<&str, bool> = HashMap::new();
    for id in blocks.iter().filter_map(|b| b.id) {
        seen.entry(id)
            .and_modify(|repeated| *repeated = true)
            .or_insert(false);
    }
    seen
}

impl Tracer {
    /// Begin with the page as it is, following at most `cap` of its blocks (the first).
    pub fn begin(current: &str, cap: usize) -> Tracer {
        let blocks = blocks_of_page(current);
        let left_out = blocks.len().saturating_sub(cap);
        let paragraphs: Vec<Paragraph> = blocks
            .iter()
            .take(cap)
            .enumerate()
            .map(|(at, b)| Paragraph {
                ordinal: at + 1,
                id: b.id.map(str::to_owned),
                excerpt: excerpt(b),
            })
            .collect();
        // Only a paragraph that has an `^id` can be followed by it.
        let tracks = paragraphs
            .iter()
            .enumerate()
            .map(|(at, p)| Track {
                at: Some(at),
                last_changed: None,
                first_written: None,
                anchored: p.id.is_some(),
            })
            .collect();
        Tracer {
            newer: current.to_owned(),
            paragraphs,
            tracks,
            left_out,
        }
    }

    /// The paragraphs followed, in the page's order.
    pub fn paragraphs(&self) -> &[Paragraph] {
        &self.paragraphs
    }

    /// How many of the page's blocks are not followed because there were more than the cap.
    pub fn left_out(&self) -> usize {
        self.left_out
    }

    /// Does any paragraph still have a counterpart in the version last fed (so that an older version can tell
    /// more)?
    pub fn is_following(&self) -> bool {
        self.tracks.iter().any(|t| t.at.is_some())
    }

    /// Feed the version before step `step`'s change: `older` is the page's text before it, `None` when the change
    /// wrote the page (the page was not there before it).
    pub fn feed(&mut self, step: usize, older: Option<&str>) {
        let Some(older_text) = older else {
            for track in &mut self.tracks {
                if track.at.take().is_some() {
                    track.first_written = Some(step);
                    track.last_changed.get_or_insert(step);
                }
            }
            return;
        };
        let map = alignment(&self.newer, older_text);
        {
            let newer = blocks_of_page(&self.newer);
            let before = blocks_of_page(older_text);
            let (newer_ids, before_ids) = (repeated_ids(&newer), repeated_ids(&before));
            for track in &mut self.tracks {
                let Some(at) = track.at else { continue };
                let counterpart = map.get(at).copied().flatten();
                let now = newer.get(at);
                let then = counterpart.and_then(|c| before.get(c));
                let (Some(now), Some(then)) = (now, then) else {
                    track.at = None;
                    track.first_written = Some(step);
                    track.last_changed.get_or_insert(step);
                    continue;
                };
                if now.text != then.text {
                    track.last_changed.get_or_insert(step);
                }
                let by_anchor = match (now.id, then.id) {
                    (Some(a), Some(b)) => {
                        a == b
                            && newer_ids.get(a) == Some(&false)
                            && before_ids.get(b) == Some(&false)
                    }
                    _ => false,
                };
                track.anchored &= by_anchor;
                track.at = counterpart;
            }
        }
        older_text.clone_into(&mut self.newer);
    }

    /// What became known of each paragraph, in the page's order.
    pub fn chains(&self) -> Vec<Chain> {
        self.tracks
            .iter()
            .map(|t| Chain {
                last_changed: t.last_changed,
                first_written: t.first_written,
                basis: if t.anchored {
                    Basis::Anchor
                } else {
                    Basis::Text
                },
            })
            .collect()
    }
}
