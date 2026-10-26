//! The Layer 2 scanner: every Portable Wiki Markdown construct in a page, with the file line it is on.
//!
//! The rules are the wiki-commons reference scanner's (`tools/pwb/markup.py`, corpus commit in
//! `tests/corpus/CORPUS-SOURCE.md`), restated here so the corpus is the acceptance test:
//!
//! - Constructs are recognized **outside** fenced code blocks, inline code spans, the frontmatter and
//!   HTML comments; the hidden regions are masked, never removed, so offsets and line numbers hold.
//! - **Free links** `[[...]]` and embeds `![[...]]`: the inner text has no `[`, `]` or newline; a
//!   preceding backslash escapes the form. Target first, label second (MKUP-6) unless the caller
//!   declares label-first. `#fragment` is a heading text, `#^id` a block identifier. A prefix is the text
//!   before the first `:` when it has no space and the target does not start with `/` or `.`.
//! - **Headings** are ATX only (MKUP-8), with GitHub-style anchors and `-1`, `-2` for duplicates.
//! - **Block identifiers** are a trailing `^id` token, `[A-Za-z0-9][A-Za-z0-9_-]*`, preceded by a space or
//!   at the start of the line, not on a heading line (MKUP-9).
//! - **Inline tags** `#tag` start after whitespace or `(`, begin with a word character, and are not all
//!   digits (MKUP-14); frontmatter `tags` may be a list or a comma/space-separated string.
//! - **Callouts** `> [!TYPE]±  Title` (MKUP-15); the type is reported upper-case.
//! - **Directives** `::: name key=value` ... `:::` and inline `:name[text]{key=value}` (MKUP-16).
//! - **Snapshot envelopes** `<!-- wiki:snapshot ... -->` ... `<!-- /wiki:snapshot -->` and **props**
//!   `<!-- wiki:props {json} -->` (chapter 12); they are HTML comments and so hidden from everything else,
//!   while their bodies are ordinary content.

use serde::Serialize;

use crate::anchor::Anchors;
use crate::frontmatter;
use crate::text::{byte, from, is_word, line_of, sl};

/// How to read `[[a|b]]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanOptions {
    /// `true` when the page comes from a label-first dialect (`[[Label|Target]]`, MKUP-6).
    pub label_first: bool,
}

/// A free link or embed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    /// The source text, `[[` to `]]` inclusive (with the `!` of an embed).
    pub raw: String,
    /// The target as written, trimmed; `""` for a same-page link.
    pub target: String,
    /// The label, when one is given and non-empty.
    pub label: Option<String>,
    /// The heading text or block identifier after `#`.
    pub fragment: Option<String>,
    /// `heading` or `block`.
    pub fragment_kind: Option<FragmentKind>,
    /// The text before the first `:` of the target, when it looks like a prefix (MKUP-13).
    pub prefix: Option<String>,
    /// `![[...]]`.
    pub embed: bool,
    /// 1-based file line.
    pub line: usize,
    /// Byte offset of `raw` in the scanned text (not serialized).
    #[serde(skip)]
    pub start: usize,
    /// Byte offset just past `raw` (not serialized).
    #[serde(skip)]
    pub end: usize,
}

/// What a link fragment names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FragmentKind {
    /// `[[Page#Heading text]]`.
    Heading,
    /// `[[Page#^block-id]]`.
    Block,
}

/// A heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    /// 1-based file line.
    pub line: usize,
    /// 1–6.
    pub level: u8,
    /// The heading text as written (Markdown kept).
    pub text: String,
    /// The GitHub-style anchor, unique within the page.
    pub anchor: String,
}

/// A block identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockId {
    /// 1-based file line.
    pub line: usize,
    /// The identifier without `^`.
    pub id: String,
}

/// Tags from both locations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Tags {
    /// From the frontmatter `tags` key.
    pub frontmatter: Vec<String>,
    /// Inline `#tag`s, in order, duplicates kept.
    pub inline: Vec<String>,
}

/// A callout opener.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Callout {
    /// 1-based file line.
    pub line: usize,
    /// Upper-cased type.
    #[serde(rename = "type")]
    pub kind: String,
    /// `+` or `-`.
    pub fold: Option<String>,
    /// The title after the marker.
    pub title: Option<String>,
}

/// A directive, container or inline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Directive {
    /// 1-based file line.
    pub line: usize,
    /// The directive name.
    pub name: String,
    /// Inline directives only: the bracketed text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// `key=value` attributes, in source order.
    pub attrs: Attrs,
    /// `true` for `:name[text]{...}`, `false` for `::: name`.
    pub inline: bool,
}

/// Ordered `key=value` attributes.
pub type Attrs = Vec<(String, String)>;

fn attrs_to_json(attrs: &Attrs) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, v) in attrs {
        m.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    serde_json::Value::Object(m)
}

/// A snapshot envelope (chapter 12).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Envelope {
    /// Opened and closed.
    Closed(ClosedEnvelope),
    /// Opened and never closed.
    Unclosed(UnclosedEnvelope),
}

/// A closed snapshot envelope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClosedEnvelope {
    /// 1-based file line of the opener.
    pub line: usize,
    /// `kind` attribute.
    pub kind: Option<String>,
    /// `name` attribute.
    pub name: Option<String>,
    /// `engine` attribute.
    pub engine: Option<String>,
    /// `src` attribute, entity-decoded.
    pub src: Option<String>,
    /// `params` attribute, parsed as JSON when it is JSON.
    pub params: serde_json::Value,
    /// `at` attribute.
    pub at: Option<String>,
    /// `target` attribute.
    pub target: Option<String>,
    /// `id` attribute.
    pub id: Option<String>,
    /// Nesting depth, 0 at the top.
    pub depth: usize,
    /// The text between the markers, surrounding newlines removed.
    pub body: String,
}

/// An envelope that never closes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnclosedEnvelope {
    /// 1-based file line of the opener.
    pub line: usize,
    /// `kind` attribute.
    pub kind: Option<String>,
    /// `name` attribute.
    pub name: Option<String>,
    /// `engine` attribute.
    pub engine: Option<String>,
    /// Always `true`.
    pub unclosed: bool,
}

/// A props envelope.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Props {
    /// 1-based file line.
    pub line: usize,
    /// The JSON object, or `{"_raw": text}` when the text is not JSON.
    pub props: serde_json::Value,
}

/// Everything the scanner found, in the shape of the corpus's `expected.scan.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanReport {
    /// The frontmatter as JSON (dates marked `{"$type": "date", "iso": ...}`), `null` when absent;
    /// `{"_error": ..., "_raw": ...}` when it is outside the plain subset.
    pub frontmatter: Option<serde_json::Value>,
    /// Headings in order.
    pub headings: Vec<Heading>,
    /// Free links and embeds in order.
    pub links: Vec<Link>,
    /// Block identifiers in order.
    pub block_ids: Vec<BlockId>,
    /// Tags.
    pub tags: Tags,
    /// Callouts in order.
    pub callouts: Vec<Callout>,
    /// Directives, by line.
    #[serde(serialize_with = "serialize_directives")]
    pub directives: Vec<Directive>,
    /// Envelopes, by line (inner before outer on the same line).
    pub envelopes: Vec<Envelope>,
    /// Props envelopes in order.
    pub props: Vec<Props>,
}

fn serialize_directives<S: serde::Serializer>(d: &[Directive], s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(d.len()))?;
    for x in d {
        let mut m = serde_json::Map::new();
        m.insert("line".into(), serde_json::Value::from(x.line));
        m.insert("name".into(), serde_json::Value::String(x.name.clone()));
        if let Some(t) = &x.text {
            m.insert("text".into(), serde_json::Value::String(t.clone()));
        }
        m.insert("attrs".into(), attrs_to_json(&x.attrs));
        m.insert("inline".into(), serde_json::Value::Bool(x.inline));
        seq.serialize_element(&serde_json::Value::Object(m))?;
    }
    seq.end()
}

/// Scan a whole page file (frontmatter included) with the default options.
pub fn scan(text: &str) -> ScanReport {
    scan_with(text, ScanOptions::default())
}

/// Scan a whole page file with options.
pub fn scan_with(text: &str, options: ScanOptions) -> ScanReport {
    let (yaml, body, _) = frontmatter::split(text);
    let body_start = text.len() - body.len();
    let fm_json = yaml.map(|y| match frontmatter::parse_yaml(y) {
        Ok(v) => v.to_json(),
        Err(e) => serde_json::json!({"_error": e.to_string(), "_raw": y}),
    });
    let mut report = scan_body(text, body_start, options);
    report.tags.frontmatter = frontmatter_tags(fm_json.as_ref());
    report.frontmatter = fm_json;
    report
}

fn frontmatter_tags(fm: Option<&serde_json::Value>) -> Vec<String> {
    match fm.and_then(|v| v.get("tags")) {
        Some(serde_json::Value::String(s)) => s
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|t| match t {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A text with a byte mask: masked bytes read as spaces.
struct Masked<'a> {
    text: &'a str,
    mask: Vec<bool>,
}

impl<'a> Masked<'a> {
    fn new(text: &'a str) -> Self {
        Masked {
            text,
            mask: vec![false; text.len()],
        }
    }

    fn at(&self, i: usize) -> u8 {
        if self.mask.get(i).copied().unwrap_or(true) {
            b' '
        } else {
            byte(self.text, i)
        }
    }

    fn is_masked(&self, i: usize) -> bool {
        self.mask.get(i).copied().unwrap_or(true)
    }

    fn mask_range(&mut self, start: usize, end: usize) {
        for i in start..end.min(self.mask.len()) {
            if byte(self.text, i) != b'\n'
                && let Some(m) = self.mask.get_mut(i)
            {
                *m = true;
            }
        }
    }

    /// The char starting at byte `i`, or a space when masked or not a boundary.
    fn char_at(&self, i: usize) -> char {
        if self.is_masked(i) {
            return ' ';
        }
        from(self.text, i).chars().next().unwrap_or(' ')
    }

    /// The char ending just before byte `i` (None at the start).
    fn char_before(&self, i: usize) -> Option<char> {
        if i == 0 {
            return None;
        }
        let prev = sl(self.text, 0, i).chars().next_back()?;
        let start = i - prev.len_utf8();
        Some(if self.is_masked(start) { ' ' } else { prev })
    }

    fn starts_with_at(&self, i: usize, pat: &str) -> bool {
        pat.bytes().enumerate().all(|(k, b)| self.at(i + k) == b)
    }

    fn find_from(&self, start: usize, pat: &str) -> Option<usize> {
        (start..self.text.len()).find(|&i| self.starts_with_at(i, pat))
    }
}

/// Byte ranges of the lines of `text` (without the `\n`, with any `\r` kept).
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            out.push((start, i));
            start = i + 1;
        }
    }
    out.push((start, text.len()));
    out
}

/// Mask fenced code blocks and inline code spans.
fn mask_code(m: &mut Masked<'_>) {
    let mut fence: Option<u8> = None;
    for (s, e) in line_ranges(m.text) {
        let line = sl(m.text, s, e);
        let f = fence_marker(line);
        match fence {
            None => {
                if let Some((ch, _)) = f {
                    fence = Some(ch);
                    m.mask_range(s, e);
                } else {
                    mask_inline_code(m, s, e);
                }
            }
            Some(ch) => {
                m.mask_range(s, e);
                if let Some((fch, _)) = f
                    && fch == ch
                    && line.trim().bytes().all(|b| b == ch)
                {
                    fence = None;
                }
            }
        }
    }
}

/// `^\s{0,3}(`{3,}|~{3,})` → the fence character and its run length.
fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < 3 && matches!(b.get(i), Some(b' ') | Some(b'\t')) {
        i += 1;
    }
    let ch = *b.get(i)?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = b
        .get(i..)
        .map_or(0, |rest| rest.iter().take_while(|&&c| c == ch).count());
    (run >= 3).then_some((ch, run))
}

/// Mask `` `code` `` spans on one line: an opening run of n backticks, content that neither starts nor
/// ends with a backtick, and a closing run of exactly n; a run with no closer is retried shorter.
fn mask_inline_code(m: &mut Masked<'_>, s: usize, e: usize) {
    let mut i = s;
    while i < e {
        if m.at(i) != b'`' {
            i += 1;
            continue;
        }
        let run_end = (i..e).find(|&k| m.at(k) != b'`').unwrap_or(e);
        let n = run_end - i;
        // Try the maximal run first, then the shorter suffixes (what a backtracking regex does).
        let mut matched = None;
        for len in (1..=n).rev() {
            let open_start = run_end - len;
            if let Some(close) = find_backtick_run(m, run_end, e, len) {
                matched = Some((open_start, close + len));
                break;
            }
        }
        match matched {
            Some((a, b)) => {
                m.mask_range(a, b);
                i = b;
            }
            None => i = run_end,
        }
    }
}

/// The start of the first run of exactly `len` backticks in `[from, to)` that is preceded by at least one
/// non-backtick byte after `from`.
fn find_backtick_run(m: &Masked<'_>, from: usize, to: usize, len: usize) -> Option<usize> {
    let mut i = from;
    let mut seen_content = false;
    while i < to {
        if m.at(i) == b'`' {
            let run_end = (i..to).find(|&k| m.at(k) != b'`').unwrap_or(to);
            if run_end - i == len && seen_content {
                return Some(i);
            }
            i = run_end;
            seen_content = true;
        } else {
            seen_content = true;
            i += 1;
        }
    }
    None
}

/// `key=value` pairs anywhere in `s`, values unquoted and entity-decoded.
pub fn parse_attrs(s: &str) -> Attrs {
    let mut out = Attrs::new();
    let mut i = 0;
    while i < s.len() {
        let Some(c) = from(s, i).chars().next() else {
            break;
        };
        if !(c.is_ascii_alphabetic() || c == '_') {
            i += c.len_utf8();
            continue;
        }
        // key
        let mut j = i;
        for ch in from(s, i).chars() {
            if is_word(ch) || ch == '-' {
                j += ch.len_utf8();
            } else {
                break;
            }
        }
        let key_end = j;
        let mut k = skip_ws(s, j);
        if byte(s, k) != b'=' {
            i += c.len_utf8();
            continue;
        }
        k = skip_ws(s, k + 1);
        let (value, next) = match byte(s, k) {
            q @ (b'"' | b'\'') => match quoted_value(s, k + 1, q) {
                Some(v) => v,
                None => {
                    i += c.len_utf8();
                    continue;
                }
            },
            _ => {
                let end = from(s, k)
                    .char_indices()
                    .find(|(_, ch)| ch.is_whitespace() || *ch == '"' || *ch == '\'')
                    .map_or(s.len(), |(off, _)| k + off);
                if end == k {
                    i += c.len_utf8();
                    continue;
                }
                (sl(s, k, end).to_owned(), end)
            }
        };
        out.push((sl(s, i, key_end).to_owned(), decode_entities(&value)));
        i = next;
    }
    out
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while matches!(byte(s, i), b' ' | b'\t') && i < s.len() {
        i += 1;
    }
    i
}

/// The content of a quoted value starting after the opening quote; backslash escapes of the quote kept
/// as the quote. Returns the value and the offset after the closing quote.
fn quoted_value(s: &str, start: usize, q: u8) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut i = start;
    while i < s.len() {
        let b = byte(s, i);
        if b == b'\\' && i + 1 < s.len() {
            let next = byte(s, i + 1);
            if next == b'"' || next == b'\'' {
                out.push(next as char);
            } else {
                out.push('\\');
                out.push_str(sl(s, i + 1, i + 2));
            }
            i += 2;
            continue;
        }
        if b == q {
            return Some((out, i + 1));
        }
        let c = from(s, i).chars().next()?;
        out.push(c);
        i += c.len_utf8();
    }
    None
}

fn decode_entities(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

/// Parse the inside of `[[...]]`.
pub fn parse_link_inner(
    inner: &str,
    options: ScanOptions,
    embed: bool,
    raw: &str,
    line: usize,
) -> Link {
    let (target_part, label) = match inner.split_once('|') {
        Some((a, b)) => {
            let (t, l) = if options.label_first { (b, a) } else { (a, b) };
            let l = l.trim();
            (t, (!l.is_empty()).then(|| l.to_owned()))
        }
        None => (inner, None),
    };
    let mut target = target_part.trim().to_owned();
    let mut fragment = None;
    let mut fragment_kind = None;
    if let Some((t, frag)) = target.split_once('#') {
        let frag = frag.trim();
        let t = t.trim().to_owned();
        if let Some(id) = frag.strip_prefix('^') {
            if !id.is_empty() {
                fragment = Some(id.to_owned());
                fragment_kind = Some(FragmentKind::Block);
            }
        } else if !frag.is_empty() {
            fragment = Some(frag.to_owned());
            fragment_kind = Some(FragmentKind::Heading);
        }
        target = t;
    }
    let prefix = if !target.starts_with('/') && !target.starts_with('.') {
        target
            .split_once(':')
            .map(|(p, _)| p)
            .filter(|p| !p.is_empty() && !p.contains(' '))
            .map(str::to_owned)
    } else {
        None
    };
    Link {
        raw: raw.to_owned(),
        target,
        label,
        fragment,
        fragment_kind,
        prefix,
        embed,
        line,
        start: 0,
        end: 0,
    }
}

/// Find free links in a masked text.
fn find_links(m: &Masked<'_>, options: ScanOptions) -> Vec<Link> {
    let mut links = Vec::new();
    let text = m.text;
    let mut i = 0;
    while i + 1 < text.len() {
        let embed = m.at(i) == b'!' && m.at(i + 1) == b'[' && m.at(i + 2) == b'[';
        let plain = m.at(i) == b'[' && m.at(i + 1) == b'[';
        if !(embed || plain) || m.char_before(i) == Some('\\') {
            i += 1;
            continue;
        }
        let inner_start = if embed { i + 3 } else { i + 2 };
        let mut j = inner_start;
        let mut ok = false;
        while j < text.len() {
            match m.at(j) {
                b'\n' | b'[' => break,
                b']' => {
                    ok = m.at(j + 1) == b']' && j > inner_start;
                    break;
                }
                _ => j += 1,
            }
        }
        if !ok {
            i += 1;
            continue;
        }
        let end = j + 2;
        let raw = sl(text, i, end);
        let inner = sl(text, inner_start, j);
        let mut link = parse_link_inner(inner, options, embed, raw, line_of(text, i));
        link.start = i;
        link.end = end;
        links.push(link);
        i = end;
    }
    links
}

/// `^(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$` → (level, text).
fn heading_parts(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = from(line, hashes);
    let rest = after.trim_start_matches([' ', '\t']);
    if rest.len() == after.len() || rest.is_empty() {
        return None;
    }
    let mut t = rest.trim_end_matches([' ', '\t']);
    t = t.trim_end_matches('#');
    t = t.trim_end_matches([' ', '\t']);
    if t.is_empty() {
        // `(.+?)` keeps at least one character: the first one.
        let first = rest.chars().next()?;
        t = sl(rest, 0, first.len_utf8());
    }
    let level = u8::try_from(hashes).ok()?;
    Some((level, t))
}

/// The masked heading text with `#` positions of code spans restored from the raw line is what the
/// reference does; here the raw line is re-parsed, and the masked line only decides *whether* it is one.
fn heading_text<'a>(masked_line: &str, raw_line: &'a str) -> Option<(u8, &'a str)> {
    let (level, _) = heading_parts(masked_line)?;
    match heading_parts(raw_line) {
        Some((l, t)) if l == level => Some((level, t)),
        _ => None,
    }
}

/// `(?:^|(?<=[ \t]))\^([A-Za-z0-9][A-Za-z0-9_-]*)[ \t]*$`.
fn block_id(line: &str) -> Option<&str> {
    let trimmed = line.trim_end_matches([' ', '\t']);
    let caret = trimmed.rfind('^')?;
    if caret > 0 && !matches!(byte(trimmed, caret - 1), b' ' | b'\t') {
        return None;
    }
    let id = from(trimmed, caret + 1);
    let mut chars = id.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    Some(id)
}

/// `^[ \t]{0,3}>[ \t]*\[!([A-Za-z][\w-]*)\]([+-]?)[ \t]*(.*)$`.
fn callout_parts(line: &str) -> Option<(String, Option<String>, Option<String>)> {
    let mut i = 0;
    while i < 3 && matches!(byte(line, i), b' ' | b'\t') {
        i += 1;
    }
    if byte(line, i) != b'>' {
        return None;
    }
    i = skip_ws(line, i + 1);
    if !from(line, i).starts_with("[!") {
        return None;
    }
    i += 2;
    let name_start = i;
    let first = from(line, i).chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    for c in from(line, i).chars() {
        if is_word(c) || c == '-' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let kind = sl(line, name_start, i).to_uppercase();
    if byte(line, i) != b']' {
        return None;
    }
    i += 1;
    let fold = match byte(line, i) {
        b'+' => Some("+".to_owned()),
        b'-' => Some("-".to_owned()),
        _ => None,
    };
    if fold.is_some() {
        i += 1;
    }
    let title = from(line, i).trim();
    Some((kind, fold, (!title.is_empty()).then(|| title.to_owned())))
}

/// `^:::[ \t]*([A-Za-z][\w-]*)[ \t]*(.*)$` → (name, rest).
fn directive_open(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(":::")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    let first = rest.chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = 0;
    for c in rest.chars() {
        if is_word(c) || c == '-' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    Some((
        sl(rest, 0, end),
        from(rest, end).trim_start_matches([' ', '\t']),
    ))
}

fn directive_close(line: &str) -> bool {
    line.strip_prefix(":::")
        .is_some_and(|r| r.trim_matches([' ', '\t']).is_empty())
}

/// `(?<![:\w]):([A-Za-z][\w-]*)\[([^\]]*)\]\{([^}]*)\}`.
fn find_inline_directives(m: &Masked<'_>) -> Vec<Directive> {
    let mut out = Vec::new();
    let text = m.text;
    let mut i = 0;
    while i < text.len() {
        if m.at(i) != b':' {
            i += 1;
            continue;
        }
        if let Some(p) = m.char_before(i)
            && (p == ':' || is_word(p))
        {
            i += 1;
            continue;
        }
        let name_start = i + 1;
        let first = m.char_at(name_start);
        if !first.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = name_start;
        while j < text.len() {
            let c = m.char_at(j);
            if is_word(c) || c == '-' {
                j += c.len_utf8();
            } else {
                break;
            }
        }
        if m.at(j) != b'[' {
            i += 1;
            continue;
        }
        let Some(close) = m.find_from(j + 1, "]") else {
            i += 1;
            continue;
        };
        if m.at(close + 1) != b'{' {
            i += 1;
            continue;
        }
        let Some(brace_end) = m.find_from(close + 2, "}") else {
            i += 1;
            continue;
        };
        out.push(Directive {
            line: line_of(text, i),
            name: sl(text, name_start, j).to_owned(),
            text: Some(sl(text, j + 1, close).to_owned()),
            attrs: parse_attrs(sl(text, close + 2, brace_end)),
            inline: true,
        });
        i = brace_end + 1;
    }
    out
}

/// Inline tags: `#` after whitespace, `(` or the start, then a word character and tag characters.
fn find_tags(m: &Masked<'_>) -> Vec<String> {
    const STOP: &str = "#[]()<>{},.!?;:\"'`|\\";
    let mut out = Vec::new();
    let text = m.text;
    let mut i = 0;
    while i < text.len() {
        if m.at(i) != b'#' {
            i += 1;
            continue;
        }
        let prev = m.char_before(i);
        let prev_ok = match prev {
            None => true,
            Some(c) => matches!(c, ' ' | '\t' | '\n' | '('),
        };
        if !prev_ok {
            i += 1;
            continue;
        }
        let first = m.char_at(i + 1);
        if !is_word(first) {
            i += 1;
            continue;
        }
        let mut j = i + 1 + first.len_utf8();
        while j < text.len() {
            let c = m.char_at(j);
            if c.is_whitespace() || STOP.contains(c) {
                break;
            }
            j += c.len_utf8();
        }
        let tag = sl(text, i + 1, j).trim_end_matches('/');
        if !tag.is_empty() && !tag.chars().all(|c| c.is_numeric()) && !tag.starts_with('^') {
            out.push(tag.to_owned());
        }
        i = j.max(i + 1);
    }
    out
}

/// Mask `[text](url "title")` and `![alt](url)` spans and bare `http(s)://` URLs before tag scanning.
fn mask_markdown_links_and_urls(m: &mut Masked<'_>) {
    let text = m.text;
    let mut spans = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if m.at(i) == b'[' {
            let mut j = i + 1;
            while j < text.len() && m.at(j) != b']' {
                j += 1;
            }
            if m.at(j) == b']' && m.at(j + 1) == b'(' {
                let mut k = j + 2;
                while k < text.len() && m.at(k) != b')' && !m.at(k).is_ascii_whitespace() {
                    k += 1;
                }
                if k > j + 2 {
                    let mut end = k;
                    if m.at(end).is_ascii_whitespace() {
                        let t = skip_ws(text, end);
                        if m.at(t) == b'"'
                            && let Some(q) = m.find_from(t + 1, "\"")
                        {
                            end = q + 1;
                        }
                    }
                    if m.at(end) == b')' {
                        let start = if i > 0 && m.at(i - 1) == b'!' {
                            i - 1
                        } else {
                            i
                        };
                        spans.push((start, end + 1));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if (m.starts_with_at(i, "http://") || m.starts_with_at(i, "https://"))
            && !m.char_before(i).is_some_and(is_word)
        {
            let mut j = i;
            while j < text.len() && !m.char_at(j).is_whitespace() {
                j += m.char_at(j).len_utf8();
            }
            spans.push((i, j));
            i = j;
            continue;
        }
        i += 1;
    }
    for (a, b) in spans {
        m.mask_range(a, b);
    }
}

/// Snapshot envelopes and props envelopes, and the ranges of all HTML comments.
fn find_envelopes(m: &Masked<'_>) -> (Vec<Envelope>, Vec<Props>, Vec<(usize, usize)>) {
    let text = m.text;
    let mut comments = Vec::new();
    let mut opens: Vec<(usize, usize, Attrs)> = Vec::new();
    let mut closes: Vec<(usize, usize)> = Vec::new();
    let mut props = Vec::new();
    let mut i = 0;
    while let Some(s) = m.find_from(i, "<!--") {
        let Some(e) = m.find_from(s + 4, "-->") else {
            break;
        };
        let end = e + 3;
        comments.push((s, end));
        let inner = sl(text, s + 4, e);
        let body = inner.trim_start();
        if let Some(rest) = body.strip_prefix("wiki:snapshot")
            && !rest.chars().next().is_some_and(is_word)
        {
            opens.push((s, end, parse_attrs(rest)));
        } else if body
            .strip_prefix("/wiki:snapshot")
            .is_some_and(|r| r.trim().is_empty())
        {
            closes.push((s, end));
        } else if let Some(rest) = body.strip_prefix("wiki:props")
            && rest.starts_with([' ', '\t'])
        {
            let json_text = rest.trim();
            if json_text.starts_with('{') && json_text.ends_with('}') {
                let data = serde_json::from_str::<serde_json::Value>(json_text)
                    .unwrap_or_else(|_| serde_json::json!({"_raw": json_text}));
                props.push(Props {
                    line: line_of(text, s),
                    props: data,
                });
            }
        }
        i = end;
    }
    // Pair openers and closers in document order with a stack; an envelope is recorded when it closes.
    let mut events: Vec<(usize, usize, Option<Attrs>)> = opens
        .into_iter()
        .map(|(s, e, a)| (s, e, Some(a)))
        .chain(closes.into_iter().map(|(s, e)| (s, e, None)))
        .collect();
    events.sort_by_key(|(s, _, _)| *s);
    let mut stack: Vec<(usize, usize, Attrs)> = Vec::new();
    let mut envelopes: Vec<Envelope> = Vec::new();
    for (s, e, attrs) in events {
        match attrs {
            Some(a) => stack.push((s, e, a)),
            None => {
                if let Some((os, oe, oa)) = stack.pop() {
                    let get = |k: &str| oa.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
                    let params = match get("params") {
                        Some(p) if !p.is_empty() => serde_json::from_str::<serde_json::Value>(&p)
                            .unwrap_or(serde_json::Value::String(p)),
                        Some(p) => serde_json::Value::String(p),
                        None => serde_json::Value::Null,
                    };
                    envelopes.push(Envelope::Closed(ClosedEnvelope {
                        line: line_of(text, os),
                        kind: get("kind"),
                        name: get("name"),
                        engine: get("engine"),
                        src: get("src"),
                        params,
                        at: get("at"),
                        target: get("target"),
                        id: get("id"),
                        depth: stack.len(),
                        body: sl(text, oe, s).trim_matches('\n').to_owned(),
                    }));
                }
            }
        }
    }
    for (os, _, oa) in stack {
        let get = |k: &str| oa.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        envelopes.push(Envelope::Unclosed(UnclosedEnvelope {
            line: line_of(text, os),
            kind: get("kind"),
            name: get("name"),
            engine: get("engine"),
            unclosed: true,
        }));
    }
    envelopes.sort_by_key(|e| match e {
        Envelope::Closed(c) => c.line,
        Envelope::Unclosed(u) => u.line,
    });
    (envelopes, props, comments)
}

/// Scan a page whose frontmatter occupies `text[..body_start]`; the frontmatter is masked and the body is
/// scanned with file line numbers. The frontmatter fields of the report are left empty.
pub fn scan_body(text: &str, body_start: usize, options: ScanOptions) -> ScanReport {
    let mut m = Masked::new(text);
    mask_code(&mut m);
    m.mask_range(0, body_start);

    let (envelopes, props, comments) = find_envelopes(&m);
    for (a, b) in comments {
        m.mask_range(a, b);
    }

    let links = find_links(&m, options);

    let mut headings = Vec::new();
    let mut block_ids = Vec::new();
    let mut callouts = Vec::new();
    let mut directives = Vec::new();
    let mut anchors = Anchors::new();
    let mut depth = 0usize;
    for (s, e) in line_ranges(text) {
        let line_no = line_of(text, s);
        let raw_line = sl(text, s, e).strip_suffix('\r').unwrap_or(sl(text, s, e));
        let masked_line = masked_line_str(&m, s, s + raw_line.len());
        if let Some((level, text_)) = heading_text(&masked_line, raw_line) {
            let anchor = anchors.next(text_);
            headings.push(Heading {
                line: line_no,
                level,
                text: text_.to_owned(),
                anchor,
            });
        } else if !masked_line.trim().is_empty()
            && let Some(id) = block_id(&masked_line)
        {
            block_ids.push(BlockId {
                line: line_no,
                id: id.to_owned(),
            });
        }
        if let Some((kind, fold, _)) = callout_parts(&masked_line) {
            let title = callout_parts(raw_line).and_then(|(_, _, t)| t);
            callouts.push(Callout {
                line: line_no,
                kind,
                fold,
                title,
            });
        }
        if directive_close(&masked_line) && depth > 0 {
            depth -= 1;
        } else if let Some((name, rest)) = directive_open(&masked_line) {
            // Attributes are read from the raw line so quoted values keep their text.
            let raw_rest = directive_open(raw_line).map_or(rest, |(_, r)| r);
            directives.push(Directive {
                line: line_no,
                name: name.to_owned(),
                text: None,
                attrs: parse_attrs(raw_rest),
                inline: false,
            });
            depth += 1;
        }
    }
    directives.extend(find_inline_directives(&m));
    directives.sort_by_key(|d| d.line);

    // Tags: hide links, Markdown links and URLs first.
    let mut tag_mask = Masked {
        text,
        mask: m.mask.clone(),
    };
    for l in &links {
        tag_mask.mask_range(l.start, l.end);
    }
    mask_markdown_links_and_urls(&mut tag_mask);
    let inline_tags = find_tags(&tag_mask);

    ScanReport {
        frontmatter: None,
        headings,
        links,
        block_ids,
        tags: Tags {
            frontmatter: Vec::new(),
            inline: inline_tags,
        },
        callouts,
        directives,
        envelopes,
        props,
    }
}

/// The masked view of `text[s..e]` as a string: masked characters become spaces, others are kept.
fn masked_line_str(m: &Masked<'_>, s: usize, e: usize) -> String {
    let mut out = String::with_capacity(e - s);
    let mut i = s;
    while i < e {
        let c = from(m.text, i).chars().next().unwrap_or(' ');
        if m.is_masked(i) {
            out.push(' ');
        } else {
            out.push(c);
        }
        i += c.len_utf8().max(1);
    }
    out
}

#[cfg(test)]
mod tests;
