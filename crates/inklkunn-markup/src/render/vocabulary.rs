//! The vocabulary of the HTML this crate writes (MKUP-12): the elements, the attributes and the class names
//! the renderer may emit, as data.
//!
//! **Why it is data.** The renderer's output is not sanitized (it says so, [`crate::Rendered::unsanitized_html`]);
//! `inklkunn-core` sanitizes it under an allowlist that *is* this vocabulary plus the profile's raw-HTML
//! subset (RFC 003 D-3; the manifest's `html_subset`, empty today). One list on each side of that line would
//! drift; one list does not. A test (`tests/vocabulary.rs`) renders every construct and every conformance
//! case and fails when the renderer writes an element, an attribute or a class that is not named here, so a
//! construct added to the renderer without a line here fails before it reaches a reader as a stripped tag.
//!
//! The sanitizer may be **stricter** than the vocabulary, never looser: [`UNALLOWED_ATTRIBUTES`] lists the
//! attributes the Markdown writer emits and no policy keeps.

/// The elements the renderer writes: Markdown's own (CommonMark, tables, strikethrough, task lists,
/// footnotes) and the profile's (callouts and directives, embeds, transclusions, tags, block markers).
pub const ELEMENTS: &[&str] = &[
    "a",
    "aside",
    "blockquote",
    "br",
    "code",
    "del",
    "div",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "img",
    "input",
    "kbd",
    "li",
    "ol",
    "p",
    "pre",
    "span",
    "strong",
    "sup",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
    "tr",
    "ul",
];

/// The attributes particular elements carry. `input` is the task-list checkbox: the renderer writes its
/// `type` and `disabled`, and a sanitizer forces both so that no other control can exist.
pub const ELEMENT_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href", "rel"]),
    ("img", &["alt", "src", "width"]),
    ("input", &["checked", "disabled", "type"]),
    ("ol", &["start"]),
];

/// Attributes any element may carry.
pub const GLOBAL_ATTRIBUTES: &[&str] = &["class", "dir", "id", "lang", "role", "title"];

/// Attribute-name prefixes any element may carry: the constructs' own data (`data-wiki-target`,
/// `data-callout`, …) and accessibility attributes (`aria-label`, …).
pub const GLOBAL_ATTRIBUTE_PREFIXES: &[&str] = &["aria-", "data-"];

/// `(element, attribute)` pairs the Markdown writer emits and no sanitizer keeps: column alignment as an
/// inline `style`, which the page's content security policy blocks anyway (RFC 003 D-3).
pub const UNALLOWED_ATTRIBUTES: &[(&str, &str)] = &[("td", "style"), ("th", "style")];

/// The class names the renderer writes, whole.
pub const CLASSES: &[&str] = &[
    "block-id",
    "callout",
    "callout-title",
    "directive",
    "embed",
    "external",
    "footnote-definition",
    "footnote-definition-label",
    "footnote-reference",
    "image-blocked",
    "link-blocked",
    "tag",
    "transclusion",
    "transclusion-refused",
    "transclusion-source",
    "wikilink",
    "wikilink-attachment",
    "wikilink-blocked",
    "wikilink-embed",
    "wikilink-interwiki",
    "wikilink-missing",
    "wikilink-self",
];

/// Class-name prefixes followed by a name the page chose: the callout's type (`callout-tip`), the directive's
/// name (`directive-toc`), the code block's language (`language-rust`).
pub const CLASS_PREFIXES: &[&str] = &["callout-", "directive-", "language-"];

/// The longest name after a class prefix.
pub const MAX_CLASS_SUFFIX: usize = 48;

/// Is `class` one token the renderer may write: a whole name of [`CLASSES`], or a prefix of
/// [`CLASS_PREFIXES`] followed by 1 to [`MAX_CLASS_SUFFIX`] letters, digits and `_ + . # -`?
pub fn class_is_known(class: &str) -> bool {
    CLASSES.contains(&class)
        || CLASS_PREFIXES.iter().any(|prefix| {
            class.strip_prefix(prefix).is_some_and(|rest| {
                (1..=MAX_CLASS_SUFFIX).contains(&rest.len())
                    && rest.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'.' | b'#' | b'-')
                    })
            })
        })
}

/// May `element` carry `attribute` according to this vocabulary?
pub fn attribute_is_known(element: &str, attribute: &str) -> bool {
    GLOBAL_ATTRIBUTES.contains(&attribute)
        || GLOBAL_ATTRIBUTE_PREFIXES
            .iter()
            .any(|p| attribute.starts_with(p))
        || ELEMENT_ATTRIBUTES
            .iter()
            .any(|(e, attrs)| *e == element && attrs.contains(&attribute))
}

#[cfg(test)]
mod tests;
