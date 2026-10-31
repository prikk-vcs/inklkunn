//! A source file: the reader and the canonical writer (RFC 005 part 2, D-2).
//!
//! ```yaml
//! kind: secondary                   # primary | secondary | observational | computational | anecdotal | other
//! availability: unknown             # available | unavailable | unknown: set by a person who checked, never a crawler
//! identity:                         # as written (optional)
//!   title: "…"
//!   author: "…"
//!   date: "…"                       # text: a year, a season, a day
//! reference:                        # where it is found (optional)
//!   type: url                       # url | doi | isbn | page | citation
//!   value: "…"
//! snapshot:                         # a copy kept (optional)
//!   kind: attachment                # attachment: only where the licence allows
//!   attachment: "attachments/…"
//! # or
//!   kind: archive                   # an archive address and the digest of what was archived
//!   address: "…"
//!   digest: "<sha-256>"
//! last_reviewed:                    # when a person last checked it (optional)
//!   date: "2026-10-31"
//!   proposal: "<uuid>"              # the curation proposal that said so (optional)
//! superseded_by: "<uuid>"           # the source that replaces it (optional)
//! ```
//!
//! The source's id is the file's name.

use inklkunn_markup::Value;
use inklkunn_markup::frontmatter::parse_yaml;

use super::claim::{digest, proposal};
use super::model::{
    Availability, Identity, Reference, ReferenceForm, Reviewed, Snapshot, Source, SourceId,
    SourceKind,
};
use super::yaml::{is_day, mapping, optional, quoted, required, text, word};
use super::{Fault, Rule};

const TOP: [&str; 7] = [
    "kind",
    "availability",
    "identity",
    "reference",
    "snapshot",
    "last_reviewed",
    "superseded_by",
];

impl Source {
    /// Read the text of the source `id`'s file. Nothing is read from a file that is not wholly well formed.
    pub fn parse(id: SourceId, source: &str) -> Result<Source, Fault> {
        let value = parse_yaml(source)?;
        let top = mapping(&value, "", &TOP)?;
        let mut out = Source::new(id);
        out.kind = word(
            required(top, "kind")?,
            SourceKind::FIELD,
            SourceKind::from_word,
        )?;
        out.availability = word(
            required(top, "availability")?,
            Availability::FIELD,
            Availability::from_word,
        )?;
        if let Some(v) = optional(top, "identity") {
            let pairs = mapping(v, "identity", &["title", "author", "date"])?;
            let part = |key: &'static str| optional(pairs, key).map(|v| text(v, key)).transpose();
            out.identity = Identity {
                title: part("title")?,
                author: part("author")?,
                date: part("date")?,
            };
        }
        if let Some(v) = optional(top, "reference") {
            let pairs = mapping(v, "reference", &["type", "value"])?;
            let value = text(required(pairs, "value")?, "value")?;
            if value.is_empty() {
                return Err(Fault::at(Rule::Value, "value"));
            }
            out.reference = Some(Reference {
                form: word(
                    required(pairs, "type")?,
                    ReferenceForm::FIELD,
                    ReferenceForm::from_word,
                )?,
                value,
            });
        }
        if let Some(v) = optional(top, "snapshot") {
            out.snapshot = Some(snapshot(v)?);
        }
        if let Some(v) = optional(top, "last_reviewed") {
            let pairs = mapping(v, "last_reviewed", &["date", "proposal"])?;
            let date = text(required(pairs, "date")?, "date")?;
            if !is_day(&date) {
                return Err(Fault::at(Rule::Value, "date"));
            }
            out.last_reviewed = Some(Reviewed {
                date,
                proposal: optional(pairs, "proposal")
                    .map(|v| proposal(v, "proposal"))
                    .transpose()?,
            });
        }
        if let Some(v) = optional(top, "superseded_by") {
            let later = SourceId::parse(&text(v, "superseded_by")?)
                .ok_or(Fault::at(Rule::Id, "superseded_by"))?;
            if later == out.id {
                // A source is not replaced by itself.
                return Err(Fault::at(Rule::Value, "superseded_by"));
            }
            out.superseded_by = Some(later);
        }
        Ok(out)
    }

    /// The canonical text of the source: every key it has, every text double-quoted. Reading it back gives the
    /// source again.
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("kind: {}\n", self.kind.word()));
        out.push_str(&format!("availability: {}\n", self.availability.word()));
        let id = &self.identity;
        if id.title.is_some() || id.author.is_some() || id.date.is_some() {
            out.push_str("identity:\n");
            for (key, value) in [
                ("title", &id.title),
                ("author", &id.author),
                ("date", &id.date),
            ] {
                if let Some(v) = value {
                    out.push_str(&format!("  {key}: {}\n", quoted(v)));
                }
            }
        }
        if let Some(r) = &self.reference {
            out.push_str(&format!(
                "reference:\n  type: {}\n  value: {}\n",
                r.form.word(),
                quoted(&r.value)
            ));
        }
        match &self.snapshot {
            Some(Snapshot::Attachment { path }) => out.push_str(&format!(
                "snapshot:\n  kind: attachment\n  attachment: {}\n",
                quoted(path)
            )),
            Some(Snapshot::Archive { address, digest }) => out.push_str(&format!(
                "snapshot:\n  kind: archive\n  address: {}\n  digest: {}\n",
                quoted(address),
                quoted(digest.as_str())
            )),
            None => {}
        }
        if let Some(r) = &self.last_reviewed {
            out.push_str(&format!("last_reviewed:\n  date: {}\n", quoted(&r.date)));
            if let Some(p) = &r.proposal {
                out.push_str(&format!("  proposal: {}\n", quoted(p)));
            }
        }
        if let Some(later) = &self.superseded_by {
            out.push_str(&format!("superseded_by: {}\n", quoted(later.as_str())));
        }
        out
    }
}

fn snapshot(value: &Value) -> Result<Snapshot, Fault> {
    let pairs = mapping(
        value,
        "snapshot",
        &["kind", "attachment", "address", "digest"],
    )?;
    match text(required(pairs, "kind")?, "kind")?.as_str() {
        "attachment" => {
            if optional(pairs, "address").is_some() || optional(pairs, "digest").is_some() {
                return Err(Fault::at(Rule::Unknown, "snapshot"));
            }
            let path = text(required(pairs, "attachment")?, "attachment")?;
            if !path.starts_with("attachments/")
                || path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(Fault::at(Rule::Value, "attachment"));
            }
            Ok(Snapshot::Attachment { path })
        }
        "archive" => {
            if optional(pairs, "attachment").is_some() {
                return Err(Fault::at(Rule::Unknown, "snapshot"));
            }
            let address = text(required(pairs, "address")?, "address")?;
            if address.is_empty() {
                return Err(Fault::at(Rule::Value, "address"));
            }
            Ok(Snapshot::Archive {
                address,
                digest: digest(required(pairs, "digest")?, "digest")?,
            })
        }
        _ => Err(Fault::at(Rule::Value, "kind")),
    }
}
