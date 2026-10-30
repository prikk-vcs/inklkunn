//! Atom feeds (RFC 4287; RFC 003 D-7; requirements IX-5): the recent changes of a wiki, of a namespace and of a
//! page. Written by hand for the one shape this crate needs, so no XML library enters the lock; the escaping is
//! the only place text becomes markup here, and it is one function.
//!
//! **Every piece of text in a feed is escaped** ([`escape`]), and characters XML 1.0 cannot carry at all are
//! dropped rather than written: a title with a control character in it cannot make the feed unreadable to the
//! readers that subscribe to it. Entry ids are `urn:inklkunn:` ids built from the wiki's stable id and the
//! change's block id, so they survive a rename of the wiki's address and a move of the server.

use std::fmt::Write as _;

use crate::time::rfc3339;

/// One entry: a change.
pub(crate) struct Entry {
    pub id: String,
    pub title: String,
    /// Unix seconds: when this server first saw the change.
    pub updated: i64,
    pub href: String,
    /// A link to what the change did (the diff), if one is known.
    pub related: Option<String>,
    pub author: String,
    pub summary: Option<String>,
    /// What the change is flagged as (`minor`, `automated`): a category each, so a reader can filter on it.
    pub categories: Vec<&'static str>,
}

/// A feed.
pub(crate) struct Feed {
    pub lang: &'static str,
    pub id: String,
    pub title: String,
    pub updated: i64,
    pub self_href: String,
    pub alternate_href: String,
    pub author: String,
    pub version: &'static str,
    pub entries: Vec<Entry>,
}

/// Text for an XML text node or a double-quoted attribute: `& < > " '` escaped, characters XML 1.0 forbids
/// dropped.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || matches!(c as u32, 0xFFFE | 0xFFFF) => {}
            c => out.push(c),
        }
    }
    out
}

impl Feed {
    /// The document.
    pub(crate) fn render(&self) -> String {
        let mut x = String::new();
        let _ = writeln!(x, "<?xml version=\"1.0\" encoding=\"utf-8\"?>");
        let _ = writeln!(
            x,
            "<feed xmlns=\"http://www.w3.org/2005/Atom\" xml:lang=\"{}\">",
            escape(self.lang)
        );
        let _ = writeln!(x, "<title>{}</title>", escape(&self.title));
        let _ = writeln!(x, "<id>{}</id>", escape(&self.id));
        let _ = writeln!(x, "<updated>{}</updated>", rfc3339(self.updated));
        let _ = writeln!(
            x,
            "<link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>",
            escape(&self.self_href)
        );
        let _ = writeln!(
            x,
            "<link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>",
            escape(&self.alternate_href)
        );
        let _ = writeln!(x, "<author><name>{}</name></author>", escape(&self.author));
        let _ = writeln!(
            x,
            "<generator version=\"{}\">inklkunn</generator>",
            escape(self.version)
        );
        for e in &self.entries {
            let _ = writeln!(x, "<entry>");
            let _ = writeln!(x, "<title>{}</title>", escape(&e.title));
            let _ = writeln!(x, "<id>{}</id>", escape(&e.id));
            let _ = writeln!(x, "<updated>{}</updated>", rfc3339(e.updated));
            let _ = writeln!(
                x,
                "<link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>",
                escape(&e.href)
            );
            if let Some(related) = &e.related {
                let _ = writeln!(
                    x,
                    "<link rel=\"related\" type=\"text/html\" href=\"{}\"/>",
                    escape(related)
                );
            }
            let _ = writeln!(x, "<author><name>{}</name></author>", escape(&e.author));
            for term in &e.categories {
                let _ = writeln!(x, "<category term=\"{}\"/>", escape(term));
            }
            if let Some(summary) = &e.summary {
                let _ = writeln!(x, "<summary type=\"text\">{}</summary>", escape(summary));
            }
            let _ = writeln!(x, "</entry>");
        }
        let _ = writeln!(x, "</feed>");
        x
    }
}

#[cfg(test)]
mod tests;
