//! The records of `history/<page>.jsonl` (wiki-commons XFER-7; RFC 007 D-2, D-3, D-5): each line read **by the schema's types**
//! into a [`Record`], and one page's file judged as a history ([`judge`]).
//!
//! A record's fields are read by type and nothing is interpreted beyond them: a field of the wrong type is a fault that
//! names the field (never its value), and a field this reader does not know is counted by name and never read. Within a
//! file **the file's own order is the history** (D-2): a `parent` that is not the record before it, a `rev` that is not new,
//! or an `at` earlier than the one before it (compared as instants) refuses the import, naming the page and the `rev`.

use std::collections::BTreeSet;

use sha2::{Digest as _, Sha256};

use super::patch;
use super::safe::looks_like_an_address;
use super::time::{Instant, parse};
use crate::import::{Digest, inspect_page};

/// A part of a record that the source may have hidden (`suppressed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    /// The page's text at the revision.
    Content,
    /// Who wrote it.
    Author,
    /// The edit summary.
    Summary,
}

/// Who a record says wrote it: the source's claim, never an account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Author {
    /// `id`, as the source's `users.yaml` has it.
    pub id: Option<String>,
    /// `name`.
    pub name: Option<String>,
    /// `kind`: `person`, `bot`, `group` or `anonymous`.
    pub kind: Option<String>,
    /// `anonymous: true`.
    pub anonymous: bool,
    /// `label`: the public label of an anonymous or temporary account.
    pub label: Option<String>,
}

impl Author {
    /// How the plan counts this author as one of the people the history names: a source id, else a name, else the anonymous label.
    pub fn identity(&self) -> Option<String> {
        self.id
            .clone()
            .or_else(|| self.name.clone())
            .or_else(|| self.label.clone())
    }

    /// The name to show for the author, and what kind of person it is.
    pub fn display(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.label.clone())
            .or_else(|| self.id.clone())
    }

    fn mentions_an_address(&self) -> bool {
        [&self.id, &self.name, &self.label]
            .into_iter()
            .flatten()
            .any(|s| looks_like_an_address(s))
    }
}

/// One line of a history file, by the schema's types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// `rev`: unique within the file.
    pub rev: String,
    /// `parent`: the record before it, or none for the first.
    pub parent: Option<String>,
    /// `type`; `edit` when absent.
    pub kind: String,
    /// `at`, as an instant.
    pub at: Option<Instant>,
    /// `author`.
    pub author: Option<Author>,
    /// `summary`.
    pub summary: Option<String>,
    /// `minor`.
    pub minor: bool,
    /// `automated`.
    pub automated: bool,
    /// `content`: the page's complete file at the revision.
    pub content: Option<String>,
    /// `patch`: a unified diff from the previous revision's text.
    pub patch: Option<String>,
    /// `sha256`.
    pub sha256: Option<Digest>,
    /// `from` and `to` of a rename: titles.
    pub from: Option<String>,
    /// See [`Record::from`].
    pub to: Option<String>,
    /// `suppressed`.
    pub suppressed: Vec<Part>,
    /// `license`.
    pub license: Option<String>,
    /// `inklkunn.withheld`: why the exporting inklkunn gave no text (RFC 007 D-7; the I-T1 review's X-1). **Not** moderation's word.
    pub withheld: Option<String>,
    /// `inklkunn.path`: the page's path after this revision, when the exporting inklkunn says it.
    pub path: Option<String>,
    /// The members this reader does not know, as written (never interpreted).
    pub unknown: Vec<String>,
}

/// A fault in a history file: where (1-based line; 0 for the file) and the rule. **Never a value**, and not the record's `rev`
/// either: a fault of types and shapes says the line, and only the rules of order and of digests name a `rev` (D-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// The line.
    pub line: usize,
    /// The rule.
    pub rule: String,
}

impl Fault {
    /// How a refusal says where: the file's page is named by the caller; this is `line N`.
    pub fn place(&self) -> String {
        if self.line == 0 {
            "the file".to_owned()
        } else {
            format!("line {}", self.line)
        }
    }
}

type Object = serde_json::Map<String, serde_json::Value>;

/// The members of `obj` named `key`, read as text.
fn string(obj: &Object, key: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{key}` is not text")),
    }
}

fn boolean(obj: &Object, key: &str) -> Result<Option<bool>, String> {
    match obj.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("`{key}` is not true or false")),
    }
}

fn author(value: Option<&serde_json::Value>, key: &str) -> Result<Option<Author>, String> {
    let obj = match value {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::Object(o)) => o,
        Some(_) => return Err(format!("`{key}` is not an object")),
    };
    let field = |name: &str| string(obj, name).map_err(|e| format!("`{key}`: {e}"));
    let kind = field("kind")?;
    if let Some(k) = &kind
        && !matches!(k.as_str(), "person" | "bot" | "group" | "anonymous")
    {
        return Err(format!(
            "`{key}.kind` is not person, bot, group or anonymous"
        ));
    }
    field("url")?;
    Ok(Some(Author {
        id: field("id")?,
        name: field("name")?,
        kind,
        anonymous: boolean(obj, "anonymous")
            .map_err(|e| format!("`{key}`: {e}"))?
            .unwrap_or(false),
        label: field("label")?,
    }))
}

/// The members of a record this reader reads. Anything else is counted by name.
const KNOWN: &[&str] = &[
    "rev",
    "parent",
    "type",
    "at",
    "author",
    "contributors",
    "summary",
    "minor",
    "automated",
    "content",
    "patch",
    "sha256",
    "size",
    "from",
    "to",
    "reverts",
    "restores",
    "suppressed",
    "reason",
    "suppressed_by",
    "review",
    "assistance",
    "license",
    "inklkunn",
];

/// One record, from one parsed line.
fn record_of(value: &serde_json::Value) -> Result<Record, String> {
    let serde_json::Value::Object(obj) = value else {
        return Err("is not a JSON object".to_owned());
    };
    let mut r = Record::default();
    match obj.get("rev") {
        Some(serde_json::Value::String(s)) if !s.is_empty() && s.chars().count() <= 256 => {
            // A `rev` is recorded in the change's message as a trailer: one line, no space at either end.
            if s.chars().any(inklkunn_store::record::is_line_or_control) || s.trim() != s {
                return Err(
                    "has a `rev` that is not one line (no control character, no space at either end)"
                        .to_owned(),
                );
            }
            r.rev = s.clone();
        }
        _ => return Err("has no `rev` of text from 1 to 256 characters".to_owned()),
    }
    r.parent = string(obj, "parent")?;
    r.kind = string(obj, "type")?.unwrap_or_else(|| "edit".to_owned());
    r.at = match obj.get("at") {
        Some(serde_json::Value::String(s)) => {
            Some(parse(s).ok_or("`at` is not an RFC 3339 date-time")?)
        }
        _ => return Err("has no `at` of RFC 3339 text".to_owned()),
    };
    r.author = author(obj.get("author"), "author")?;
    if let Some(serde_json::Value::Array(list)) = obj.get("contributors") {
        for item in list {
            author(Some(item), "contributors")?;
        }
    } else if obj.get("contributors").is_some_and(|v| !v.is_null()) {
        return Err("`contributors` is not a list".to_owned());
    }
    author(obj.get("suppressed_by"), "suppressed_by")?;
    r.summary = string(obj, "summary")?;
    r.minor = boolean(obj, "minor")?.unwrap_or(false);
    r.automated = boolean(obj, "automated")?.unwrap_or(false);
    r.content = string(obj, "content")?;
    r.patch = string(obj, "patch")?;
    r.from = string(obj, "from")?;
    r.to = string(obj, "to")?;
    string(obj, "reverts")?;
    string(obj, "restores")?;
    string(obj, "reason")?;
    r.license = string(obj, "license")?;
    match obj.get("size") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Number(n)) if n.as_u64().is_some() => {}
        Some(_) => return Err("`size` is not a whole number".to_owned()),
    }
    for key in ["review", "assistance"] {
        match obj.get(key) {
            None | Some(serde_json::Value::Null | serde_json::Value::Object(_)) => {}
            Some(_) => return Err(format!("`{key}` is not an object")),
        }
    }
    if let Some(text) = string(obj, "sha256")? {
        r.sha256 = Some(
            super::manifest::digest_from_hex(&text)
                .ok_or("`sha256` is not 64 lower-case hexadecimal digits")?,
        );
    }
    match obj.get("suppressed") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                let part = match item.as_str() {
                    Some("content") => Part::Content,
                    Some("author") => Part::Author,
                    Some("summary") => Part::Summary,
                    _ => {
                        return Err(
                            "`suppressed` names something but content, author or summary"
                                .to_owned(),
                        );
                    }
                };
                if !r.suppressed.contains(&part) {
                    r.suppressed.push(part);
                }
            }
        }
        Some(_) => return Err("`suppressed` is not a list".to_owned()),
    }
    // inklkunn's own facts, under one key (D-7): the ones the plan reads.
    match obj.get("inklkunn") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Object(own)) => {
            r.withheld = match own.get("withheld") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                // The reason is a word; any other shape still says the text was withheld.
                Some(_) => Some(String::new()),
            };
            r.path = string(own, "path").ok().flatten();
        }
        Some(_) => return Err("`inklkunn` is not an object".to_owned()),
    }
    r.unknown = obj
        .keys()
        .filter(|k| !KNOWN.contains(&k.as_str()))
        .cloned()
        .collect();
    Ok(r)
}

/// Read the lines of one history file (UTF-8 JSON Lines, one record a line, no blank line). The first fault stops the file.
pub fn read(bytes: &[u8]) -> Result<Vec<Record>, Fault> {
    let at_line = |line: usize, rule: &str| Fault {
        line,
        rule: rule.to_owned(),
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(at_line(0, "is not UTF-8 text"));
    };
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut out = Vec::new();
    if body.is_empty() {
        return Ok(out);
    }
    for (n, line) in body.split('\n').enumerate() {
        let at = n + 1;
        if line.is_empty() {
            return Err(at_line(at, "is empty"));
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|_| at_line(at, "is not JSON"))?;
        out.push(record_of(&value).map_err(|rule| Fault { line: at, rule })?);
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------------------- one page

/// What a record does when the import replays it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Writes the page's text (`create`, `edit`, `restore`, `revert`): the digest and size of the text.
    Text {
        /// SHA-256 of the text.
        sha256: Digest,
        /// Its size in bytes.
        bytes: u64,
    },
    /// Moves the page (`rename`): the titles it names.
    Rename {
        /// `from`.
        from: String,
        /// `to`.
        to: String,
    },
    /// Removes the page (`delete`).
    Delete,
    /// Is not replayed.
    NotReplayed(Skipped),
}

/// Why a record is not replayed (counted in the report, by reason).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Skipped {
    /// The source hid the text (`suppressed: [content]`): there is no text to replay (D-3).
    SuppressedContent,
    /// The exporting inklkunn withheld the text (`inklkunn.withheld`), with its reason.
    Withheld(String),
    /// A text-bearing record with no text and no patch.
    NoText,
    /// A record of a type that is the source's act, not a page's text: `upload`, `protect`, `merge`, `fork`, `review`, or the
    /// engine's own (D-2), by the type as written.
    Type(String),
}

/// A record, judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// `rev`.
    pub rev: String,
    /// `at`.
    pub at: Instant,
    /// What the replay does with it.
    pub step: Step,
    /// The author as the source says it, or `None` when the source hid it (or says nothing).
    pub author: Option<Author>,
    /// The summary, or `None` when the source hid it (or says nothing).
    pub summary: Option<String>,
    /// The parts the source hid, to be said *hidden at the source*.
    pub hidden: Vec<Part>,
    /// `minor`.
    pub minor: bool,
    /// `automated`.
    pub automated: bool,
    /// The page's path after this revision, as far as the bundle says it.
    pub path: Option<String>,
}

/// What the plan needs from one page's file besides its revisions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Judged {
    /// The revisions, in the file's order.
    pub revisions: Vec<Revision>,
    /// The licences that revisions' texts or records declare (frontmatter `license`, or the record's own), each with the first
    /// `rev` that declares it: a revision under a licence the wiki cannot take is a reason to leave the page out.
    pub licences: Vec<(String, String)>,
    /// The text after the last record that gave one, when it is known (what the page's final file is compared with).
    pub last_text: Option<Digest>,
    /// Unknown members of records, by name (never interpreted).
    pub unknown_members: BTreeSet<String>,
    /// Whether any record names an e-mail address as an author.
    pub address_in_author: Option<String>,
}

/// What a page's text is, as the replay's next record sees it.
enum Text {
    /// Nothing yet.
    Nothing,
    /// This text.
    Known(String),
    /// A text that exists and that the plan has not got (hidden, withheld, or absent from its record).
    Unknown,
    /// The page was removed.
    Gone,
}

/// What one record did to a page's text, as [`Walker::step`] says it.
pub(super) struct Stepped {
    /// What the replay does with the record.
    pub(super) step: Step,
    /// The page's complete text after the record, when it is a text step: the caller holds it only as long as it needs it.
    pub(super) text: Option<String>,
    /// The licence the text's frontmatter declares, when it does.
    pub(super) licence: Option<String>,
}

/// One page's text as its records go by, in the file's order: **the one place that says what a record does to it**, so that the
/// plan that judges and the replay that writes cannot differ. `content` is the text; a `patch` is applied to the text before it and
/// its `sha256` must be the result's.
pub(super) struct Walker {
    text: Text,
}

impl Walker {
    pub(super) fn new() -> Self {
        Walker {
            text: Text::Nothing,
        }
    }

    /// The digest of the text after the last record that gave one, when the walk has it.
    pub(super) fn last_digest(&self) -> Option<Digest> {
        match &self.text {
            Text::Known(t) => Some(Sha256::digest(t.as_bytes()).into()),
            _ => None,
        }
    }

    /// Take the next record. `Err` is a rule (never a value) that the record breaks.
    pub(super) fn step(&mut self, r: &Record, page_cap: u64) -> Result<Stepped, String> {
        let hides_content = r.suppressed.contains(&Part::Content);
        let withheld = r.withheld.is_some();
        let mut licence = None;
        let mut text_out = None;
        let step = match r.kind.as_str() {
            _ if hides_content => {
                self.text = Text::Unknown;
                Step::NotReplayed(Skipped::SuppressedContent)
            }
            _ if withheld => {
                self.text = Text::Unknown;
                Step::NotReplayed(Skipped::Withheld(r.withheld.clone().unwrap_or_default()))
            }
            "rename" => match (&r.from, &r.to) {
                (Some(from), Some(to)) if !from.is_empty() && !to.is_empty() => Step::Rename {
                    from: from.clone(),
                    to: to.clone(),
                },
                _ => return Err("is a rename without a `from` and a `to`".to_owned()),
            },
            "delete" => {
                self.text = Text::Gone;
                Step::Delete
            }
            "create" | "edit" | "restore" | "revert" => {
                let next = match (&r.content, &r.patch) {
                    (Some(content), _) => Some(content.clone()),
                    (None, Some(patch)) => {
                        if r.sha256.is_none() {
                            return Err(
                                "has a `patch` and no `sha256` to check the result against"
                                    .to_owned(),
                            );
                        }
                        let Text::Known(old) = &self.text else {
                            return Err(
                                "has a `patch` and no text before it to apply it to".to_owned()
                            );
                        };
                        Some(
                            patch::apply(old, patch)
                                .map_err(|rule| format!("has a `patch` that {rule}"))?,
                        )
                    }
                    (None, None) => None,
                };
                match next {
                    None => {
                        self.text = Text::Unknown;
                        Step::NotReplayed(Skipped::NoText)
                    }
                    Some(next) => {
                        let bytes = next.len() as u64;
                        if bytes > page_cap {
                            return Err(format!(
                                "has a text larger than the {} KiB cap of a page",
                                page_cap >> 10
                            ));
                        }
                        let sha256: Digest = Sha256::digest(next.as_bytes()).into();
                        if let Some(declared) = r.sha256
                            && declared != sha256
                        {
                            return Err("has a `sha256` that is not the text's".to_owned());
                        }
                        licence = inspect_page_text(&next)?;
                        text_out = Some(next.clone());
                        self.text = Text::Known(next);
                        Step::Text { sha256, bytes }
                    }
                }
            }
            other => Step::NotReplayed(Skipped::Type(other.to_owned())),
        };
        Ok(Stepped {
            step,
            text: text_out,
            licence,
        })
    }
}

/// Judge one page's records as a history: the order, each text, each digest. `Err` is a refusal as `(rev, rule)`: the page is named
/// by the caller, the rule never repeats a value.
pub fn judge(records: &[Record], page_cap: u64) -> Result<Judged, (Option<String>, String)> {
    let mut judged = Judged::default();
    let mut walker = Walker::new();
    let mut before: Option<&Record> = None;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for r in records {
        let refuse = |rule: &str| (Some(r.rev.clone()), rule.to_owned());
        let at = r.at.ok_or_else(|| refuse("has no `at`"))?;
        // The change's message writes the time as a date of four digits of year, and year 0 has none.
        if at.secs < -62_135_596_800 {
            return Err(refuse("has an `at` before the year 1"));
        }
        if !seen.insert(r.rev.as_str()) {
            return Err(refuse("is used by an earlier record of the page"));
        }
        match before {
            None if r.parent.is_some() => {
                return Err(refuse(
                    "is the first record of the page and names a `parent`",
                ));
            }
            Some(b) if r.parent.as_deref() != Some(b.rev.as_str()) => {
                return Err(refuse(
                    "has a `parent` that is not the record before it in the file",
                ));
            }
            _ => {}
        }
        if let Some(b) = before
            && let Some(was) = b.at
            && at < was
        {
            return Err(refuse("is earlier than the record before it in the file"));
        }
        before = Some(r);
        judged.unknown_members.extend(r.unknown.iter().cloned());
        if judged.address_in_author.is_none()
            && r.author.as_ref().is_some_and(Author::mentions_an_address)
        {
            judged.address_in_author = Some(r.rev.clone());
        }
        let hides_content = r.suppressed.contains(&Part::Content);
        let withheld = r.withheld.is_some();
        if (hides_content || withheld) && (r.content.is_some() || r.patch.is_some()) {
            return Err(refuse(
                "says its text is hidden or withheld and carries text (which cannot be told from what was hidden)",
            ));
        }
        let hidden: Vec<Part> = r
            .suppressed
            .iter()
            .copied()
            .filter(|p| *p != Part::Content)
            .collect();
        let stepped = walker.step(r, page_cap).map_err(|rule| refuse(&rule))?;
        if let Some(licence) = stepped.licence {
            note_licence(&mut judged, licence, &r.rev);
        }
        let step = stepped.step;
        if let Some(licence) = &r.license {
            note_licence(&mut judged, licence.clone(), &r.rev);
        }
        let hides_author = r.suppressed.contains(&Part::Author);
        let hides_summary = r.suppressed.contains(&Part::Summary);
        judged.revisions.push(Revision {
            rev: r.rev.clone(),
            at,
            step,
            author: if hides_author { None } else { r.author.clone() },
            summary: if hides_summary {
                None
            } else {
                r.summary.clone()
            },
            hidden,
            minor: r.minor,
            automated: r.automated,
            path: r.path.clone(),
        });
    }
    judged.last_text = walker.last_digest();
    Ok(judged)
}

fn note_licence(judged: &mut Judged, licence: String, rev: &str) {
    if !judged.licences.iter().any(|(l, _)| *l == licence) {
        judged.licences.push((licence, rev.to_owned()));
    }
}

/// What a revision's text must be, as the folder import judges a page: UTF-8 (it is a JSON string already) whose frontmatter the
/// parser accepts. The licence its frontmatter declares, if any.
fn inspect_page_text(text: &str) -> Result<Option<String>, String> {
    inspect_page(text.as_bytes()).map_err(|why| format!("has a text that {why}"))?;
    Ok(inklkunn_markup::parse_metadata(text)
        .ok()
        .and_then(|m| m.license))
}
