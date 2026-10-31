//! Text helpers of the replay: a page's body and blocks as the product has them, and how much of a block's
//! wording changed as the product's own diff marks count it.

use inklkunn_core::read::Hunk;
use inklkunn_core::read::{LineKind, Segment};
use inklkunn_markup::{blocks, frontmatter};

/// The part of a page the block segmentation reads: the text after the frontmatter, or after the byte-order
/// mark when there is none. The same rule as the merge's alignment (`inklkunn_core::align_blocks`), so block
/// numbers agree with its answer.
pub fn body_of(text: &str) -> &str {
    match frontmatter::locate(text) {
        Some(l) => text.get(l.close.end..).unwrap_or(""),
        None => text.strip_prefix('\u{feff}').unwrap_or(text),
    }
}

/// The texts of a page's top-level blocks, in order (the product's `blocks`).
pub fn block_texts(page: &str) -> Vec<&str> {
    blocks(body_of(page)).into_iter().map(|b| b.text).collect()
}

/// A script class for counting words where there are no spaces between them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Letters,
    Han,
    Hiragana,
    Katakana,
    Hangul,
}

fn script_of(c: char) -> Option<Script> {
    match c as u32 {
        0x3005
        | 0x3007
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xf900..=0xfaff
        | 0x20000..=0x2fa1f => Some(Script::Han),
        0x3040..=0x309f => Some(Script::Hiragana),
        0x30a0..=0x30ff | 0xff66..=0xff9f => Some(Script::Katakana),
        0xac00..=0xd7af => Some(Script::Hangul),
        _ if c.is_alphanumeric() || c == '_' => Some(Script::Letters),
        _ => None,
    }
}

/// Words in `text`: maximal runs of one script, where whitespace, punctuation and a change of script end a
/// word. (Japanese has no spaces between words; a run of kanji, of hiragana or of katakana is one word here.)
pub fn count_words(text: &str) -> u64 {
    let mut words = 0;
    let mut current: Option<Script> = None;
    for c in text.chars() {
        let s = script_of(c);
        if s.is_some() && s != current {
            words += 1;
        }
        current = s;
    }
    words
}

/// Characters that are not whitespace.
pub fn count_chars(text: &str) -> u64 {
    text.chars().filter(|c| !c.is_whitespace()).count() as u64
}

/// How much one block's wording changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    /// Non-whitespace characters in the marked segments (old and new side together).
    pub chars: u64,
    /// Words in the marked segments (old and new side together).
    pub words: u64,
    /// The product's refinement could not mark a pair of lines that differ (too different, or too long), so the
    /// whole pair is counted.
    pub unrefined: bool,
}

/// The wording change from `old` to `new`: a unified hunk of the old lines as `-` and the new lines as `+` is
/// read and refined by the product (`Hunk::parse`, `Hunk::refine`), and the marked segments are counted. A pair
/// of lines that differs but received no mark is counted whole and flags `unrefined`; a line with no partner is
/// counted whole without the flag (it is an addition or a removal of a line).
pub fn wording_change(old: &str, new: &str) -> Change {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut raw = format!("@@ -1,{} +1,{} @@\n", old_lines.len(), new_lines.len());
    for l in &old_lines {
        raw.push('-');
        raw.push_str(l);
        raw.push('\n');
    }
    for l in &new_lines {
        raw.push('+');
        raw.push_str(l);
        raw.push('\n');
    }
    let Some(mut hunk) = Hunk::parse(&raw) else {
        return whole(old, new, true);
    };
    hunk.refine();
    let removed: Vec<&[Segment]> = lines_of(&hunk, LineKind::Removed);
    let added: Vec<&[Segment]> = lines_of(&hunk, LineKind::Added);
    let mut change = Change::default();
    for k in 0..removed.len().max(added.len()) {
        match (removed.get(k), added.get(k)) {
            (Some(o), Some(n)) => {
                let (ot, nt) = (join(o), join(n));
                if ot == nt {
                    continue;
                }
                let marked: Vec<&str> = o
                    .iter()
                    .chain(n.iter())
                    .filter(|s| s.marked)
                    .map(|s| s.text.as_str())
                    .collect();
                if marked.is_empty() {
                    change.unrefined = true;
                    change.chars += count_chars(&ot) + count_chars(&nt);
                    change.words += count_words(&ot) + count_words(&nt);
                } else {
                    for m in marked {
                        change.chars += count_chars(m);
                        change.words += count_words(m);
                    }
                }
            }
            (Some(only), None) | (None, Some(only)) => {
                let t = join(only);
                change.chars += count_chars(&t);
                change.words += count_words(&t);
            }
            (None, None) => {}
        }
    }
    change
}

/// The whole of both texts counted (an added or removed block: `old` or `new` empty).
pub fn whole(old: &str, new: &str, unrefined: bool) -> Change {
    Change {
        chars: count_chars(old) + count_chars(new),
        words: count_words(old) + count_words(new),
        unrefined,
    }
}

fn lines_of(hunk: &Hunk, kind: LineKind) -> Vec<&[Segment]> {
    hunk.lines
        .iter()
        .filter(|l| l.kind == kind)
        .map(|l| l.segments.as_slice())
        .collect()
}

fn join(segments: &[Segment]) -> String {
    segments.iter().map(|s| s.text.as_str()).collect()
}

/// A block as a labeller reads it: each converter snapshot (`<!-- wiki:snapshot … src="X" -->…<!-- /wiki:snapshot -->`,
/// what the built-in converter leaves for a template or a table it did not expand) is replaced by the source it
/// stands for, `X`, so a template such as `{{convert|9|cm|in}}` is read as written instead of as a long comment. The
/// replay and every measure use the converted text as it is; only the packets are made readable this way.
pub fn readable(text: &str) -> String {
    const OPEN: &str = "<!-- wiki:snapshot";
    const CLOSE: &str = "<!-- /wiki:snapshot -->";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(rest.get(..at).unwrap_or(""));
        let tail = rest.get(at..).unwrap_or("");
        let Some(end) = tail.find(CLOSE) else {
            // Not closed: leave the rest as it is.
            out.push_str(tail);
            return out;
        };
        let snapshot = tail.get(..end).unwrap_or("");
        match snapshot.find(" src=\"").and_then(|i| {
            let from = i + " src=\"".len();
            snapshot
                .get(from..)
                .and_then(|r| r.rfind("\" -->").map(|j| r.get(..j).unwrap_or("")))
        }) {
            Some(src) => out.push_str(src),
            None => out.push_str(snapshot),
        }
        rest = tail.get(end + CLOSE.len()..).unwrap_or("");
    }
    out.push_str(rest);
    out
}
