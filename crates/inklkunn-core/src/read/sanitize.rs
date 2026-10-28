//! The sanitizer pass (RFC 003 D-3; threat model T-6, C-6b) and the type that proves it ran.
//!
//! **A [`SafeHtml`] can only come out of a [`Sanitizer`].** Its field is private, it has no constructor, no
//! `Default`, no `From<String>` and no deserializer, so a function that takes one (a template, the API's
//! fragment endpoint, a feed entry) cannot be handed HTML that did not pass the policy: the rule "sanitize
//! before any template sees the HTML" is a property of the types, not a habit. The read service builds one
//! for every page it returns, including a page read back from the derived cache: a row of the database is
//! input like any other (T-10), so the cached text is sanitized again on the way out.
//!
//! **The policy is the profile's vocabulary and nothing more.** The allowlist is exactly what
//! `inklkunn-markup` may write ([`inklkunn_markup::render::vocabulary`]) plus the profile's raw-HTML subset
//! ([`HTML_SUBSET`], what the bundle manifest calls `html_subset`: empty today). Because the renderer's own
//! output reaches the sanitizer as the same kind of text as HTML a page's author wrote, raw HTML survives
//! only where it spells something the renderer writes, with the attributes and classes the renderer writes.
//! Beyond the allowlist the policy is stricter than HTML is:
//!
//! - schemes: `http`, `https`, `mailto`, `tel`; a relative URL passes; nothing else (no `javascript:`, no
//!   `data:`);
//! - an image keeps its `src` only when it is relative (an attachment): no remote image is ever loaded on a
//!   reader's behalf (PRV-6), and the image degrades to its alt text;
//! - classes are the renderer's names (and the callout types, directive names and code languages the page
//!   chooses, bounded); `id` is one token; `role` is `note`; `dir` and `lang` are plain; a link's `rel`
//!   keeps only `nofollow`, `ugc`, `noopener` and `noreferrer`;
//! - the one form control is the task-list checkbox, forced to `type="checkbox" disabled`;
//! - comments, scripts, styles, frames, objects, templates and their contents are dropped whole.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use inklkunn_markup::ALLOWED_SCHEMES;
use inklkunn_markup::render::vocabulary::{
    ELEMENT_ATTRIBUTES, ELEMENTS, GLOBAL_ATTRIBUTE_PREFIXES, GLOBAL_ATTRIBUTES, class_is_known,
};

/// Elements a page may write by hand beyond the ones the renderer writes: the profile's raw-HTML subset
/// (`html_subset` in the bundle manifest). **Empty**, as the manifest of every wiki says today; a name added
/// here must be added to the manifest's specification first.
pub const HTML_SUBSET: &[&str] = &[];

/// Elements dropped together with everything inside them: their contents are not text a reader should see.
const DROPPED_WHOLE: &[&str] = &[
    "iframe",
    "math",
    "noembed",
    "noframes",
    "noscript",
    "object",
    "plaintext",
    "script",
    "style",
    "svg",
    "template",
    "textarea",
    "title",
    "xmp",
];

/// HTML that has passed the sanitizer. Only [`Sanitizer`] makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeHtml(String);

impl SafeHtml {
    /// The HTML.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SafeHtml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The policy, built once and shared.
#[derive(Debug)]
pub struct Sanitizer {
    policy: ammonia::Builder<'static>,
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self::profile()
    }
}

impl Sanitizer {
    /// The profile's policy.
    pub fn profile() -> Self {
        let tags: HashSet<&'static str> = ELEMENTS.iter().chain(HTML_SUBSET).copied().collect();
        // `input`'s `type` and `disabled` are allowed so that the renderer's own attributes stay where it put
        // them, and their values are forced below whatever a page wrote. (An attribute the sanitizer has to
        // add is appended in the order of a hash map's iteration, which differs from run to run; the
        // renderer's checkboxes have both and never meet it, so the same page is the same bytes in every
        // process, which a cache that is rebuilt after a restart depends on.)
        let tag_attributes: HashMap<&'static str, HashSet<&'static str>> = ELEMENT_ATTRIBUTES
            .iter()
            .map(|(element, attrs)| (*element, attrs.iter().copied().collect()))
            .collect();
        let schemes: HashSet<&'static str> = ALLOWED_SCHEMES.iter().copied().collect();
        let mut policy = ammonia::Builder::empty();
        policy
            .tags(tags)
            .clean_content_tags(DROPPED_WHOLE.iter().copied().collect())
            .tag_attributes(tag_attributes)
            .generic_attributes(GLOBAL_ATTRIBUTES.iter().copied().collect())
            .generic_attribute_prefixes(GLOBAL_ATTRIBUTE_PREFIXES.iter().copied().collect())
            .url_schemes(schemes)
            .url_relative(ammonia::UrlRelative::PassThrough)
            .link_rel(None)
            .strip_comments(true)
            .set_tag_attribute_value("input", "type", "checkbox")
            .set_tag_attribute_value("input", "disabled", "")
            .attribute_filter(filter_attribute);
        Sanitizer { policy }
    }

    /// Sanitize `html`.
    pub fn clean(&self, html: &str) -> SafeHtml {
        SafeHtml(self.policy.clean(html).to_string())
    }

    /// Plain text made into HTML: every character that means something to an HTML parser is escaped. For the
    /// places a surface wants an HTML-typed value for text a person wrote (a title, an alias, a description).
    pub fn text(&self, plain: &str) -> SafeHtml {
        SafeHtml(ammonia::clean_text(plain))
    }
}

/// The per-attribute rules ammonia's allowlist cannot express.
fn filter_attribute<'v>(element: &str, attribute: &str, value: &'v str) -> Option<Cow<'v, str>> {
    let keep = Some(Cow::Borrowed(value));
    match (element, attribute) {
        (_, "class") => keep_known_classes(value),
        (_, "id") => (is_one_token(value) && value.len() <= 200).then_some(Cow::Borrowed(value)),
        (_, "role") => (value == "note").then_some(Cow::Borrowed(value)),
        (_, "dir") => matches!(value, "ltr" | "rtl" | "auto").then_some(Cow::Borrowed(value)),
        (_, "lang") => is_language_tag(value).then_some(Cow::Borrowed(value)),
        ("a", "rel") => keep_known_rel(value),
        ("img", "src") => is_attachment_url(value).then_some(Cow::Borrowed(value)),
        ("img", "width") => is_digits(value, 4).then_some(Cow::Borrowed(value)),
        ("ol", "start") => is_digits(value, 9).then_some(Cow::Borrowed(value)),
        _ => keep,
    }
}

/// The class tokens the renderer writes, in their order; the attribute goes when none is left.
fn keep_known_classes(value: &str) -> Option<Cow<'_, str>> {
    let known: Vec<&str> = value
        .split_ascii_whitespace()
        .filter(|c| class_is_known(c))
        .collect();
    if known.is_empty() {
        None
    } else if known.join(" ") == value {
        Some(Cow::Borrowed(value))
    } else {
        Some(Cow::Owned(known.join(" ")))
    }
}

/// Only the relation tokens that mean something to a crawler or a browser and cost a reader nothing.
fn keep_known_rel(value: &str) -> Option<Cow<'_, str>> {
    let known: Vec<&str> = value
        .split_ascii_whitespace()
        .filter(|t| matches!(*t, "nofollow" | "ugc" | "noopener" | "noreferrer"))
        .collect();
    (!known.is_empty()).then(|| Cow::Owned(known.join(" ")))
}

fn is_one_token(value: &str) -> bool {
    !value.is_empty() && !value.bytes().any(|b| b.is_ascii_whitespace() || b < 0x20)
}

fn is_digits(value: &str, at_most: usize) -> bool {
    (1..=at_most).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}

fn is_language_tag(value: &str) -> bool {
    let mut parts = value.split('-');
    parts
        .next()
        .is_some_and(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()))
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// A URL that names a file of this wiki and not a place elsewhere: no scheme, not scheme-relative.
fn is_attachment_url(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.starts_with("//") || v.starts_with('\\') {
        return false;
    }
    match v.find(':') {
        None => true,
        Some(colon) => v.find(['/', '?', '#']).is_some_and(|i| i < colon),
    }
}

#[cfg(test)]
mod tests;
