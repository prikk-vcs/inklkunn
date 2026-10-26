//! Small text helpers shared by the modules: index-free slicing (the workspace denies `indexing_slicing`,
//! and a parser that cannot panic on a bad offset is the point), HTML escaping, percent-encoding, and
//! the normalizations MKUP-19 and MKUP-5 ask for.

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

/// 1-based line number of byte offset `pos` in `text`.
pub(crate) fn line_of(text: &str, pos: usize) -> usize {
    sl(text, 0, pos).bytes().filter(|b| *b == b'\n').count() + 1
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

/// Escape a URL for an `href` attribute: URL characters pass (with `&` and quotes entity-escaped),
/// everything else is percent-encoded as UTF-8.
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

fn percent_encode_char(c: char, out: &mut String) {
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
