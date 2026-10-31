//! The replay's records: one JSON Lines file per article, holding text-derived metrics and never text.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::read_text;

/// What happened to a block in one revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// An aligned pair whose text differs: the id survived.
    Changed,
    /// An unmatched old block paired with an unmatched new one between the same anchors: the id did not survive.
    Replaced,
    /// A new block with no old counterpart.
    Added,
    /// An old block with no new counterpart.
    Removed,
}

impl Kind {
    /// Every kind, in report order.
    pub const ALL: [Kind; 4] = [Kind::Changed, Kind::Replaced, Kind::Added, Kind::Removed];

    /// The name used in the files.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Changed => "changed",
            Kind::Replaced => "replaced",
            Kind::Added => "added",
            Kind::Removed => "removed",
        }
    }
}

/// One event: an edited block of one revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// What happened.
    pub kind: Kind,
    /// `en` or `ja`.
    pub lang: String,
    /// The article's title.
    pub article: String,
    /// The revision id.
    pub rev: u64,
    /// The previous replayed revision's id.
    pub parent_rev: u64,
    /// The block number in the previous revision (none for an added block).
    pub prev_block: Option<usize>,
    /// The block number in this revision (none for a removed block).
    pub new_block: Option<usize>,
    /// The minted id the old block carried.
    pub id_before: Option<String>,
    /// The id the new block carries.
    pub id_after: Option<String>,
    /// The old block's id is the new block's.
    pub id_survived: bool,
    /// Non-whitespace characters in the changed wording (old and new side together).
    pub chars_changed: u64,
    /// Words in the changed wording (old and new side together).
    pub words_changed: u64,
    /// The product's refinement could not mark the pair, so it was counted whole.
    pub unrefined: bool,
    /// The old and the new text differ (true for every event; kept so the M-2b check reads a fact).
    pub text_differs: bool,
    /// Other events in the same revision.
    pub others_changed_in_revision: u64,
    /// The revision's whole body is byte-identical to an earlier revision's that is not its parent.
    pub is_revert: bool,
}

/// One revision's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionRec {
    /// Always `revision`.
    pub kind: String,
    /// `en` or `ja`.
    pub lang: String,
    /// The article's title.
    pub article: String,
    /// The revision id.
    pub rev: u64,
    /// The previous replayed revision's id (none for the first).
    pub parent_rev: Option<u64>,
    /// Its position in the replay, from 0.
    pub position: usize,
    /// Blocks in the revision.
    pub blocks: usize,
    /// Ids minted in this revision (all of them in the first).
    pub minted: usize,
    /// Events of kind `changed`.
    pub changed: u64,
    /// Events of kind `replaced`.
    pub replaced: u64,
    /// Events of kind `added`.
    pub added: u64,
    /// Events of kind `removed`.
    pub removed: u64,
    /// See [`Event::is_revert`].
    pub is_revert: bool,
}

/// A line of an events file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// A revision's record.
    Revision(RevisionRec),
    /// An event.
    Event(Event),
}

impl Record {
    /// The JSON line (no newline).
    pub fn to_line(&self) -> Result<String, String> {
        match self {
            Record::Revision(r) => serde_json::to_string(r),
            Record::Event(e) => serde_json::to_string(e),
        }
        .map_err(|e| e.to_string())
    }
}

/// Read every `*.jsonl` file of a directory in name order.
pub fn read_dir(dir: &Path) -> Result<Vec<Record>, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        for (n, line) in read_text(&f)?.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let at = || format!("{} line {}", f.display(), n + 1);
            let v: serde_json::Value =
                serde_json::from_str(line).map_err(|e| format!("{}: {e}", at()))?;
            let rec = if v.get("kind").and_then(|k| k.as_str()) == Some("revision") {
                Record::Revision(serde_json::from_value(v).map_err(|e| format!("{}: {e}", at()))?)
            } else {
                Record::Event(serde_json::from_value(v).map_err(|e| format!("{}: {e}", at()))?)
            };
            out.push(rec);
        }
    }
    Ok(out)
}
