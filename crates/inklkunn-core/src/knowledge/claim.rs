//! A claim file: the reader and the canonical writer (RFC 005 part 2, D-1).
//!
//! ```yaml
//! kind: observation                 # observation | interpretation | hypothesis | opinion | decision | policy | question | unknown
//! lifecycle: proposed               # proposed | adopted | retired | unknown
//! community_status:
//!   status: unknown                 # accepted | contested | unresolved | unknown
//!   proposal: "<uuid>"              # the accepted curation proposal that decided it (optional)
//!   judged_on: "<sha-256>"          # the wording it was decided on (optional)
//! expressions:                      # where the claim is said
//!   - page_id: "<the page's id>"
//!     block: "<the block's ^id>"
//! relations:                        # to other claims
//!   - type: refines                 # refines | supersedes | contradicts | split-from | merged-into
//!     claim: "<uuid>"
//! sources:                          # to source files
//!   - source: "<uuid>"
//!     relation: supports            # supports | challenges
//!     bearing: direct               # direct | partial | indirect: how directly, never how true
//!     note: "…"                     # optional
//!     judged_on: "<sha-256>"        # the wording the relation was judged on
//! ```
//!
//! The claim's id is the file's name. The three lists are kept in one order (expressions by anchor, relations by
//! type and claim, sources by source id), so a claim has one text: the reader sorts, the writer writes sorted.

use inklkunn_markup::Value;
use inklkunn_markup::frontmatter::parse_yaml;

use super::model::{
    Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Lifecycle, Link, Relation,
    RelationType, SourceId, SourceLink, Standing, StatementKind, block_id_ok, is_uuid_shape,
    page_id_ok,
};
use super::yaml::{
    MAX_EXPRESSIONS, MAX_LINKS, list, mapping, optional, quoted, required, text, word,
};
use super::{Fault, Rule};

const TOP: [&str; 6] = [
    "kind",
    "lifecycle",
    "community_status",
    "expressions",
    "relations",
    "sources",
];

/// A digest key's value.
pub(super) fn digest(value: &Value, field: &'static str) -> Result<Digest, Fault> {
    Digest::parse(&text(value, field)?).ok_or(Fault::at(Rule::Value, field))
}

/// A proposal id's value.
pub(super) fn proposal(value: &Value, field: &'static str) -> Result<String, Fault> {
    let id = text(value, field)?;
    if is_uuid_shape(&id) {
        Ok(id)
    } else {
        Err(Fault::at(Rule::Id, field))
    }
}

impl Claim {
    /// Read the text of the claim `id`'s file. Nothing is read from a file that is not wholly well formed.
    pub fn parse(id: ClaimId, source: &str) -> Result<Claim, Fault> {
        let value = parse_yaml(source)?;
        let top = mapping(&value, "", &TOP)?;
        let kind = word(
            required(top, "kind")?,
            StatementKind::FIELD,
            StatementKind::from_word,
        )?;
        let lifecycle = word(
            required(top, "lifecycle")?,
            Lifecycle::FIELD,
            Lifecycle::from_word,
        )?;
        let community_status = community_status(required(top, "community_status")?)?;

        let mut expressions = Vec::new();
        for item in list(optional(top, "expressions"), "expressions", MAX_EXPRESSIONS)? {
            let pairs = mapping(item, "expressions", &["page_id", "block"])?;
            let page_id = text(required(pairs, "page_id")?, "page_id")?;
            let block = text(required(pairs, "block")?, "block")?;
            if !page_id_ok(&page_id) || !block_id_ok(&block) {
                return Err(Fault::at(Rule::Id, "expressions"));
            }
            expressions.push(Expression { page_id, block });
        }

        let mut relations = Vec::new();
        for item in list(optional(top, "relations"), "relations", MAX_LINKS)? {
            let pairs = mapping(item, "relations", &["type", "claim"])?;
            let kind = word(
                required(pairs, "type")?,
                RelationType::FIELD,
                RelationType::from_word,
            )?;
            let claim = ClaimId::parse(&text(required(pairs, "claim")?, "claim")?)
                .ok_or(Fault::at(Rule::Id, "relations"))?;
            if claim == id {
                // A claim does not relate to itself.
                return Err(Fault::at(Rule::Value, "relations"));
            }
            relations.push(Relation { kind, claim });
        }

        let mut sources = Vec::new();
        for item in list(optional(top, "sources"), "sources", MAX_LINKS)? {
            let pairs = mapping(
                item,
                "sources",
                &["source", "relation", "bearing", "note", "judged_on"],
            )?;
            sources.push(SourceLink {
                source: SourceId::parse(&text(required(pairs, "source")?, "source")?)
                    .ok_or(Fault::at(Rule::Id, "sources"))?,
                relation: word(required(pairs, "relation")?, Link::FIELD, Link::from_word)?,
                bearing: word(
                    required(pairs, "bearing")?,
                    Bearing::FIELD,
                    Bearing::from_word,
                )?,
                note: optional(pairs, "note")
                    .map(|v| text(v, "note"))
                    .transpose()?,
                judged_on: digest(required(pairs, "judged_on")?, "judged_on")?,
            });
        }

        let mut claim = Claim {
            id,
            kind,
            expressions,
            relations,
            lifecycle,
            community_status,
            sources,
        };
        claim.normalize();
        // The lists name their entries once: a second one would be a second answer to the same question, and the
        // merge matches entries by these keys.
        if has_repeat(&claim.expressions) {
            return Err(Fault::at(Rule::Duplicate, "expressions"));
        }
        if has_repeat(&claim.relations) {
            return Err(Fault::at(Rule::Duplicate, "relations"));
        }
        if claim
            .sources
            .windows(2)
            .any(|w| w.first().map(|a| &a.source) == w.get(1).map(|b| &b.source))
        {
            return Err(Fault::at(Rule::Duplicate, "sources"));
        }
        Ok(claim)
    }

    /// Put the three lists in the one order a claim's text has.
    pub fn normalize(&mut self) {
        self.expressions.sort();
        self.relations.sort();
        self.sources.sort_by(|a, b| a.source.cmp(&b.source));
    }

    /// The canonical text of the claim: every key, the lists sorted, every text double-quoted. Reading it back
    /// gives the claim again.
    pub fn to_yaml(&self) -> String {
        let mut claim = self.clone();
        claim.normalize();
        let mut out = String::new();
        out.push_str(&format!("kind: {}\n", claim.kind.word()));
        out.push_str(&format!("lifecycle: {}\n", claim.lifecycle.word()));
        out.push_str("community_status:\n");
        let status = &claim.community_status;
        out.push_str(&format!("  status: {}\n", status.status.word()));
        if let Some(p) = &status.proposal {
            out.push_str(&format!("  proposal: {}\n", quoted(p)));
        }
        if let Some(d) = &status.judged_on {
            out.push_str(&format!("  judged_on: {}\n", quoted(d.as_str())));
        }
        if claim.expressions.is_empty() {
            out.push_str("expressions: []\n");
        } else {
            out.push_str("expressions:\n");
            for e in &claim.expressions {
                out.push_str(&format!(
                    "  - page_id: {}\n    block: {}\n",
                    quoted(&e.page_id),
                    quoted(&e.block)
                ));
            }
        }
        if claim.relations.is_empty() {
            out.push_str("relations: []\n");
        } else {
            out.push_str("relations:\n");
            for r in &claim.relations {
                out.push_str(&format!(
                    "  - type: {}\n    claim: {}\n",
                    r.kind.word(),
                    quoted(r.claim.as_str())
                ));
            }
        }
        if claim.sources.is_empty() {
            out.push_str("sources: []\n");
        } else {
            out.push_str("sources:\n");
            for s in &claim.sources {
                out.push_str(&format!(
                    "  - source: {}\n    relation: {}\n    bearing: {}\n",
                    quoted(s.source.as_str()),
                    s.relation.word(),
                    s.bearing.word()
                ));
                if let Some(note) = &s.note {
                    out.push_str(&format!("    note: {}\n", quoted(note)));
                }
                out.push_str(&format!(
                    "    judged_on: {}\n",
                    quoted(s.judged_on.as_str())
                ));
            }
        }
        out
    }
}

fn community_status(value: &Value) -> Result<CommunityStatus, Fault> {
    let pairs = mapping(
        value,
        "community_status",
        &["status", "proposal", "judged_on"],
    )?;
    Ok(CommunityStatus {
        status: word(
            required(pairs, "status")?,
            Standing::FIELD,
            Standing::from_word,
        )?,
        proposal: optional(pairs, "proposal")
            .map(|v| proposal(v, "proposal"))
            .transpose()?,
        judged_on: optional(pairs, "judged_on")
            .map(|v| digest(v, "judged_on"))
            .transpose()?,
    })
}

/// Does a sorted list repeat an entry?
fn has_repeat<T: PartialEq>(sorted: &[T]) -> bool {
    sorted.windows(2).any(|w| w.first() == w.get(1))
}
