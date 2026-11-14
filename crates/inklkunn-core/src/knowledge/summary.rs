//! What the knowledge files of a proposal's revision do, in facts a surface can word (RFC 005 part 2 D-3): the
//! claims it makes, the sources it records, and what it changes of those that exist. **Facts only**: no verdict
//! on any of it, no word that ranks.
//!
//! Read from the files themselves, against the files as the proposal's base had them. A file that does not read
//! is said so by rule ([`Item::Unreadable`]), never by what it contained.

use std::collections::BTreeSet;

use super::{
    Availability, Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Family, File,
    Identity, Lifecycle, Link, Reference, Relation, Reviewed, Rule, Source, SourceId, SourceKind,
    SourceLink, Standing, StatementKind, family_of, read_file,
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

/// What a re-judgement is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejudgement {
    /// The claim's community status.
    Standing,
    /// A source the claim cites.
    Source(SourceId),
}

/// One change to a claim that exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimChange {
    /// The kind of statement.
    Kind(StatementKind),
    /// Where it stands in its life.
    Lifecycle(Lifecycle),
    /// **The community status** (only a curation proposal may). Only when the word itself changes: a status whose
    /// word stays and whose wording moves is [`ClaimChange::Rejudged`].
    Standing {
        /// Before.
        from: Standing,
        /// After.
        to: Standing,
    },
    /// **Trust moved to other wording** (RFC 005 part 1 H-1 point 3): what the community status, or a source link, was
    /// judged on is now another wording (or none). Never silent: every one of these is a line of its own, with the
    /// wording it moved to.
    Rejudged {
        /// What was judged.
        on: Rejudgement,
        /// The wording it is judged on now: the digest of the paragraph's text (the claim's [`Item::ClaimChanged`]
        /// `said` says where to look), `None` when it is no longer tied to one.
        to: Option<Digest>,
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
    /// Who and what it is, as written: **what it is now** (K15: a line that says only that it changed cannot be
    /// told from another).
    Identity(Identity),
    /// Where it is found: what it is now (`None`: it says nowhere).
    Reference(Option<Reference>),
    /// The copy kept.
    Snapshot,
    /// The day a person last checked it (`None`: cleared).
    Reviewed(Option<String>),
    /// Whether it can be had, as the person who checked says.
    Availability(Availability),
    /// The source that replaces it (`None`: it is no longer marked as replaced).
    Superseded(Option<SourceId>),
}

/// One thing a revision does to the knowledge layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A claim is made. **Everything the file says that a person decides on is here** (K15): the page has a line for
    /// each of these, and none is left out by a `..`.
    ClaimMade {
        /// The claim.
        claim: ClaimId,
        /// What kind of statement.
        kind: StatementKind,
        /// Its community status as the file has it (only a curation proposal may set one).
        standing: Standing,
        /// The wording that status was decided on, when it was.
        judged_on: Option<Digest>,
        /// Where it stands in its life.
        lifecycle: Lifecycle,
        /// Where it is said.
        said: Vec<Expression>,
        /// What it relates to.
        relations: Vec<Relation>,
        /// What it cites.
        cites: Vec<Cite>,
    },
    /// A claim that exists is changed.
    ClaimChanged {
        /// The claim.
        claim: ClaimId,
        /// Where it is said once the change is made: where a surface finds the wording a [`ClaimChange::Rejudged`]
        /// moved to, and the paragraph that names the claim on the page.
        said: Vec<Expression>,
        /// Where it was said before: the paragraph that names the claim when it is said nowhere now.
        was_said: Vec<Expression>,
        /// What changes.
        changes: Vec<ClaimChange>,
    },
    /// A source is recorded. **Everything the file says of it** (K15): who and what it is, where it is found, whether
    /// it can be had and the day a person checked it are on its line.
    SourceRecorded {
        /// The source.
        source: SourceId,
        /// What kind.
        kind: SourceKind,
        /// Who and what it is, as written.
        identity: Identity,
        /// Where it is found.
        reference: Option<Reference>,
        /// A copy of it is kept (which one is the file's).
        snapshot: bool,
        /// The day a person last checked it: only a curation proposal may set one.
        reviewed: Option<String>,
        /// The source that replaces it.
        superseded_by: Option<SourceId>,
        /// Whether it can be had, as the person who checked says: only a curation proposal may set one.
        availability: Availability,
    },
    /// A source that exists is changed.
    SourceChanged {
        /// The source.
        source: SourceId,
        /// Who and what it was, as the base had it: the name by which a reader knows it.
        named: Identity,
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
    /// **A file the base has and the layer cannot read is removed or written over** (handoff 005 corrections, second
    /// round, K16): what it held is not known, so this is said as what it is, never as a removal of a claim or a
    /// source or as a new one. Only a curation proposal may do it.
    UnreadableReplaced {
        /// Which kind of file it is meant to be.
        family: Family,
        /// Its id, as its name spells it.
        id: String,
    },
    /// A file the revision writes is not one the layer reads (by rule: the file's words are not repeated).
    Unreadable {
        /// Which kind of file it is meant to be.
        family: Family,
        /// Its name, as the path spells it (never what the file said).
        id: String,
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
///
/// **Total by construction** (handoff 005 corrections K5): both claims are taken apart field by field with no `..`, so
/// a field added to [`Claim`], [`CommunityStatus`] or [`SourceLink`] does not compile until it is either described here
/// or named as not being the person's to write. The only fields left out are the proposal ids the service writes itself
/// at acceptance (`meaningful`).
fn claim_changes(before: &Claim, after: &Claim) -> Vec<ClaimChange> {
    let Claim {
        id: _,
        kind,
        expressions,
        relations,
        lifecycle,
        community_status,
        sources,
    } = before;
    let mut out = Vec::new();
    if *kind != after.kind {
        out.push(ClaimChange::Kind(after.kind));
    }
    if *lifecycle != after.lifecycle {
        out.push(ClaimChange::Lifecycle(after.lifecycle));
    }
    let CommunityStatus {
        status,
        proposal: _,
        judged_on,
    } = community_status;
    if *status != after.community_status.status {
        out.push(ClaimChange::Standing {
            from: *status,
            to: after.community_status.status,
        });
    }
    if *judged_on != after.community_status.judged_on {
        out.push(ClaimChange::Rejudged {
            on: Rejudgement::Standing,
            to: after.community_status.judged_on.clone(),
        });
    }
    let (old, new): (BTreeSet<_>, BTreeSet<_>) = (
        expressions.iter().collect(),
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
    let (old, new): (BTreeSet<_>, BTreeSet<_>) =
        (relations.iter().collect(), after.relations.iter().collect());
    out.extend(
        new.difference(&old)
            .map(|r| ClaimChange::Related((*r).clone())),
    );
    out.extend(
        old.difference(&new)
            .map(|r| ClaimChange::Unrelated((*r).clone())),
    );
    for link in &after.sources {
        match sources.iter().find(|l| l.source == link.source) {
            None => out.push(ClaimChange::Cited(cite_of(link))),
            Some(was) => {
                let SourceLink {
                    source: _,
                    relation,
                    bearing,
                    note,
                    judged_on,
                } = was;
                if (*relation, *bearing) != (link.relation, link.bearing) {
                    out.push(ClaimChange::Recited(cite_of(link)));
                }
                if *note != link.note {
                    out.push(ClaimChange::Noted(link.source.clone()));
                }
                if *judged_on != link.judged_on {
                    out.push(ClaimChange::Rejudged {
                        on: Rejudgement::Source(link.source.clone()),
                        to: Some(link.judged_on.clone()),
                    });
                }
            }
        }
    }
    for was in sources {
        if !after.sources.iter().any(|l| l.source == was.source) {
            out.push(ClaimChange::Uncited(was.source.clone()));
        }
    }
    out
}

fn source_changes(before: &Source, after: &Source) -> Vec<SourceChange> {
    let Source {
        id: _,
        kind,
        identity,
        reference,
        snapshot,
        last_reviewed,
        superseded_by,
        availability,
    } = before;
    let mut out = Vec::new();
    if *kind != after.kind {
        out.push(SourceChange::Kind(after.kind));
    }
    if *identity != after.identity {
        out.push(SourceChange::Identity(after.identity.clone()));
    }
    if *reference != after.reference {
        out.push(SourceChange::Reference(after.reference.clone()));
    }
    if *snapshot != after.snapshot {
        out.push(SourceChange::Snapshot);
    }
    // The day; the proposal that said so is the service's own (stamped at acceptance).
    let day = |r: &Option<Reviewed>| r.as_ref().map(|r| r.date.clone());
    if day(last_reviewed) != day(&after.last_reviewed) {
        out.push(SourceChange::Reviewed(day(&after.last_reviewed)));
    }
    if *availability != after.availability {
        out.push(SourceChange::Availability(after.availability));
    }
    if *superseded_by != after.superseded_by {
        out.push(SourceChange::Superseded(after.superseded_by.clone()));
    }
    out
}

/// A file as the person's change is about it: without the proposal ids the service writes itself at acceptance
/// (a status's, a review's), whatever the file says ([`super::curation::stamp`]). Two files with the same meaningful
/// content are the same change, and two that differ have a line on the proposal's page.
fn meaningful(file: &File) -> File {
    match file {
        File::Claim(c) => {
            let mut c = c.clone();
            c.community_status.proposal = None;
            File::Claim(c)
        }
        File::Source(s) => {
            let mut s = s.clone();
            if let Some(review) = &mut s.last_reviewed {
                review.proposal = None;
            }
            File::Source(s)
        }
    }
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
        let had = before(path);
        let was = had
            .as_ref()
            .and_then(|t| read_file(path, t.as_bytes()).ok());
        let replaces_unreadable = had.is_some() && was.is_none();
        if replaces_unreadable {
            out.push(Item::UnreadableReplaced {
                family,
                id: name_of(path),
            });
        }
        let Some(text) = text else {
            if !replaces_unreadable {
                out.push(Item::Removed {
                    family,
                    id: name_of(path),
                });
            }
            continue;
        };
        let now = match read_file(path, text.as_bytes()) {
            Ok(file) => file,
            Err(fault) => {
                out.push(Item::Unreadable {
                    family,
                    id: name_of(path),
                    rule: fault.rule,
                });
                continue;
            }
        };
        match (was, now) {
            (None, File::Claim(c)) => {
                // Taken apart with no `..`: a field added to a claim does not compile until this says what the page
                // does with it (K15).
                let Claim {
                    id,
                    kind,
                    expressions,
                    relations,
                    lifecycle,
                    community_status:
                        CommunityStatus {
                            status,
                            proposal: _,
                            judged_on,
                        },
                    sources,
                } = *c;
                out.push(Item::ClaimMade {
                    claim: id,
                    kind,
                    standing: status,
                    judged_on,
                    lifecycle,
                    said: expressions,
                    relations,
                    cites: sources.iter().map(cite_of).collect(),
                });
            }
            (Some(File::Claim(b)), File::Claim(a)) => {
                let changes = claim_changes(&b, &a);
                if !changes.is_empty() {
                    out.push(Item::ClaimChanged {
                        claim: a.id.clone(),
                        said: a.expressions.clone(),
                        was_said: b.expressions.clone(),
                        changes,
                    });
                }
            }
            (None, File::Source(s)) => {
                let Source {
                    id,
                    kind,
                    identity,
                    reference,
                    snapshot,
                    last_reviewed,
                    superseded_by,
                    availability,
                } = *s;
                out.push(Item::SourceRecorded {
                    source: id,
                    kind,
                    identity,
                    reference,
                    snapshot: snapshot.is_some(),
                    reviewed: last_reviewed.map(|r| r.date),
                    superseded_by,
                    availability,
                });
            }
            (Some(File::Source(b)), File::Source(a)) => {
                let changes = source_changes(&b, &a);
                if !changes.is_empty() {
                    out.push(Item::SourceChanged {
                        source: a.id.clone(),
                        named: b.identity.clone(),
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

/// The knowledge files among `files` whose change **no line of the proposal's page would describe** (handoff 005
/// corrections K5: trust never moves unseen). Empty is the only answer a proposal may be sent with.
///
/// *Described* is this, and only this: the file as the proposal writes it, read and without the proposal ids the
/// service stamps itself at acceptance (`meaningful`), is not the file the base had, and [`summarize`] gives no
/// [`Item`] for it. A new file, a removal and a file that does not read are each an item of their own; a file that
/// reads the same as the base's is no change and is not asked about. So the answer is non-empty exactly when
/// [`summarize`] would print nothing for a file that changes something: a field the summary does not know. (The field
/// lists of `claim_changes` and `source_changes` are exhaustive, and the property test over every field and
/// random pairs holds this.) In path order.
pub fn undescribed(
    files: &[(String, Option<String>)],
    before: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    undescribed_by(files, before, &summarize)
}

/// What describes a set of files: [`summarize`] is the page's own; a test may hand a blind one.
pub type Describer<'a> =
    &'a dyn Fn(&[(String, Option<String>)], &dyn Fn(&str) -> Option<String>) -> Vec<Item>;

/// [`undescribed`], with the describer given (the proposal service asks it with [`summarize`], the one the page prints).
pub fn undescribed_by(
    files: &[(String, Option<String>)],
    before: &dyn Fn(&str) -> Option<String>,
    describe: Describer<'_>,
) -> Vec<String> {
    let mut sorted: Vec<&(String, Option<String>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (path, text) in sorted {
        let (Some(_), Some(text)) = (family_of(path), text) else {
            continue;
        };
        let (Some(was), Ok(now)) = (
            before(path).and_then(|t| read_file(path, t.as_bytes()).ok()),
            read_file(path, text.as_bytes()),
        ) else {
            continue;
        };
        if meaningful(&was) != meaningful(&now)
            && describe(&[(path.clone(), Some(text.clone()))], before).is_empty()
        {
            out.push(path.clone());
        }
    }
    out
}
