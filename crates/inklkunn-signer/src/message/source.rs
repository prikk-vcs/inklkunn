//! What an imported change says about the revision it came from (RFC 007 D-3; handoff I-T3): **the source's facts, as typed
//! trailers, never as a signature.**
//!
//! A change that replays one revision of a bundle is the wiki's own (`Kind: import`, the system principal, the wiki's keys).
//! Its message carries what the *source* said about the revision, so that anyone holding the repository reads it with `prikk
//! log` and a page can say it in words:
//!
//! ```text
//! Imported-Rev: <the source's id of the revision, one line, 1 to 256 characters>
//! Imported-At: <RFC 3339 in UTC to the second: 2009-02-13T23:31:30Z>
//! Imported-Author: <person | bot | group | anonymous | unspecified> (<the name the source gives>)
//! Imported-Summary: <the source's summary, one line>
//! Imported-Hidden: <author | summary | author, summary>        (what the source hid)
//! Imported-Minor: true
//! Imported-Automated: true
//! ```
//!
//! They follow `Imported-From`, once each and in this order. `Imported-Rev` and `Imported-At` always come together; the rest
//! are present only when the source said so (a name or a summary the source hid is *hidden*, never an empty value, and
//! `minor` and `automated` are written only when true). **The author's name is a name and nothing else**: it is never an
//! account's id, and nothing reads it as one.
//!
//! Like the rest of the message, the values never echo in an error. A name and a summary come from a person at the source, so
//! they are made one line and bounded ([`SourceRevision::with_author`], [`SourceRevision::with_summary`] clean them, and say
//! when they had to cut); a `rev` is an identifier and is refused when it is not one line.

use std::fmt;

use inklkunn_store::record::{is_line_or_control, rfc3339};

use super::MessageError;

/// The longest `rev`, in characters (the history record's own bound).
pub const SOURCE_REV_MAX_CHARS: usize = 256;
/// The longest author name, in characters.
pub const SOURCE_NAME_MAX_CHARS: usize = 200;
/// The longest summary, in characters.
pub const SOURCE_SUMMARY_MAX_CHARS: usize = 500;
/// The mark of a text that was cut.
const ELLIPSIS: char = '…';

/// What kind of author the source says wrote the revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceAuthorKind {
    /// A person.
    Person,
    /// A program.
    Bot,
    /// A group.
    Group,
    /// Nobody who is named (an address replaced by the converter, a visitor).
    Anonymous,
    /// The source says nothing about the kind.
    Unspecified,
}

impl SourceAuthorKind {
    /// The trailer's word.
    pub const fn as_str(self) -> &'static str {
        match self {
            SourceAuthorKind::Person => "person",
            SourceAuthorKind::Bot => "bot",
            SourceAuthorKind::Group => "group",
            SourceAuthorKind::Anonymous => "anonymous",
            SourceAuthorKind::Unspecified => "unspecified",
        }
    }

    /// The kind a word names.
    pub fn parse(word: &str) -> Option<Self> {
        [
            SourceAuthorKind::Person,
            SourceAuthorKind::Bot,
            SourceAuthorKind::Group,
            SourceAuthorKind::Anonymous,
            SourceAuthorKind::Unspecified,
        ]
        .into_iter()
        .find(|k| k.as_str() == word)
    }
}

/// The author a source names: a name, as text, and a kind. **Not an account**: nothing here is an id of this service.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceAuthor {
    kind: SourceAuthorKind,
    name: String,
}

impl SourceAuthor {
    /// The kind the source gave.
    pub fn kind(&self) -> SourceAuthorKind {
        self.kind
    }

    /// The name, as the source gave it (made one line and bounded).
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The facts of one imported revision (see the module). Compose with [`SourceRevision::new`] and the `with_` methods; a message
/// holds one only when its kind is `import`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceRevision {
    rev: String,
    at: i64,
    author: Option<SourceAuthor>,
    summary: Option<String>,
    hidden_author: bool,
    hidden_summary: bool,
    minor: bool,
    automated: bool,
}

/// `text` as one line of at most `max` characters: control characters and line breaks become spaces, the ends are trimmed, and a
/// longer text is cut with a closing `…`. `None` when nothing is left. **Lossy on purpose**: the text is a person's at the source.
fn one_line(text: &str, max: usize) -> Option<String> {
    let spaced: String = text
        .chars()
        .map(|c| if is_line_or_control(c) { ' ' } else { c })
        .collect();
    let trimmed = spaced.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= max {
        return Some(trimmed.to_owned());
    }
    let mut cut: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    cut.push(ELLIPSIS);
    Some(cut)
}

impl SourceRevision {
    /// The revision `rev` of the source, made at `at` (Unix seconds, UTC). `rev` must be one line of 1 to
    /// [`SOURCE_REV_MAX_CHARS`] characters with no space at either end.
    pub fn new(rev: &str, at: i64) -> Result<Self, MessageError> {
        if rev.is_empty() || rev.chars().any(is_line_or_control) || rev.trim() != rev {
            return Err(MessageError::NotOneLine("Imported-Rev"));
        }
        if rev.chars().count() > SOURCE_REV_MAX_CHARS {
            return Err(MessageError::Invalid {
                field: "Imported-Rev",
                rule: "at most 256 characters",
            });
        }
        // The text of a date this message can write and read back: four digits of year.
        if !(-62_135_596_800..=253_402_300_799).contains(&at) {
            return Err(MessageError::Invalid {
                field: "Imported-At",
                rule: "a time between the years 0001 and 9999",
            });
        }
        Ok(SourceRevision {
            rev: rev.to_owned(),
            at,
            author: None,
            summary: None,
            hidden_author: false,
            hidden_summary: false,
            minor: false,
            automated: false,
        })
    }

    /// The author the source names, with the name made one line and bounded (a name with nothing left is no author).
    #[must_use]
    pub fn with_author(mut self, kind: SourceAuthorKind, name: &str) -> Self {
        self.hidden_author = false;
        self.author = one_line(name, SOURCE_NAME_MAX_CHARS).map(|name| SourceAuthor { kind, name });
        self
    }

    /// The summary the source gave, made one line and bounded (nothing left is no summary).
    #[must_use]
    pub fn with_summary(mut self, summary: &str) -> Self {
        self.hidden_summary = false;
        self.summary = one_line(summary, SOURCE_SUMMARY_MAX_CHARS);
        self
    }

    /// The source hid the author: there is none to name, and the message says so.
    #[must_use]
    pub fn with_author_hidden(mut self) -> Self {
        self.author = None;
        self.hidden_author = true;
        self
    }

    /// The source hid the summary.
    #[must_use]
    pub fn with_summary_hidden(mut self) -> Self {
        self.summary = None;
        self.hidden_summary = true;
        self
    }

    /// The source flagged the revision minor.
    #[must_use]
    pub fn minor(mut self, minor: bool) -> Self {
        self.minor = minor;
        self
    }

    /// The source flagged the revision automated.
    #[must_use]
    pub fn automated(mut self, automated: bool) -> Self {
        self.automated = automated;
        self
    }

    /// The source's id of the revision.
    pub fn rev(&self) -> &str {
        &self.rev
    }

    /// When the source says it was made: Unix seconds, UTC.
    pub fn at(&self) -> i64 {
        self.at
    }

    /// The author the source names.
    pub fn author(&self) -> Option<&SourceAuthor> {
        self.author.as_ref()
    }

    /// The summary the source gave.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Did the source hide the author?
    pub fn author_hidden(&self) -> bool {
        self.hidden_author
    }

    /// Did the source hide the summary?
    pub fn summary_hidden(&self) -> bool {
        self.hidden_summary
    }

    /// Flagged minor at the source.
    pub fn is_minor(&self) -> bool {
        self.minor
    }

    /// Flagged automated at the source.
    pub fn is_automated(&self) -> bool {
        self.automated
    }

    /// The trailer lines, `(name, value)` in the order the message writes them.
    pub(super) fn trailers(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![
            (super::ORDER[super::IMPORTED_REV], self.rev.clone()),
            (super::ORDER[super::IMPORTED_AT], rfc3339(self.at)),
        ];
        if let Some(a) = &self.author {
            out.push((
                super::ORDER[super::IMPORTED_AUTHOR],
                format!("{} ({})", a.kind.as_str(), a.name),
            ));
        }
        if let Some(s) = &self.summary {
            out.push((super::ORDER[super::IMPORTED_SUMMARY], s.clone()));
        }
        match (self.hidden_author, self.hidden_summary) {
            (true, true) => out.push((
                super::ORDER[super::IMPORTED_HIDDEN],
                "author, summary".to_owned(),
            )),
            (true, false) => out.push((super::ORDER[super::IMPORTED_HIDDEN], "author".to_owned())),
            (false, true) => out.push((super::ORDER[super::IMPORTED_HIDDEN], "summary".to_owned())),
            (false, false) => {}
        }
        if self.minor {
            out.push((super::ORDER[super::IMPORTED_MINOR], "true".to_owned()));
        }
        if self.automated {
            out.push((super::ORDER[super::IMPORTED_AUTOMATED], "true".to_owned()));
        }
        out
    }

    /// Read the trailers' values (those a message had, in the slots of [`super::ORDER`] from `Imported-Rev` on), strictly: only
    /// the canonical text [`SourceRevision::trailers`] writes.
    pub(super) fn parse(values: &[Option<&str>]) -> Result<Option<Self>, MessageError> {
        let get = |i: usize| values.get(i - super::IMPORTED_REV).copied().flatten();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let rev = get(super::IMPORTED_REV).ok_or(MessageError::Missing("Imported-Rev"))?;
        let at = get(super::IMPORTED_AT).ok_or(MessageError::Missing("Imported-At"))?;
        let at = parse_at(at)?;
        let mut out = SourceRevision::new(rev, at)?;
        if let Some(value) = get(super::IMPORTED_AUTHOR) {
            let bad = |rule: &'static str| MessageError::Invalid {
                field: "Imported-Author",
                rule,
            };
            let (head, rest) = value
                .split_once(" (")
                .ok_or_else(|| bad("`<kind> (<name>)`"))?;
            let name = rest
                .strip_suffix(')')
                .ok_or_else(|| bad("the parenthesis around the name is closed"))?;
            let kind = SourceAuthorKind::parse(head)
                .ok_or_else(|| bad("person, bot, group, anonymous or unspecified"))?;
            // Only a name that is already one line and bounded is canonical: what was written is what is read back.
            let clean = one_line(name, SOURCE_NAME_MAX_CHARS).filter(|c| c == name);
            if clean.is_none() {
                return Err(bad("a name of one line, within its bound"));
            }
            out.author = Some(SourceAuthor {
                kind,
                name: name.to_owned(),
            });
        }
        if let Some(value) = get(super::IMPORTED_SUMMARY) {
            if one_line(value, SOURCE_SUMMARY_MAX_CHARS).as_deref() != Some(value) {
                return Err(MessageError::Invalid {
                    field: "Imported-Summary",
                    rule: "a summary of one line, within its bound",
                });
            }
            out.summary = Some(value.to_owned());
        }
        if let Some(value) = get(super::IMPORTED_HIDDEN) {
            let (author, summary) = match value {
                "author" => (true, false),
                "summary" => (false, true),
                "author, summary" => (true, true),
                _ => {
                    return Err(MessageError::Invalid {
                        field: "Imported-Hidden",
                        rule: "author, summary or author, summary",
                    });
                }
            };
            if (author && out.author.is_some()) || (summary && out.summary.is_some()) {
                return Err(MessageError::Invalid {
                    field: "Imported-Hidden",
                    rule: "a part that is hidden is not also given",
                });
            }
            out.hidden_author = author;
            out.hidden_summary = summary;
        }
        for (slot, field) in [
            (super::IMPORTED_MINOR, "Imported-Minor"),
            (super::IMPORTED_AUTOMATED, "Imported-Automated"),
        ] {
            if let Some(value) = get(slot) {
                if value != "true" {
                    return Err(MessageError::Invalid {
                        field,
                        rule: "true (it is written only when true)",
                    });
                }
                if slot == super::IMPORTED_MINOR {
                    out.minor = true;
                } else {
                    out.automated = true;
                }
            }
        }
        Ok(Some(out))
    }
}

impl fmt::Display for SourceAuthorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unix seconds from `YYYY-MM-DDTHH:MM:SSZ`, and only from that exact form (what [`rfc3339`] writes).
fn parse_at(text: &str) -> Result<i64, MessageError> {
    let bad = || MessageError::Invalid {
        field: "Imported-At",
        rule: "RFC 3339 in UTC to the second, like 1999-12-31T23:59:59Z",
    };
    let digits = |s: &str| -> Option<i64> {
        (s.bytes().all(|b| b.is_ascii_digit()) && !s.is_empty()).then(|| s.parse().ok())?
    };
    let b = text.as_bytes();
    if b.len() != 20
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
        || b.get(19) != Some(&b'Z')
    {
        return Err(bad());
    }
    let part = |from: usize, to: usize| text.get(from..to).and_then(digits);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        part(0, 4),
        part(5, 7),
        part(8, 10),
        part(11, 13),
        part(14, 16),
        part(17, 19),
    ) else {
        return Err(bad());
    };
    // Howard Hinnant's days-from-civil, then the inverse check: only a date that writes itself back is a date.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
    if year == 0 || rfc3339(secs) != text {
        return Err(bad());
    }
    Ok(secs)
}

#[cfg(test)]
mod tests;
