//! The one quiet link on a paragraph that holds a claim (RFC 005 part 2 D-7; handoff K-T6).
//!
//! **A page with no claims is the page it was.** The read service's body is cut into stretches only where a
//! claimed paragraph ends, and a template prints the stretches one after the other with the link between them;
//! with nothing to mark there is one stretch, the whole body, byte for byte. The body is sanitized HTML (a
//! `SafeHtml`) and is never changed: it is only cut, at the `</p>` or `</li>` of a paragraph, and every byte of
//! the link is written by the template from the catalog's words and an address this crate builds.
//!
//! **Where a claim is.** The knowledge index says which blocks of a page (by the page's frontmatter `id` and the
//! block's `^id`) hold a claim and that the page really has each once. The renderer writes a paragraph or a list
//! item's block id as `<p id="h-^b1">`/`<li id="h-^b1">` (the page's id prefix, then `^`), so a claimed
//! paragraph is found by that opening tag. A claim whose block is some other kind of block (a table, a code
//! block, a quotation) is offered no link here; its page is still there.
//!
//! **The link's accessible name** is distinct per paragraph: it says which paragraph, by its first words, and a
//! paragraph that holds several claims has one link per claim, each named by its number. Two paragraphs that
//! begin alike are told apart by a count.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use askama::filters::HtmlSafe;
use inklkunn_core::SafeHtml;
use inklkunn_core::knowledge::index::KnowledgeIndex;
use inklkunn_core::read::{HEADING_ID_PREFIX, PageView};
use inklkunn_store::WikiRecord;

use super::paths;
use crate::catalog::{Key, T};
use crate::services::Services;

/// The most characters of a paragraph's beginning that name its link.
const START_CHARS: usize = 40;

/// A claim said in one block of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParagraphClaim {
    /// The block's `^id`, without the `^`.
    pub(crate) block: String,
    /// The claim's id.
    pub(crate) claim: String,
}

/// The claims a page offers a reader: those said at blocks it really has. A page without an `id` in its
/// frontmatter cannot be named by a claim, so it costs no lookup.
pub(crate) fn for_page(
    services: &Services,
    wiki: &WikiRecord,
    view: Option<&str>,
    page: &PageView,
) -> Vec<ParagraphClaim> {
    let Some(id) = page.meta.id.as_deref() else {
        return Vec::new();
    };
    match services.read.knowledge_index(wiki, view) {
        Ok(index) => offered(&index, id),
        Err(e) => {
            // The page is served all the same, without the links; the operator hears why.
            (services.log)(&format!(
                "the claims of a page of wiki `{}` could not be read now: {e}",
                wiki.slug
            ));
            Vec::new()
        }
    }
}

/// The claims said in the page whose frontmatter `id` is `page_id`, by block and claim.
pub(crate) fn offered(index: &KnowledgeIndex, page_id: &str) -> Vec<ParagraphClaim> {
    index
        .claims_on_page(page_id)
        .map(|a| ParagraphClaim {
            block: a.block.clone(),
            claim: a.claim.clone(),
        })
        .collect()
}

/// A stretch of a sanitized page body, printed as it is: it is only ever cut out of a [`SafeHtml`], at the end
/// of a paragraph, and printed in order with nothing taken out.
pub(crate) struct Fragment(String);

impl fmt::Display for Fragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl HtmlSafe for Fragment {}

/// The link to a claim's page, as the template draws it.
pub(crate) struct Tool {
    /// Where it leads.
    pub(crate) href: String,
    /// What it says: *Sources and views*.
    pub(crate) text: String,
    /// What a screen reader says: the text, and which paragraph.
    pub(crate) name: String,
}

/// A stretch of the body, and the links that stand at its end (inside the paragraph it ends).
pub(crate) struct BodyPart {
    pub(crate) html: Fragment,
    pub(crate) tools: Vec<Tool>,
}

/// One claimed paragraph found in the body.
struct Span {
    block: String,
    /// Just after the opening tag.
    inside: usize,
    /// The start of the closing tag.
    close: usize,
}

/// The body cut at the end of every claimed paragraph, with the links to put there.
pub(crate) fn parts(
    html: &SafeHtml,
    claims: &[ParagraphClaim],
    t: T,
    slug: &str,
    view: Option<&str>,
) -> Vec<BodyPart> {
    let body = html.as_str();
    let mut wanted: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for c in claims {
        wanted.entry(c.block.as_str()).or_default().push(&c.claim);
    }
    let mut spans = if wanted.is_empty() {
        Vec::new()
    } else {
        claimed_paragraphs(body, &wanted)
    };
    spans.sort_by_key(|s| s.close);

    let mut used: HashMap<String, usize> = HashMap::new();
    let mut parts = Vec::new();
    let mut cursor = 0;
    for span in spans {
        let (Some(before), Some(inside)) = (
            body.get(cursor..span.close),
            body.get(span.inside..span.close),
        ) else {
            continue;
        };
        let here = wanted
            .get(span.block.as_str())
            .map_or(&[][..], |v| v.as_slice());
        let start = beginning(inside);
        let paragraph = if start.is_empty() {
            t.get(Key::ClaimParagraphEmpty)
        } else {
            t.msg(Key::ClaimParagraphStart).with("text", start).text()
        };
        let tools = here
            .iter()
            .enumerate()
            .map(|(i, claim)| {
                let name = if here.len() == 1 {
                    t.msg(Key::ClaimLinkName)
                        .with("paragraph", &paragraph)
                        .text()
                } else {
                    t.msg(Key::ClaimLinkNameNth)
                        .with("n", i + 1)
                        .with("total", here.len())
                        .with("paragraph", &paragraph)
                        .text()
                };
                Tool {
                    href: paths::claim(slug, claim, view),
                    text: t.get(Key::ClaimLink),
                    name: distinct(&mut used, name),
                }
            })
            .collect();
        parts.push(BodyPart {
            html: Fragment(before.to_owned()),
            tools,
        });
        cursor = span.close;
    }
    parts.push(BodyPart {
        html: Fragment(body.get(cursor..).unwrap_or_default().to_owned()),
        tools: Vec::new(),
    });
    parts
}

/// A name no earlier link has: the second paragraph that begins alike is told from the first by a count.
pub(super) fn distinct(used: &mut HashMap<String, usize>, name: String) -> String {
    let seen = used.entry(name.clone()).or_insert(0);
    *seen += 1;
    if *seen == 1 {
        name
    } else {
        format!("{name} ({seen})")
    }
}

/// The paragraphs (and list items) of the body that carry one of the wanted block ids. A block id the body
/// writes on more than one element is ambiguous and is found nowhere.
fn claimed_paragraphs(body: &str, wanted: &BTreeMap<&str, Vec<&str>>) -> Vec<Span> {
    let mut found: HashMap<String, Option<Span>> = HashMap::new();
    for tag in ["p", "li"] {
        let open = format!("<{tag} id=\"{HEADING_ID_PREFIX}^");
        for (at, _) in body.match_indices(&open) {
            let start = at + open.len();
            let Some(rest) = body.get(start..) else {
                continue;
            };
            let Some(end) = rest.find("\">") else {
                continue;
            };
            let Some(block) = rest.get(..end).filter(|b| wanted.contains_key(*b)) else {
                continue;
            };
            let inside = start + end + 2;
            let span = closing(body, inside, tag).map(|close| Span {
                block: block.to_owned(),
                inside,
                close,
            });
            // The second element with the same id spoils it for both.
            found
                .entry(block.to_owned())
                .and_modify(|s| *s = None)
                .or_insert(span);
        }
    }
    found.into_values().flatten().collect()
}

/// Where the element opened just before `from` is closed: the start of its closing tag, counting elements of
/// the same name opened inside it.
fn closing(body: &str, from: usize, tag: &str) -> Option<usize> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut depth = 1_usize;
    let mut at = from;
    loop {
        let rest = body.get(at..)?;
        let next_close = rest.find(&close)?;
        let head = rest.get(..next_close)?;
        let next_open = head.match_indices(&open).map(|(i, _)| i).find(|i| {
            head.get(i + open.len()..)
                .is_some_and(|r| r.starts_with('>') || r.starts_with(' '))
        });
        if let Some(o) = next_open {
            depth += 1;
            at += o + open.len();
        } else {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(at + next_close);
            }
            at += next_close + close.len();
        }
    }
}

/// The first words of a stretch of sanitized HTML, as plain text: tags dropped, the few entities the sanitizer
/// writes read back, white space collapsed, at most [`START_CHARS`] characters (and an ellipsis when it was cut).
fn beginning(fragment: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = text.chars();
    let start: String = chars.by_ref().take(START_CHARS).collect();
    if chars.next().is_some() {
        format!("{start}…")
    } else {
        start
    }
}

#[cfg(test)]
mod tests;
