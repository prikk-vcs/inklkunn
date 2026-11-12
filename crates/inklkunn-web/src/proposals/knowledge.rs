//! What a proposal does to the knowledge layer, in the catalog's words (RFC 005 part 2 D-3): the claims it makes,
//! the sources it records and what it changes of those that exist, as the lines a proposal's page lists. **Facts,
//! not verdicts**: no word here says a claim or a source is good, true or settled, and a change of a community
//! status is said as the change it is.

use inklkunn_core::knowledge::Family;
use inklkunn_core::knowledge::summary::{Cite, ClaimChange, Item, SourceChange};

use super::labels::{
    availability_word, bearing_word, lifecycle_word, link_word, relation_type_word, source_word,
    standing_word, statement_word,
};
use crate::catalog::{Key, T};
use crate::health::rule_key;

/// One thing the proposal does, with the particulars under it.
#[derive(Clone)]
pub(crate) struct Line {
    pub(crate) text: String,
    pub(crate) more: Vec<String>,
}

fn cite(t: T, c: &Cite) -> String {
    t.msg(Key::KnowCite)
        .with("relation", t.get(link_word(c.relation)))
        .with("bearing", t.get(bearing_word(c.bearing)))
        .text()
}

fn claim_change(t: T, change: &ClaimChange) -> String {
    match change {
        ClaimChange::Kind(k) => t
            .msg(Key::KnowChangeKind)
            .with("kind", t.get(statement_word(*k)))
            .text(),
        ClaimChange::Lifecycle(l) => t
            .msg(Key::KnowChangeLifecycle)
            .with("lifecycle", t.get(lifecycle_word(*l)))
            .text(),
        ClaimChange::Standing { from, to } => t
            .msg(Key::KnowChangeStanding)
            .with("from", t.get(standing_word(*from)))
            .with("to", t.get(standing_word(*to)))
            .text(),
        ClaimChange::Said(_) => t.get(Key::KnowChangeSaid),
        ClaimChange::Unsaid(_) => t.get(Key::KnowChangeUnsaid),
        ClaimChange::Related(r) => t
            .msg(Key::KnowChangeRelated)
            .with("relation", t.get(relation_type_word(r.kind)))
            .text(),
        ClaimChange::Unrelated(r) => t
            .msg(Key::KnowChangeUnrelated)
            .with("relation", t.get(relation_type_word(r.kind)))
            .text(),
        ClaimChange::Cited(c) => cite(t, c),
        ClaimChange::Uncited(_) => t.get(Key::KnowChangeUncited),
        ClaimChange::Recited(c) => t
            .msg(Key::KnowChangeRecited)
            .with("relation", t.get(link_word(c.relation)))
            .with("bearing", t.get(bearing_word(c.bearing)))
            .text(),
        ClaimChange::Noted(_) => t.get(Key::KnowChangeNoted),
    }
}

fn source_change(t: T, change: &SourceChange) -> String {
    match change {
        SourceChange::Kind(k) => t
            .msg(Key::KnowSourceChangeKind)
            .with("kind", t.get(source_word(*k)))
            .text(),
        SourceChange::Identity => t.get(Key::KnowSourceChangeIdentity),
        SourceChange::Reference => t.get(Key::KnowSourceChangeReference),
        SourceChange::Snapshot => t.get(Key::KnowSourceChangeSnapshot),
        SourceChange::Reviewed(Some(day)) => {
            t.msg(Key::KnowSourceChangeReviewed).with("day", day).text()
        }
        SourceChange::Reviewed(None) => t.get(Key::KnowSourceChangeUnreviewed),
        SourceChange::Availability(a) => t
            .msg(Key::KnowSourceChangeAvailability)
            .with("availability", t.get(availability_word(*a)))
            .text(),
        SourceChange::Superseded => t.get(Key::KnowSourceChangeSuperseded),
    }
}

/// The lines of a proposal's page for these items.
pub(crate) fn lines(t: T, items: &[Item]) -> Vec<Line> {
    items
        .iter()
        .map(|item| match item {
            Item::ClaimMade {
                kind, said, cites, ..
            } => Line {
                text: t
                    .msg(Key::KnowClaimMade)
                    .with("kind", t.get(statement_word(*kind)))
                    .n(said.len() as u64)
                    .text(),
                more: cites.iter().map(|c| cite(t, c)).collect(),
            },
            Item::ClaimChanged { changes, .. } => Line {
                text: t.get(Key::KnowClaimChanged),
                more: changes.iter().map(|c| claim_change(t, c)).collect(),
            },
            Item::SourceRecorded { kind, title, .. } => Line {
                text: match title {
                    Some(title) => t
                        .msg(Key::KnowSourceRecorded)
                        .with("kind", t.get(source_word(*kind)))
                        .with("title", title)
                        .text(),
                    None => t
                        .msg(Key::KnowSourceRecordedUntitled)
                        .with("kind", t.get(source_word(*kind)))
                        .text(),
                },
                more: Vec::new(),
            },
            Item::SourceChanged { changes, .. } => Line {
                text: t.get(Key::KnowSourceChanged),
                more: changes.iter().map(|c| source_change(t, c)).collect(),
            },
            Item::Removed { family, .. } => Line {
                text: t.get(match family {
                    Family::Claim => Key::KnowRemovedClaim,
                    Family::Source => Key::KnowRemovedSource,
                }),
                more: Vec::new(),
            },
            Item::Unreadable { rule, .. } => Line {
                text: t
                    .msg(Key::KnowUnreadable)
                    .with("reason", t.get(rule_key(*rule)))
                    .text(),
                more: Vec::new(),
            },
        })
        .collect()
}

/// What a proposal's knowledge files come to against the base they were written on.
pub(crate) struct Known {
    /// The lines of the proposal's page.
    pub(crate) lines: Vec<Line>,
    /// The files that do what only a curation proposal may ([`curated_by`], the acceptance's own rule).
    pub(crate) curated: Vec<inklkunn_core::knowledge::check::Refusal>,
}

/// The lines for the knowledge files (`files`: path and new text, `None` for a removal) of a proposal written on the
/// block `base_block`: each file is compared with the text the base had.
pub(crate) fn file_lines(
    ctx: &super::Ctx,
    t: T,
    base_block: &str,
    files: &[inklkunn_store::FileChange],
) -> Result<Known, inklkunn_core::read::ReadError> {
    let mut before: std::collections::BTreeMap<String, Option<String>> =
        std::collections::BTreeMap::new();
    let mut given: Vec<(String, Option<String>)> = Vec::new();
    for f in files
        .iter()
        .filter(|f| inklkunn_core::knowledge::family_of(&f.path).is_some())
    {
        before.insert(
            f.path.clone(),
            ctx.services
                .read
                .source_at(&ctx.wiki, base_block, &f.path)?,
        );
        given.push((f.path.clone(), f.text.clone()));
    }
    let was = |path: &str| before.get(path).cloned().flatten();
    let items = inklkunn_core::knowledge::summary::summarize(&given, &was);
    let curated = inklkunn_core::knowledge::check::curated_by(&given, &was);
    Ok(Known {
        lines: lines(t, &items),
        curated,
    })
}

#[cfg(test)]
mod tests;
