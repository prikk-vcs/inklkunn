//! The interwiki map: `governance/interwiki.yaml` (RFC 007 D-8; handoff I-T4). **The third governance file**, and the only
//! one that is not about who may do what: it says where a link such as `[[wikipedia:Rust]]` leads.
//!
//! ```yaml
//! wikipedia: https://en.wikipedia.org/wiki/{title}
//! docs: "https://example.org/docs?page={title}"
//! ```
//!
//! **The file is a flat map of prefixes to templates**, the shape the bundle manifest's `interwiki` has, so that an import
//! writes the bundle's map as it is. It has no keys of its own: every key is a prefix, and a key that cannot be one is an
//! [`UnknownKey`](Refusal::UnknownKey). (A later version cannot add a key without taking a prefix from someone, and so a
//! later version would be another file: that is a reason this one says what it is, and none says `version`.) An empty file is
//! an empty map.
//!
//! # What a prefix and a template are
//!
//! - **A prefix** is ASCII letters, digits and `-`, one to [`MAX_PREFIX_CHARS`] characters, and **is none of the wiki's own
//!   namespaces** ([`namespaces_of_titles`]; compared without regard to the case of ASCII letters, so that `help` and `Help`
//!   are not two names for a reader). The namespaces are the wiki's *at the time the change is accepted*: a page made later
//!   with a prefix's name as its namespace is the reader's to see ([`Interwiki`] wins in the resolver, as a link with a
//!   mapped prefix always has), and no map is refused later for it.
//! - **A template** is `https://` and a host, no user information, **exactly one `{title}`, in the path or the query**, no
//!   other brace, and only the characters a URL is written with (ASCII, no space, none of `"<>\^`|`): what the title is
//!   replaced by is percent-encoded (a `/` is kept, for subpages), so that it cannot end the path, start the query or the
//!   fragment, or add a parameter. At most [`MAX_TEMPLATE_CHARS`] characters. A fixed fragment after the placeholder's place
//!   is allowed; the placeholder in the fragment is not.
//! - **A title that has a segment `.` or `..`** (`[[gh:../../x]]`) **is no link at all**: it stays dangling, as a link with an
//!   unknown prefix does. Encoding the dots would not keep the link under the template's path, because a browser reads `%2E%2E`
//!   as `..` as well (the URL standard's *double-dot path segment*), so the one way to keep a title from climbing out of the
//!   path it stands in is to not make it a link.
//! - **At most [`MAX_INTERWIKI_PREFIXES`] prefixes**, and a file of at most [`MAX_INTERWIKI_BYTES`].
//!
//! # Three ways to a map
//!
//! - [`validate_interwiki`]: **what acceptance asks** of a proposed file (the namespaces are the wiki's).
//! - [`read_interwiki`]: **what the service asks of the file in the repository**: the same rules except the clash with a
//!   namespace, and its fault is a [`ReadFault`] (it makes the wiki refuse writes, as a policy that cannot be read does).
//! - [`judge_interwiki`]: **what an import asks of a bundle's map**, one prefix at a time: a prefix that fails is left out
//!   and listed, and the rest are the wiki's map ([`LeftOut`]).
//!
//! **Nothing here repeats what a file said.** A fault names the file, the rule, the schema's field (`prefix` or `template`)
//! and the line.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_markup::Value;

use super::chars::quoted;
use super::fault::{Fault, GovernanceFile, ReadFault, Refusal};
use super::locate::{Step, line_of};
use super::parse::prepare;

/// The most prefixes a map has.
pub const MAX_INTERWIKI_PREFIXES: usize = 256;

/// The most characters of a prefix.
pub const MAX_PREFIX_CHARS: usize = 32;

/// The most characters of a template.
pub const MAX_TEMPLATE_CHARS: usize = 512;

/// The most bytes `governance/interwiki.yaml` may have: [`MAX_INTERWIKI_PREFIXES`] prefixes and templates at their
/// limits, and the punctuation between them, fit.
pub const MAX_INTERWIKI_BYTES: usize = 160 * 1024;

/// The placeholder, and the only one.
const PLACEHOLDER: &str = "{title}";

/// The field of the schema a refusal about a prefix names.
pub(super) const PREFIX_FIELD: &str = "prefix";

/// The field of the schema a refusal about a template names.
pub(super) const TEMPLATE_FIELD: &str = "template";

/// An interwiki map that was read or judged sound: **prefix to template, each template ready for the resolver**.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Interwiki {
    map: BTreeMap<String, String>,
}

impl Interwiki {
    /// The map with no prefix: what a wiki without the file has.
    pub fn empty() -> Interwiki {
        Interwiki::default()
    }

    /// The template of a prefix.
    pub fn get(&self, prefix: &str) -> Option<&str> {
        self.map.get(prefix).map(String::as_str)
    }

    /// The prefixes and their templates, by prefix.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(p, t)| (p.as_str(), t.as_str()))
    }

    /// How many prefixes.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Has it no prefix?
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The map as the resolver takes it ([`inklkunn_markup::Index::resolve_with`]), without a copy.
    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.map
    }

    /// The map as the resolver's own field takes it ([`inklkunn_markup::Index::interwiki`]).
    pub fn to_map(&self) -> BTreeMap<String, String> {
        self.map.clone()
    }

    /// **Does a page titled `title` have a namespace the map has taken?** The rule of correction W-5, the other direction of
    /// [`validate_interwiki`]'s clash of a prefix with a namespace: a link `[[gh:Notes]]` leads to the map's `gh`, so a page `GH:Notes`
    /// (or `gh/Notes`, a directory `gh`) could never be reached by its own title, and one name would mean two things. A namespace is
    /// what [`namespaces_of_titles`] calls one, and it is compared with the prefixes **without regard to the case of ASCII letters**
    /// (`GH` is `gh`). The answer is the first namespace of the title (the directory, then the part before the colon) that the map has.
    pub fn takes_namespace_of(&self, title: &str) -> Option<MappedNamespace> {
        if self.map.is_empty() {
            return None;
        }
        namespaces_of_title(title).find_map(|namespace| {
            self.map
                .iter()
                .find(|(prefix, _)| prefix.to_ascii_lowercase() == namespace)
                .map(|(prefix, template)| MappedNamespace {
                    prefix: prefix.clone(),
                    host: host_of(template),
                })
        })
    }

    /// A short name for this map's content (the first 16 hex digits of the SHA-256 of [`Interwiki::to_yaml`]): two maps with the
    /// same prefixes and templates have the same one. For a key under which something resolved with the map is kept; empty for an
    /// empty map.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        if self.map.is_empty() {
            return String::new();
        }
        let digest = Sha256::digest(self.to_yaml().as_bytes());
        digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
    }

    /// The canonical text of the file: every template quoted, in prefix order. Reading it back gives the map again.
    pub fn to_yaml(&self) -> String {
        if self.map.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (prefix, template) in &self.map {
            out.push_str(prefix);
            out.push_str(": ");
            out.push_str(&quoted(template));
            out.push('\n');
        }
        out
    }
}

/// **A page title that is in a namespace the interwiki map has taken** (correction W-5): the prefix as the map spells it and the host
/// of the template it leads to. Both are the map's, validated when the map was read (a prefix is ASCII letters, digits and `-`; a
/// host is dot-separated labels and an optional port), and **never the title's own words**: a surface may say them as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedNamespace {
    prefix: String,
    host: String,
}

impl MappedNamespace {
    /// The prefix, as the map spells it.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Where the prefix leads: the host of its template (with its port, when it has one).
    pub fn host(&self) -> &str {
        &self.host
    }
}

impl std::fmt::Display for MappedNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the interwiki map has the prefix `{}`, which leads to {}: a page cannot have it as its namespace",
            self.prefix, self.host
        )
    }
}

/// A prefix of a bundle's map that the wiki did not take, and why: **the prefix as the bundle wrote it** (untrusted text: a
/// surface escapes it) and the rule it broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// The prefix, as the bundle wrote it.
    pub prefix: String,
    /// The rule the prefix or its template broke.
    pub refusal: Refusal,
}

/// The namespaces of **one title**, as the clash rule asks them: **the first segment of a title that has a `/`** (a directory is a
/// namespace) **and the part before the first `:`** of the title's first segment (`Help:Contents` is in `Help`), each in lower case,
/// kept only when it could be a prefix; in that order, none, one or two. **The one definition both directions of the rule use**: the
/// clash of a prefix with the wiki's namespaces ([`namespaces_of_titles`]) and the clash of a page's title with a mapped prefix
/// ([`Interwiki::takes_namespace_of`]), so that the two cannot disagree.
fn namespaces_of_title(title: &str) -> impl Iterator<Item = String> {
    let (first, nested) = match title.split_once('/') {
        Some((first, _)) => (first, true),
        None => (title, false),
    };
    let directory = (nested && is_prefix(first)).then(|| first.to_ascii_lowercase());
    let colon = first
        .split_once(':')
        .map(|(before, _)| before)
        .filter(|before| is_prefix(before))
        .map(str::to_ascii_lowercase);
    directory.into_iter().chain(colon)
}

/// The namespaces a wiki has, as the clash rule asks them: from its page titles (each title's first segment when it has a `/`, and the part of its first segment before a `:`, in lower case; see `Interwiki::takes_namespace_of`).
/// The titles are the wiki's (the caller reads them from the title index of the state the change is accepted on).
pub fn namespaces_of_titles<'a>(titles: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    titles.into_iter().flat_map(namespaces_of_title).collect()
}

/// Is `s` a prefix by its characters and length: one to [`MAX_PREFIX_CHARS`] ASCII letters, digits and `-`?
fn is_prefix(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_PREFIX_CHARS
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The rule a prefix breaks, or `None`: its characters (an [`UnknownKey`](Refusal::UnknownKey): the file's keys are the
/// prefixes, so a key that is none is one the schema does not have), its length, and the clash with a namespace.
fn prefix_rule(prefix: &str, namespaces: Option<&BTreeSet<String>>) -> Option<Refusal> {
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Some(Refusal::UnknownKey);
    }
    if prefix.len() > MAX_PREFIX_CHARS {
        return Some(Refusal::Bounds);
    }
    if namespaces.is_some_and(|ns| ns.contains(&prefix.to_ascii_lowercase())) {
        return Some(Refusal::PrefixIsNamespace);
    }
    None
}

/// The host of a template that [`template_rule`] accepted: what stands between `https://` and the first `/`, `?` or `#`.
fn host_of(template: &str) -> String {
    let rest = template.strip_prefix("https://").unwrap_or(template);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest.get(..end).unwrap_or_default().to_owned()
}

/// The rule a template breaks, or `None`. In this order: its length, its characters, its scheme, user information, where
/// the one placeholder stands, and its host. The first rule broken is the one named.
fn template_rule(template: &str) -> Option<Refusal> {
    if template.chars().count() > MAX_TEMPLATE_CHARS {
        return Some(Refusal::Bounds);
    }
    if !template.bytes().all(|b| {
        b.is_ascii_graphic() && !matches!(b, b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'|')
    }) {
        return Some(Refusal::TemplateCharacter);
    }
    let Some(rest) = template.strip_prefix("https://") else {
        return Some(Refusal::TemplateScheme);
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or("");
    if authority.contains('@') {
        return Some(Refusal::TemplateUserInfo);
    }
    // The placeholder: one, whole, after the authority and before a fragment, and no other brace.
    let fragment_start = rest.find('#').unwrap_or(rest.len());
    let Some((at, _)) = rest.match_indices(PLACEHOLDER).next() else {
        return Some(Refusal::TemplatePlaceholder);
    };
    if at < authority_end || at > fragment_start {
        return Some(Refusal::TemplatePlaceholder);
    }
    // With the first `{title}` taken out no brace is left: a second `{title}` and any other placeholder are braces.
    if rest.replacen(PLACEHOLDER, "", 1).contains(['{', '}']) {
        return Some(Refusal::TemplatePlaceholder);
    }
    host_rule(authority)
}

/// The rule the authority (`host` or `host:port`) breaks: a host of dot-separated labels of ASCII letters, digits and `-`
/// (none empty, none starting or ending with `-`), at most 253 characters, and an optional port of one to five digits.
fn host_rule(authority: &str) -> Option<Refusal> {
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if port.is_some_and(|p| p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Some(Refusal::TemplateHost);
    }
    let label_ok = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if host.is_empty() || host.len() > 253 || !host.split('.').all(label_ok) {
        return Some(Refusal::TemplateHost);
    }
    None
}

/// The map of a text, or the first fault. `namespaces` is `Some` when the clash with a namespace is judged.
fn parse(text: &str, namespaces: Option<&BTreeSet<String>>) -> Result<Interwiki, Fault> {
    let file = GovernanceFile::Interwiki;
    let value = prepare(file, text)?;
    let Value::Map(pairs) = value else {
        return Err(Fault::whole(file, Refusal::WrongShape));
    };
    if pairs.len() > MAX_INTERWIKI_PREFIXES {
        return Err(Fault::at(file, Refusal::Bounds, PREFIX_FIELD));
    }
    let located = |mut fault: Fault, key: &str| {
        fault.line = line_of(text, &[Step::Key(key.to_owned())]);
        fault
    };
    let mut map = BTreeMap::new();
    for (prefix, value) in &pairs {
        if let Some(rule) = prefix_rule(prefix, namespaces) {
            return Err(located(Fault::at(file, rule, PREFIX_FIELD), prefix));
        }
        let template = match value {
            Value::Str(s) => s,
            _ => {
                return Err(located(
                    Fault::at(file, Refusal::WrongShape, TEMPLATE_FIELD),
                    prefix,
                ));
            }
        };
        if let Some(rule) = template_rule(template) {
            return Err(located(Fault::at(file, rule, TEMPLATE_FIELD), prefix));
        }
        // The parser refuses a key written twice; this keeps the map honest if it ever did not.
        if map.insert(prefix.clone(), template.clone()).is_some() {
            return Err(located(
                Fault::at(file, Refusal::RepeatedKey, PREFIX_FIELD),
                prefix,
            ));
        }
    }
    Ok(Interwiki { map })
}

/// Judge the text of a proposed `governance/interwiki.yaml` for acceptance: wholly well formed, within its bounds, and with no
/// prefix that is one of the wiki's `namespaces` ([`namespaces_of_titles`]). The error names the file, the rule, the field and
/// the line, and never repeats what the file said.
pub fn validate_interwiki(text: &str, namespaces: &BTreeSet<String>) -> Result<Interwiki, Fault> {
    parse(text, Some(namespaces))
}

/// [`validate_interwiki`] for the bytes of a file: refused as [`Refusal::NotText`] when they are not UTF-8.
pub fn validate_interwiki_bytes(
    bytes: &[u8],
    namespaces: &BTreeSet<String>,
) -> Result<Interwiki, Fault> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Fault::whole(GovernanceFile::Interwiki, Refusal::NotText))?;
    validate_interwiki(text, namespaces)
}

/// Read the bytes of `governance/interwiki.yaml` as they are in the repository: the rules of [`validate_interwiki`] **except
/// the clash with a namespace**, which a page made after the file was accepted could otherwise turn into a wiki that cannot
/// be written. The fault is the one that makes every write refuse.
pub fn read_interwiki(bytes: &[u8]) -> Result<Interwiki, ReadFault> {
    let file = GovernanceFile::Interwiki;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ReadFault::new(file, Fault::whole(file, Refusal::NotText)))?;
    parse(text, None).map_err(|fault| ReadFault::new(file, fault))
}

/// **Judge the interwiki map of a bundle, one prefix at a time** (RFC 007 D-8: *a template that fails is left out and listed*).
/// `given` is the bundle's map in the order the bundle gave it; the first [`MAX_INTERWIKI_PREFIXES`] that pass are the map, and
/// every prefix that fails, or comes after the bound, is a [`LeftOut`] with the rule it broke. `namespaces` is the wiki's
/// ([`namespaces_of_titles`] over the titles the import writes).
///
/// A prefix is judged by the same functions as a file's, so what acceptance would refuse is what an import leaves out; and what
/// is accepted here is a map [`read_interwiki`] reads back whole.
pub fn judge_interwiki(
    given: impl IntoIterator<Item = (String, String)>,
    namespaces: &BTreeSet<String>,
) -> (Interwiki, Vec<LeftOut>) {
    let mut map = BTreeMap::new();
    let mut left = Vec::new();
    for (prefix, template) in given {
        let rule = prefix_rule(&prefix, Some(namespaces)).or_else(|| template_rule(&template));
        let rule = rule.or_else(|| {
            if map.contains_key(&prefix) {
                Some(Refusal::RepeatedKey)
            } else if map.len() >= MAX_INTERWIKI_PREFIXES {
                Some(Refusal::Bounds)
            } else {
                None
            }
        });
        match rule {
            Some(refusal) => left.push(LeftOut { prefix, refusal }),
            None => {
                map.insert(prefix, template);
            }
        }
    }
    (Interwiki { map }, left)
}
