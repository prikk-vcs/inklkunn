//! A page's diff as lines, and the word-level refinement of them (RFC 003 D-4; internal design RD-2; handoff
//! 003 T5).
//!
//! **prikk supplies the line diff; inklkunn only reads it.** [`Hunk::parse`] takes one unified hunk exactly as
//! prikk printed it and keeps every line's text whole. [`Hunk::refine`] is the **word-level refinement, which
//! is inklkunn's own and is labelled so** wherever it is shown: inside a removed line that is followed by an
//! added line it marks the words that differ. A mark never adds, drops or changes a character: the segments of
//! a line always join to the line prikk gave, and [`Hunk::to_unified`] writes the hunk back from the refined
//! model byte for byte equal to the input (the property the tests hold: marks cannot change the line diff).
//!
//! A hunk that is not in the shape of a unified hunk is not guessed at: [`HunkView::Raw`] keeps its text, to
//! be shown as text.
//!
//! **Bounded work.** The refinement of one pair of lines is a longest-common-subsequence over their words; a
//! pair with more than [`MAX_WORDS`] words on a side, or with almost nothing in common, is left unmarked (the
//! whole line is the change, which is true).

/// The most words a line may have on either side to be refined.
pub const MAX_WORDS: usize = 600;

/// Two lines need at least this share of their words in common (in tenths) to be marked word by word; below
/// it the lines are different lines and marking every word would say nothing.
const MIN_COMMON_TENTHS: usize = 4;

/// What a line of a hunk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Unchanged text around a change.
    Context,
    /// A line only the older side has.
    Removed,
    /// A line only the newer side has.
    Added,
    /// prikk's `\ No newline at end of file`.
    Marker,
}

impl LineKind {
    fn prefix(self) -> char {
        match self {
            LineKind::Context => ' ',
            LineKind::Removed => '-',
            LineKind::Added => '+',
            LineKind::Marker => '\\',
        }
    }
}

/// A run of one line's text, marked or not. The segments of a line join to its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The text. Untrusted: a surface escapes it.
    pub text: String,
    /// The words inklkunn's refinement found different (only ever in a removed or added line).
    pub marked: bool,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// What it is.
    pub kind: LineKind,
    /// Its text, in segments; one unmarked segment unless the refinement marked words.
    pub segments: Vec<Segment>,
}

impl DiffLine {
    /// The line's text, as prikk gave it (the segments joined).
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// One unified hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@` line as prikk printed it.
    pub header: String,
    /// The first line of the older side the hunk covers, when the header says.
    pub old_start: Option<u64>,
    /// The first line of the newer side the hunk covers, when the header says.
    pub new_start: Option<u64>,
    /// The lines.
    pub lines: Vec<DiffLine>,
    /// The hunk's text ended with a line feed (it does in every hunk prikk prints; kept so the round trip is
    /// exact whatever the input).
    ends_with_newline: bool,
}

/// A hunk to show: as lines, or, when it is not in the shape of a unified hunk, as the text it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkView {
    /// Parsed into lines (and refined).
    Lines(Hunk),
    /// Not a unified hunk: shown as text, never interpreted.
    Raw(String),
}

impl HunkView {
    /// Read `raw` and refine it. Never fails: what cannot be read as lines is kept as text.
    pub fn of(raw: &str) -> HunkView {
        match Hunk::parse(raw) {
            Some(mut hunk) => {
                hunk.refine();
                HunkView::Lines(hunk)
            }
            None => HunkView::Raw(raw.to_owned()),
        }
    }

    /// The hunk as unified text again, exactly as it was read.
    pub fn to_unified(&self) -> String {
        match self {
            HunkView::Lines(h) => h.to_unified(),
            HunkView::Raw(t) => t.clone(),
        }
    }
}

fn header_starts(header: &str) -> (Option<u64>, Option<u64>) {
    // `@@ -6,4 +6,4 @@ section`
    let mut old = None;
    let mut new = None;
    for part in header.split_whitespace().skip(1).take(2) {
        let start = |p: &str| p.split(',').next().and_then(|n| n.parse::<u64>().ok());
        if let Some(rest) = part.strip_prefix('-') {
            old = start(rest);
        } else if let Some(rest) = part.strip_prefix('+') {
            new = start(rest);
        }
    }
    (old, new)
}

impl Hunk {
    /// Read one unified hunk. `None` when it does not begin with `@@` or holds a line that is not a context,
    /// removed, added or `\` line (an empty line is not a context line: unified diff writes a space).
    pub fn parse(raw: &str) -> Option<Hunk> {
        let ends_with_newline = raw.ends_with('\n');
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let mut rows = body.split('\n');
        let header = rows.next()?;
        if !header.starts_with("@@ ") || !header.contains(" @@") {
            return None;
        }
        let mut lines = Vec::new();
        for row in rows {
            let mut chars = row.chars();
            let kind = match chars.next()? {
                ' ' => LineKind::Context,
                '-' => LineKind::Removed,
                '+' => LineKind::Added,
                '\\' => LineKind::Marker,
                _ => return None,
            };
            lines.push(DiffLine {
                kind,
                segments: vec![Segment {
                    text: chars.as_str().to_owned(),
                    marked: false,
                }],
            });
        }
        let (old_start, new_start) = header_starts(header);
        Some(Hunk {
            header: header.to_owned(),
            old_start,
            new_start,
            lines,
            ends_with_newline,
        })
    }

    /// The hunk as unified text: the header, then each line with its prefix and the segments joined.
    pub fn to_unified(&self) -> String {
        let mut out = String::with_capacity(self.header.len() + 1);
        out.push_str(&self.header);
        for line in &self.lines {
            out.push('\n');
            out.push(line.kind.prefix());
            for s in &line.segments {
                out.push_str(&s.text);
            }
        }
        if self.ends_with_newline {
            out.push('\n');
        }
        out
    }

    /// How many lines the hunk adds.
    pub fn added(&self) -> u64 {
        self.count(LineKind::Added)
    }

    /// How many lines the hunk removes.
    pub fn removed(&self) -> u64 {
        self.count(LineKind::Removed)
    }

    fn count(&self, kind: LineKind) -> u64 {
        self.lines.iter().filter(|l| l.kind == kind).count() as u64
    }

    /// Mark the words that differ between each removed line and the added line that takes its place: the
    /// i-th removed line of a run with the i-th added line of the run that follows it. The text of every line
    /// is left whole.
    pub fn refine(&mut self) {
        let mut i = 0;
        while let Some(line) = self.lines.get(i) {
            if line.kind != LineKind::Removed {
                i += 1;
                continue;
            }
            let removed_from = i;
            while self
                .lines
                .get(i)
                .is_some_and(|l| l.kind == LineKind::Removed)
            {
                i += 1;
            }
            let added_from = i;
            while self.lines.get(i).is_some_and(|l| l.kind == LineKind::Added) {
                i += 1;
            }
            let pairs = (added_from - removed_from).min(i - added_from);
            for k in 0..pairs {
                self.refine_pair(removed_from + k, added_from + k);
            }
        }
    }

    fn refine_pair(&mut self, removed: usize, added: usize) {
        let (Some(old), Some(new)) = (
            self.lines.get(removed).map(DiffLine::text),
            self.lines.get(added).map(DiffLine::text),
        ) else {
            return;
        };
        let Some((old_segments, new_segments)) = mark_words(&old, &new) else {
            return;
        };
        if let Some(line) = self.lines.get_mut(removed) {
            line.segments = old_segments;
        }
        if let Some(line) = self.lines.get_mut(added) {
            line.segments = new_segments;
        }
    }
}

// ------------------------------------------------------------------------------------ words

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Single,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xff66..=0xff9f | 0xac00..=0xd7af | 0x20000..=0x2fa1f)
}

fn class_of(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if is_cjk(c) {
        Class::Single
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Single
    }
}

/// The words of `text` as byte ranges that tile it: a run of letters and digits, a run of spaces, or one
/// other character (every character of a script without spaces is a word of its own).
fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut previous: Option<Class> = None;
    for (at, c) in text.char_indices() {
        let class = class_of(c);
        let joins = matches!(class, Class::Word | Class::Space) && previous == Some(class);
        if !joins && at > start {
            if let Some(word) = text.get(start..at) {
                out.push(word);
            }
            start = at;
        }
        previous = Some(class);
    }
    if let Some(word) = text.get(start..)
        && !word.is_empty()
    {
        out.push(word);
    }
    out
}

/// A table of `(rows + 1) × (columns + 1)` counts, read and written without indexing.
struct Table {
    width: usize,
    cells: Vec<u16>,
}

impl Table {
    fn new(rows: usize, columns: usize) -> Table {
        Table {
            width: columns + 1,
            cells: vec![0; (rows + 1) * (columns + 1)],
        }
    }

    fn at(&self, i: usize, j: usize) -> u16 {
        self.cells.get(i * self.width + j).copied().unwrap_or(0)
    }

    fn set(&mut self, i: usize, j: usize, value: u16) {
        if let Some(cell) = self.cells.get_mut(i * self.width + j) {
            *cell = value;
        }
    }
}

fn is_blank(word: &str) -> bool {
    word.chars().all(char::is_whitespace)
}

/// The segments of `old` and of `new` with the words that are not in common marked, or `None` when the lines
/// are too long to compare or too different for marks to mean anything.
fn mark_words(old: &str, new: &str) -> Option<(Vec<Segment>, Vec<Segment>)> {
    let a = words(old);
    let b = words(new);
    if a.len() > MAX_WORDS || b.len() > MAX_WORDS {
        return None;
    }
    let mut table = Table::new(a.len(), b.len());
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            let value = if x == y {
                table.at(i, j) + 1
            } else {
                table.at(i, j + 1).max(table.at(i + 1, j))
            };
            table.set(i + 1, j + 1, value);
        }
    }
    // Walk back from the end to find which words are in common.
    let mut common_a = vec![false; a.len()];
    let mut common_b = vec![false; b.len()];
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 && j > 0 {
        let (x, y) = (a.get(i - 1)?, b.get(j - 1)?);
        if x == y {
            *common_a.get_mut(i - 1)? = true;
            *common_b.get_mut(j - 1)? = true;
            i -= 1;
            j -= 1;
        } else if table.at(i - 1, j) >= table.at(i, j - 1) {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    let spoken = |words: &[&str], common: &[bool]| {
        words
            .iter()
            .zip(common)
            .filter(|(w, c)| **c && !is_blank(w))
            .count()
    };
    let total = |words: &[&str]| words.iter().filter(|w| !is_blank(w)).count();
    let shared = spoken(&a, &common_a);
    let longest = total(&a).max(total(&b));
    // Nothing to mark, or nothing in common: leave the lines whole.
    if longest == 0 || shared * 10 < longest * MIN_COMMON_TENTHS {
        return None;
    }
    Some((segments(&a, &common_a), segments(&b, &common_b)))
}

/// The words as segments: neighbours with the same mark are one segment.
fn segments(words: &[&str], common: &[bool]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for (word, is_common) in words.iter().zip(common) {
        let marked = !*is_common;
        match out.last_mut() {
            Some(last) if last.marked == marked => last.text.push_str(word),
            _ => out.push(Segment {
                text: (*word).to_owned(),
                marked,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests;
