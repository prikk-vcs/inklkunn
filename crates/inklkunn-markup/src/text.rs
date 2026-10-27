//! Small text helpers shared by the modules: index-free slicing (the workspace denies `indexing_slicing`,
//! and a parser that cannot panic on a bad offset is the point), a line index, HTML escaping,
//! percent-encoding, and the normalizations MKUP-19 and MKUP-5 ask for.

use unicode_normalization::UnicodeNormalization;

/// `s[r]` that yields `""` instead of panicking on a bad or non-boundary range.
pub(crate) fn sl(s: &str, start: usize, end: usize) -> &str {
    s.get(start..end).unwrap_or("")
}

/// `s[start..]` without the panic.
pub(crate) fn from(s: &str, start: usize) -> &str {
    s.get(start..).unwrap_or("")
}

/// The byte at `i`, or a space when out of range.
pub(crate) fn byte(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(b' ')
}

/// The line structure of a text, computed once: byte offsets of line starts, so that the line of any
/// offset is a binary search and a line's range is a lookup.
#[derive(Debug, Clone)]
pub(crate) struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub(crate) fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        LineIndex {
            starts,
            len: text.len(),
        }
    }

    /// 1-based line number of byte offset `pos`.
    pub(crate) fn line_of(&self, pos: usize) -> usize {
        match self.starts.binary_search(&pos) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }

    /// The number of lines (a trailing newline does not start a new line of content, but is counted as
    /// one, like `split('\n')`).
    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    /// The `(start, end)` byte range of 1-based line `no`, without the `\n`.
    pub(crate) fn range(&self, no: usize) -> Option<(usize, usize)> {
        let start = *self.starts.get(no.checked_sub(1)?)?;
        let end = self
            .starts
            .get(no)
            .map_or(self.len, |n| n.saturating_sub(1));
        Some((start, end.max(start)))
    }

    /// Every line range in order.
    pub(crate) fn ranges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (1..=self.count()).filter_map(|n| self.range(n))
    }
}

/// NFC and trimmed: the first step of every comparison (MKUP-19).
pub fn nfc_trim(s: &str) -> String {
    let n: String = s.nfc().collect();
    n.trim().to_owned()
}

/// The MKUP-5 comparison key: NFC, trimmed, runs of whitespace, `_` and `-` collapsed to one space,
/// lower-cased (the closest `std` offers to Unicode case folding).
pub fn normalize(s: &str) -> String {
    let n = nfc_trim(s);
    let mut out = String::with_capacity(n.len());
    let mut pending_sep = false;
    for c in n.chars() {
        if c.is_whitespace() || c == '_' || c == '-' {
            pending_sep = true;
            continue;
        }
        if pending_sep && !out.is_empty() {
            out.push(' ');
        }
        pending_sep = false;
        for l in c.to_lowercase() {
            out.push(l);
        }
    }
    out
}

/// Is `c` a "word" character in the sense of a Unicode-aware `\w`: a letter, a digit, a mark or `_`.
pub(crate) fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || unicode_normalization::char::is_combining_mark(c)
}

/// Escape text for an HTML text node or a double-quoted attribute value.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a URL **for the attribute context only**: URL characters pass (with `&` and quotes
/// entity-escaped), everything else is percent-encoded as UTF-8. This does not judge the URL: a
/// `javascript:` scheme passes through unchanged. Callers that emit an `href` decide the scheme with
/// [`href_is_allowed`] first; the web layer's sanitizer is the second line.
pub fn escape_href(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            c if c.is_ascii_alphanumeric() || "-._~:/?#[]@!$()*+,;=%".contains(c) => out.push(c),
            c => percent_encode_char(c, &mut out),
        }
    }
    out
}

/// Schemes an `href` this crate emits may carry; anything else with a scheme is refused and the link
/// rendered as plain text. A URL without a scheme (relative, absolute-path or fragment) is allowed.
pub const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// Is `url` acceptable as an `href`: no scheme, or one of [`ALLOWED_SCHEMES`] (case-insensitive)? The
/// scheme is what precedes the first `:` when that comes before any `/`, `?` or `#`.
pub fn href_is_allowed(url: &str) -> bool {
    let t = url.trim();
    if t.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return false;
    }
    let Some(colon) = t.find(':') else {
        return true;
    };
    if t.find(['/', '?', '#']).is_some_and(|i| i < colon) {
        return true;
    }
    let scheme = sl(t, 0, colon);
    ALLOWED_SCHEMES
        .iter()
        .any(|s| s.eq_ignore_ascii_case(scheme))
}

/// Percent-encode like Python's `urllib.parse.quote` with its default safe set (`/`): ASCII letters,
/// digits, `_.-~` and `/` pass; everything else becomes `%XX` per UTF-8 byte.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '~' | '/') {
            out.push(c);
        } else {
            percent_encode_char(c, &mut out);
        }
    }
    out
}

pub(crate) fn percent_encode_char(c: char, out: &mut String) {
    let mut buf = [0u8; 4];
    for b in c.encode_utf8(&mut buf).bytes() {
        out.push_str(&format!("%{b:02X}"));
    }
}

/// POSIX `normpath(join(dirname(from), rel))` without the `.md`, for `./` and `../` link targets.
pub(crate) fn relative_target(from_path: &str, rel: &str) -> String {
    let dir = from_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = Vec::new();
    for c in dir.split('/').chain(rel.split('/')) {
        match c {
            "" | "." => {}
            ".." => {
                if matches!(parts.last(), Some(&"..")) || parts.is_empty() {
                    parts.push("..");
                } else {
                    parts.pop();
                }
            }
            c => parts.push(c),
        }
    }
    let joined = parts.join("/");
    strip_md(&joined).to_owned()
}

/// `path` without a trailing `.md` (case-insensitive).
pub(crate) fn strip_md(path: &str) -> &str {
    if path.len() >= 3
        && path
            .get(path.len() - 3..)
            .is_some_and(|e| e.eq_ignore_ascii_case(".md"))
    {
        sl(path, 0, path.len() - 3)
    } else {
        path
    }
}

#[cfg(test)]
mod tests;
