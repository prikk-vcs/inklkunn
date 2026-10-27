//! Heading anchors in the "GitHub style" MKUP-8 recommends: lower-case, punctuation removed, spaces
//! to hyphens, duplicates suffixed `-1`, `-2`. Letters and digits of every script survive.
//!
//! One reduction, used by the scanner for the anchors it reports and by the renderer for the ids it
//! writes, so a `[[Page#Heading]]` link lands on the heading's `id`.

use std::collections::{BTreeMap, BTreeSet};

/// The anchor of a heading's text, before de-duplication.
///
/// Markdown inside the heading is reduced first: code spans lose their backticks, emphasis markers
/// (`*`, `_` at word edges) are dropped, `[[Link|label]]` keeps its text, `[text](url)` keeps its text,
/// HTML tags are removed. Then everything that is not a letter, a digit, an underscore, a space or a
/// hyphen is removed, and spaces become hyphens.
pub fn github_anchor(text: &str) -> String {
    let mut s = strip_markdown_links(text);
    s = strip_tags(&s);
    s = s.replace(['`', '*'], "");
    s = strip_underscore_emphasis(&s);
    let mut out = String::with_capacity(s.len());
    for c in s.trim().chars() {
        if c.is_alphanumeric() || c == '_' || unicode_normalization::char::is_combining_mark(c) {
            for l in c.to_lowercase() {
                out.push(l);
            }
        } else if c == ' ' || c == '-' || c == '\u{3000}' {
            out.push('-');
        }
        // every other character is punctuation and is removed
    }
    out
}

/// `_em_` → `em`: underscores that open or close a word are emphasis markers, inner ones are text.
fn strip_underscore_emphasis(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let prev_word = i
                .checked_sub(1)
                .and_then(|p| chars.get(p))
                .is_some_and(|p| p.is_alphanumeric());
            let next_word = chars.get(i + 1).is_some_and(|n| n.is_alphanumeric());
            if prev_word && next_word {
                out.push('_');
            }
            continue;
        }
        out.push(*c);
    }
    out
}

/// Remove `<…>` tags.
fn strip_tags(h: &str) -> String {
    let mut out = String::with_capacity(h.len());
    let mut in_tag = false;
    for c in h.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Reduce `[[Target|label]]`, `[[Target]]`, `![[Embed]]` and `[text](url)` to their visible text.
fn strip_markdown_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let before = rest.get(..start).unwrap_or("");
        out.push_str(before.strip_suffix('!').unwrap_or(before));
        let after = rest.get(start + 2..).unwrap_or("");
        match after.find("]]") {
            Some(end) => {
                let inner = after.get(..end).unwrap_or("");
                let visible = inner.split_once('|').map_or(inner, |(_, l)| l);
                let visible = visible.rsplit('/').next().unwrap_or(visible);
                let visible = visible
                    .split_once('#')
                    .map_or(visible, |(t, f)| if t.is_empty() { f } else { t });
                out.push_str(visible);
                rest = after.get(end + 2..).unwrap_or("");
            }
            None => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // [text](url) → text
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(start) = rest.find('[') {
        let Some(close_rel) = rest.get(start..).and_then(|r| r.find("](")) else {
            break;
        };
        let close = start + close_rel;
        let Some(paren_rel) = rest.get(close + 2..).and_then(|r| r.find(')')) else {
            break;
        };
        result.push_str(rest.get(..start).unwrap_or(""));
        result.push_str(rest.get(start + 1..close).unwrap_or(""));
        rest = rest.get(close + 2 + paren_rel + 1..).unwrap_or("");
    }
    result.push_str(rest);
    result
}

/// Assigns anchors to a sequence of heading texts, suffixing duplicates `-1`, `-2`, … so that every
/// issued id is unique: `a`, `a-1`, `a` → `a`, `a-1`, `a-2` (the second `a` cannot take `a-1`, which is
/// already issued, as GitHub also does not).
#[derive(Debug, Default)]
pub struct Anchors {
    issued: BTreeSet<String>,
    /// The next suffix to try per base, so a page of many equal headings stays linear.
    next_n: BTreeMap<String, usize>,
}

impl Anchors {
    /// A new, empty de-duplicator.
    pub fn new() -> Self {
        Self::default()
    }

    /// The unique anchor for the next heading with `text`.
    pub fn next(&mut self, text: &str) -> String {
        let base = github_anchor(text);
        self.next_from_base(&base)
    }

    /// The unique id for an already reduced base.
    pub fn next_from_base(&mut self, base: &str) -> String {
        let mut n = self.next_n.get(base).copied().unwrap_or(0);
        let mut candidate = if n == 0 {
            base.to_owned()
        } else {
            format!("{base}-{n}")
        };
        while self.issued.contains(&candidate) {
            n += 1;
            candidate = format!("{base}-{n}");
        }
        self.next_n.insert(base.to_owned(), n + 1);
        self.issued.insert(candidate.clone());
        candidate
    }

    /// Record an id issued elsewhere (the scanner's), so later ids avoid it.
    pub fn reserve(&mut self, id: &str) {
        self.issued.insert(id.to_owned());
    }
}

#[cfg(test)]
mod tests;
