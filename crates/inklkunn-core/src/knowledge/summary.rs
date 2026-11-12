//! What the knowledge files of a proposal's revision do, in facts a surface can word (RFC 005 part 2 D-3): the
//! claims it makes, the sources it records, and what it changes of those that exist. **Facts only**: no verdict
//! on any of it, no word that ranks.
//!
//! Read from the files themselves, against the files as the proposal's base had them. A file that does not read
//! is said so by rule ([`Item::Unreadable`]), never by what it contained.

use std::collections::BTreeSet;

use super::{
    Availability, Bearing, Claim, ClaimId, Expression, Family, File, Lifecycle, Link, Relation,
    Rule, Source, SourceId, SourceKind, Standing, StatementKind, family_of, read_file,
};

/// A claim's citation of a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cite {
    /// The source.
    pub source: SourceId,
    /// Supports or challenges.
    pub relation: Link,
    /// How directly it speaks to the claim.
    pub bearing: Bearing,
}

/// One change to a claim that exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimChange {
    /// The kind of statement.
    Kind(StatementKind),
    /// Where it stands in its life.
    Lifecycle(Lifecycle),
    /// **The community status** (only a curation proposal may).
    Standing {
        /// Before.
        from: Standing,
        /// After.
        to: Standing,
    },
    /// Said in one more place.
    Said(Expression),
    /// No longer said in a place: the paragraph's marker is gone from the claim.
    Unsaid(Expression),
    /// Related to another claim.
    Related(Relation),
    /// No longer related to another claim.
    Unrelated(Relation),
    /// Cites one more source.
    Cited(Cite),
    /// No longer cites a source.
    Uncited(SourceId),
    /// The relation or the bearing of a citation changed.
    Recited(Cite),
    /// The note of a citation changed.
    Noted(SourceId),
}

/// One change to a source that exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChange {
    /// What kind of source.
    Kind(SourceKind),
    /// Who and what it is, as written.
    Identity,
    /// Where it is found.
    Reference,
    /// The copy kept.
    Snapshot,
    /// The day a person last checked it (`None`: cleared).
    Reviewed(Option<String>),
    /// Whether it can be had, as the person who checked says.
    Availability(Availability),
    /// The source that replaces it.
    Superseded,
}

/// One thing a revision does to the knowledge layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A claim is made.
    ClaimMade {
        /// The claim.
        claim: ClaimId,
        /// What kind of statement.
        kind: StatementKind,
        /// Its community status as the file has it (only a curation proposal may set one).
        standing: Standing,
        /// Where it is said.
        said: Vec<Expression>,
        /// What it cites.
        cites: Vec<Cite>,
    },
    /// A claim that exists is changed.
    ClaimChanged {
        /// The claim.
        claim: ClaimId,
        /// What changes.
        changes: Vec<ClaimChange>,
    },
    /// A source is recorded.
    SourceRecorded {
        /// The source.
        source: SourceId,
        /// What kind.
        kind: SourceKind,
        /// Its title as written, if it has one.
        title: Option<String>,
        /// The file says someone checked it (a review date or an availability): only a curation proposal may.
        checked: bool,
    },
    /// A source that exists is changed.
    SourceChanged {
        /// The source.
        source: SourceId,
        /// What changes.
        changes: Vec<SourceChange>,
    },
    /// A knowledge file is removed.
    Removed {
        /// Which kind of file.
        family: Family,
        /// Its id, as its name spells it.
        id: String,
    },
    /// A file the revision writes is not one the layer reads (by rule: the file's words are not repeated).
    Unreadable {
        /// Which kind of file it is meant to be.
        family: Family,
        /// The rule it breaks.
        rule: Rule,
    },
}

fn cite_of(link: &super::SourceLink) -> Cite {
    Cite {
        source: link.source.clone(),
        relation: link.relation,
        bearing: link.bearing,
    }
}

/// What changes between two states of a claim.
fn claim_changes(before: &Claim, after: &Claim) -> Vec<ClaimChange> {
    let mut out = Vec::new();
    if before.kind != after.kind {
        out.push(ClaimChange::Kind(after.kind));
    }
    if before.lifecycle != after.lifecycle {
        out.push(ClaimChange::Lifecycle(after.lifecycle));
    }
    if before.community_status != after.community_status {
        out.push(ClaimChange::Standing {
            from: before.community_status.status,
            to: after.community_status.status,
        });
    }
    let (old, new): (BTreeSet<_>, BTreeSet<_>) = (
        before.expressions.iter().collect(),
        after.expressions.iter().collect(),
    );
    out.extend(
        new.difference(&old)
            .map(|e| ClaimChange::Said((*e).clone())),
    );
    out.extend(
        old.difference(&new)
            .map(|e| ClaimChange::Unsaid((*e).clone())),
    );
    let (old, new): (BTreeSet<_>, BTreeSet<_>) = (
        before.relations.iter().collect(),
        after.relations.iter().collect(),
    );
    out.extend(
        new.difference(&old)
            .map(|r| ClaimChange::Related((*r).clone())),
    );
    out.extend(
        old.difference(&new)
            .map(|r| ClaimChange::Unrelated((*r).clone())),
    );
    for link in &after.sources {
        match before.sources.iter().find(|l| l.source == link.source) {
            None => out.push(ClaimChange::Cited(cite_of(link))),
            Some(was) => {
                if (was.relation, was.bearing) != (link.relation, link.bearing) {
                    out.push(ClaimChange::Recited(cite_of(link)));
                }
                if was.note != link.note {
                    out.push(ClaimChange::Noted(link.source.clone()));
                }
            }
        }
    }
    for was in &before.sources {
        if !after.sources.iter().any(|l| l.source == was.source) {
            out.push(ClaimChange::Uncited(was.source.clone()));
        }
    }
    out
}

fn source_changes(before: &Source, after: &Source) -> Vec<SourceChange> {
    let mut out = Vec::new();
    if before.kind != after.kind {
        out.push(SourceChange::Kind(after.kind));
    }
    if before.identity != after.identity {
        out.push(SourceChange::Identity);
    }
    if before.reference != after.reference {
        out.push(SourceChange::Reference);
    }
    if before.snapshot != after.snapshot {
        out.push(SourceChange::Snapshot);
    }
    let day = |s: &Source| s.last_reviewed.as_ref().map(|r| r.date.clone());
    if day(before) != day(after) {
        out.push(SourceChange::Reviewed(day(after)));
    }
    if before.availability != after.availability {
        out.push(SourceChange::Availability(after.availability));
    }
    if before.superseded_by != after.superseded_by {
        out.push(SourceChange::Superseded);
    }
    out
}

/// The id a knowledge file's name spells.
fn name_of(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|n| n.strip_suffix(".yaml"))
        .unwrap_or("")
        .to_owned()
}

/// What `files` (path, and the new text or `None` for a removal) do to the knowledge layer, against `before` (the
/// text the proposal's base had at a path). Files that are not knowledge files are not in the answer. In path
/// order.
pub fn summarize(
    files: &[(String, Option<String>)],
    before: &dyn Fn(&str) -> Option<String>,
) -> Vec<Item> {
    let mut sorted: Vec<&(String, Option<String>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (path, text) in sorted {
        let Some(family) = family_of(path) else {
            continue;
        };
        let was = before(path).and_then(|t| read_file(path, t.as_bytes()).ok());
        let Some(text) = text else {
            out.push(Item::Removed {
                family,
                id: name_of(path),
            });
            continue;
        };
        let now = match read_file(path, text.as_bytes()) {
            Ok(file) => file,
            Err(fault) => {
                out.push(Item::Unreadable {
                    family,
                    rule: fault.rule,
                });
                continue;
            }
        };
        match (was, now) {
            (None, File::Claim(c)) => out.push(Item::ClaimMade {
                claim: c.id.clone(),
                kind: c.kind,
                standing: c.community_status.status,
                said: c.expressions.clone(),
                cites: c.sources.iter().map(cite_of).collect(),
            }),
            (Some(File::Claim(b)), File::Claim(a)) => {
                let changes = claim_changes(&b, &a);
                if !changes.is_empty() {
                    out.push(Item::ClaimChanged {
                        claim: a.id.clone(),
                        changes,
                    });
                }
            }
            (None, File::Source(s)) => out.push(Item::SourceRecorded {
                source: s.id.clone(),
                kind: s.kind,
                title: s.identity.title.clone(),
                checked: s.last_reviewed.is_some() || s.availability != Availability::Unknown,
            }),
            (Some(File::Source(b)), File::Source(a)) => {
                let changes = source_changes(&b, &a);
                if !changes.is_empty() {
                    out.push(Item::SourceChanged {
                        source: a.id.clone(),
                        changes,
                    });
                }
            }
            // The family follows the path, so the two cannot differ for one path.
            _ => {}
        }
    }
    out
}
