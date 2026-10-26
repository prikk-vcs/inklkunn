//! Heading anchors in the "GitHub style" MKUP-8 recommends: lower-case, punctuation removed, spaces
//! to hyphens, duplicates suffixed `-1`, `-2`. Letters and digits of every script survive.

/// The anchor of a heading's text, before de-duplication.
///
/// Markdown inside the heading is reduced first: code spans lose their backticks, emphasis markers
/// are dropped, `[[Link|label]]` keeps its text, `[text](url)` keeps its text. Then everything that
/// is not a letter, a digit, an underscore, a space or a hyphen is removed, and spaces become hyphens.
pub fn github_anchor(text: &str) -> String {
    let mut s = strip_markdown_links(text);
    s = s.replace(['`', '*'], "");
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

/// Reduce `[[Target|label]]`, `[[Target]]` and `[text](url)` to their visible text.
fn strip_markdown_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(rest.get(..start).unwrap_or(""));
        let after = rest.get(start + 2..).unwrap_or("");
        match after.find("]]") {
            Some(end) => {
                let inner = after.get(..end).unwrap_or("");
                let visible = inner.split_once('|').map_or(inner, |(_, l)| l);
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

/// Assigns anchors to a sequence of heading texts, suffixing duplicates `-1`, `-2`, … in order.
#[derive(Debug, Default)]
pub struct Anchors {
    seen: std::collections::BTreeMap<String, usize>,
}

impl Anchors {
    /// A new, empty de-duplicator.
    pub fn new() -> Self {
        Self::default()
    }

    /// The unique anchor for the next heading with `text`.
    pub fn next(&mut self, text: &str) -> String {
        let base = github_anchor(text);
        let n = self.seen.entry(base.clone()).or_insert(0);
        let anchor = if *n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        *n += 1;
        anchor
    }
}

#[cfg(test)]
mod tests;
