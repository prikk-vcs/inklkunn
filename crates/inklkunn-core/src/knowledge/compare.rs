//! Comparing two views' claims (RFC 005 part 2 D-9; handoff 005 K-T8): **claim by claim, what differs** between the
//! knowledge indexes of two tips, in the three ways the RFC names: the *wording* (what the claim's paragraphs say),
//! the *status* (what the community has decided in that view) and the *sources* (which sources bear on the claim,
//! how, and what each source is).
//!
//! **Only differences are stated, and none is a verdict.** A claim present in one view and not in the other is said
//! so; a claim present in both whose three facts agree is not listed. Nothing here says which view is right, which is
//! adopted or which is nearer anything: the comparison has two sides, called `a` and `b` by the caller, and treats
//! them alike. It reads the derived indexes only (never the files again), and it is a pure function of the two.
//!
//! *Wording* is the digest of each paragraph a claim is said in, as the page has it at the tip
//! ([`index::PRESENT`](super::index::PRESENT) anchors only): two views word a claim differently when the sets of
//! those digests differ. A claim whose paragraph is missing in one view (the page or the block is not there) has no
//! wording there, and that is a difference too.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_store::KnowledgeSourceRow;

use super::CLAIMS_DIR;
use super::index::{KnowledgeIndex, PRESENT};

/// Where a claim is, of the two views compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Presence {
    /// Both views have it.
    Both,
    /// Only the first view has it.
    OnlyA,
    /// Only the second view has it.
    OnlyB,
    /// The first view has it, and the second has a file of that name that cannot be read: nothing can be said of
    /// what that view holds, and it is not said to have no such claim.
    UnreadableInB,
    /// The second view has it, and the first has a file of that name that cannot be read.
    UnreadableInA,
}

/// One claim that differs between two views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimDifference {
    /// The claim's id.
    pub id: String,
    /// Which views have it.
    pub presence: Presence,
    /// Both have it and its paragraphs read differently.
    pub wording: bool,
    /// Both have it and the community's status differs: the word in the first view and in the second.
    pub status: Option<(String, String)>,
    /// Both have it and the sources that bear on it, or how, or what a source is, differ.
    pub sources: bool,
}

/// A source a claim cites: the source, the relation, the bearing and, if the index holds the source, its kind and
/// whether it can be had.
type Cited<'a> = (&'a str, &'a str, &'a str, Option<(&'a str, &'a str)>);

/// What a claim shows of itself in one view, for comparing.
struct Face<'a> {
    standing: &'a str,
    /// The digests of the paragraphs it is said in, at blocks the pages really have.
    wording: BTreeSet<&'a str>,
    sources: BTreeSet<Cited<'a>>,
}

/// Every claim of an index with its face, by id.
fn faces(index: &KnowledgeIndex) -> BTreeMap<&str, Face<'_>> {
    let rows = index.rows();
    let sources: BTreeMap<&str, &KnowledgeSourceRow> =
        rows.sources.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut out: BTreeMap<&str, Face<'_>> = rows
        .claims
        .iter()
        .map(|c| {
            (
                c.id.as_str(),
                Face {
                    standing: c.standing.as_str(),
                    wording: BTreeSet::new(),
                    sources: BTreeSet::new(),
                },
            )
        })
        .collect();
    for a in rows.anchors.iter().filter(|a| a.state == PRESENT) {
        if let (Some(face), Some(digest)) = (out.get_mut(a.claim.as_str()), a.digest.as_deref()) {
            face.wording.insert(digest);
        }
    }
    for l in &rows.links {
        if let Some(face) = out.get_mut(l.claim.as_str()) {
            let of = sources
                .get(l.source.as_str())
                .map(|s| (s.kind.as_str(), s.availability.as_str()));
            face.sources.insert((
                l.source.as_str(),
                l.relation.as_str(),
                l.bearing.as_str(),
                of,
            ));
        }
    }
    out
}

/// The claims that differ between the tips two indexes are of, by claim id. Unbounded: the caller keeps as many as
/// it shows and says how many it left out.
pub fn compare(a: &KnowledgeIndex, b: &KnowledgeIndex) -> Vec<ClaimDifference> {
    let (fa, fb) = (faces(a), faces(b));
    let ids: BTreeSet<&str> = fa.keys().chain(fb.keys()).copied().collect();
    let mut out = Vec::new();
    for id in ids {
        let difference = match (fa.get(id), fb.get(id)) {
            (Some(x), Some(y)) => {
                let d = ClaimDifference {
                    id: id.to_owned(),
                    presence: Presence::Both,
                    wording: x.wording != y.wording,
                    status: (x.standing != y.standing)
                        .then(|| (x.standing.to_owned(), y.standing.to_owned())),
                    sources: x.sources != y.sources,
                };
                if !(d.wording || d.status.is_some() || d.sources) {
                    continue;
                }
                d
            }
            (Some(_), None) => only(
                id,
                if unreadable(b, id) {
                    Presence::UnreadableInB
                } else {
                    Presence::OnlyA
                },
            ),
            (None, Some(_)) => only(
                id,
                if unreadable(a, id) {
                    Presence::UnreadableInA
                } else {
                    Presence::OnlyB
                },
            ),
            (None, None) => continue,
        };
        out.push(difference);
    }
    out
}

/// Does the index list the claim's own file among those it could not read?
fn unreadable(index: &KnowledgeIndex, id: &str) -> bool {
    let path = format!("{CLAIMS_DIR}/{id}.yaml");
    index.invalid().iter().any(|f| f.path == path)
}

fn only(id: &str, presence: Presence) -> ClaimDifference {
    ClaimDifference {
        id: id.to_owned(),
        presence,
        wording: false,
        status: None,
        sources: false,
    }
}
