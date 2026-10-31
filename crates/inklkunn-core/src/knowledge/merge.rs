//! The knowledge files in RFC 004's merge (RFC 005 part 2, D-4; handoff K-T3): the third file family beside pages
//! and frontmatter.
//!
//! A claim file or a source file is merged **key by key**, as frontmatter is, and its lists **by the stable
//! identity of their entries**:
//!
//! - a claim's `expressions` by anchor (page id and block), its `relations` by `(type, claim)`, its `sources` by
//!   source id;
//! - an entry that has no value of its own (an expression, a relation) is *there* or *not*: added on one side, it is
//!   taken; removed on one side while the other left it, it is taken out; so nothing is lost without a side having
//!   removed it;
//! - an entry that has values (a source's link: relation, bearing, note, `judged_on`) is merged by its keys, and a
//!   value **changed on both sides, differently, is an overlap with both texts, never resolved**. So is an entry
//!   one side removed and the other changed, and one both sides added differently;
//! - a claim's `community_status` is one value (a status, the proposal that decided it and the words it was decided
//!   on belong together: a mixture of two decisions is no decision);
//! - a source file's `reference`, `snapshot` and `last_reviewed` are one value each, for the same reason; its
//!   `identity` is merged by its three keys.
//!
//! A file one side left as it was is taken from the other **as written** (nothing is rewritten that did not need to
//! be); a file both sides changed is merged and written in the canonical form (comments, which the format does not
//! have, do not survive). A file that is not wholly well formed on any of the three sides is not merged key by
//! key: both texts are an overlap, because nothing is read from a file that cannot be read. Pure.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Claim, CommunityStatus, Expression, Fault, File, Lifecycle, Relation, Source, SourceId,
    SourceLink, StatementKind, family_of, read_file,
};
use crate::merge::{LineLevel, MergedPage, Overlap, PageRef, PageText, Place};

/// Is this path one the knowledge merge takes (a claim's or a source's file)?
pub fn is_merged_here(path: &str) -> bool {
    family_of(path).is_some()
}

/// One place where the sides cannot be taken together.
struct Clash {
    key: String,
    base: Option<String>,
    theirs: Option<String>,
    ours: Option<String>,
}

type Merged<T> = Result<T, Box<Clash>>;

/// Three sides of one value: the base's, the tip's, the proposal's.
fn pick<T: PartialEq + Clone>(base: &T, theirs: &T, ours: &T) -> Option<T> {
    if theirs == ours || ours == base {
        Some(theirs.clone())
    } else if theirs == base {
        Some(ours.clone())
    } else {
        None
    }
}

/// A scalar by key: the value taken, or the clash with the three texts.
fn scalar<T: PartialEq + Clone>(
    key: &str,
    sides: [&T; 3],
    show: impl Fn(&T) -> Option<String>,
) -> Merged<T> {
    let [base, theirs, ours] = sides;
    pick(base, theirs, ours).ok_or_else(|| {
        Box::new(Clash {
            key: key.to_owned(),
            base: show(base),
            theirs: show(theirs),
            ours: show(ours),
        })
    })
}

/// A list of entries that have no value of their own: an entry is there if a side added it or both kept it.
fn presence<T: Ord + Clone>(base: &[T], theirs: &[T], ours: &[T]) -> Vec<T> {
    let (b, t, o): (BTreeSet<&T>, BTreeSet<&T>, BTreeSet<&T>) = (
        base.iter().collect(),
        theirs.iter().collect(),
        ours.iter().collect(),
    );
    let all: BTreeSet<&T> = b.iter().chain(&t).chain(&o).copied().collect();
    all.into_iter()
        .filter(|k| {
            let (in_t, in_o) = (t.contains(*k), o.contains(*k));
            // Both kept it, or one side added it (it was not in the base), or one side kept it while the other
            // removed it: a removal is taken only when the side that stayed left it as it was, which an entry with no
            // value of its own always does.
            match (in_t, in_o) {
                (true, true) => true,
                (true, false) | (false, true) => !b.contains(*k),
                (false, false) => false,
            }
        })
        .cloned()
        .collect()
}

fn link_text(link: &SourceLink) -> String {
    let mut out = format!(
        "source: {}\nrelation: {}\nbearing: {}\n",
        link.source,
        link.relation.word(),
        link.bearing.word()
    );
    if let Some(note) = &link.note {
        out.push_str(&format!("note: {note}\n"));
    }
    out.push_str(&format!("judged_on: {}\n", link.judged_on));
    out
}

fn standing_text(status: &CommunityStatus) -> String {
    let mut out = format!("status: {}\n", status.status.word());
    if let Some(p) = &status.proposal {
        out.push_str(&format!("proposal: {p}\n"));
    }
    if let Some(d) = &status.judged_on {
        out.push_str(&format!("judged_on: {d}\n"));
    }
    out
}

/// The sources of a claim by source id, merged by key.
fn links(
    base: &[SourceLink],
    theirs: &[SourceLink],
    ours: &[SourceLink],
) -> Merged<Vec<SourceLink>> {
    let by = |list: &[SourceLink]| -> BTreeMap<SourceId, SourceLink> {
        list.iter().map(|l| (l.source.clone(), l.clone())).collect()
    };
    let (b, t, o) = (by(base), by(theirs), by(ours));
    let ids: BTreeSet<&SourceId> = b.keys().chain(t.keys()).chain(o.keys()).collect();
    let mut out = Vec::new();
    for id in ids {
        let clash = |which: &str, bt: Option<String>, tt: Option<String>, ot: Option<String>| {
            Box::new(Clash {
                key: format!("sources/{id}{which}"),
                base: bt,
                theirs: tt,
                ours: ot,
            })
        };
        let whole = |l: Option<&SourceLink>| l.map(link_text);
        match (b.get(id), t.get(id), o.get(id)) {
            (None, Some(x), None) | (None, None, Some(x)) => out.push(x.clone()),
            (None, Some(x), Some(y)) => {
                if x == y {
                    out.push(x.clone());
                } else {
                    return Err(clash("", None, whole(Some(x)), whole(Some(y))));
                }
            }
            (Some(_), None, None) | (None, None, None) => {}
            // One side removed it: taken if the other left it as it was.
            (Some(base), Some(x), None) | (Some(base), None, Some(x)) => {
                if x != base {
                    let (tt, ot) = if t.contains_key(id) {
                        (whole(Some(x)), None)
                    } else {
                        (None, whole(Some(x)))
                    };
                    return Err(clash("", whole(Some(base)), tt, ot));
                }
            }
            (Some(base), Some(x), Some(y)) => {
                let field = |name: &str, show: &dyn Fn(&SourceLink) -> String| {
                    (name.to_owned(), show(base), show(x), show(y))
                };
                let relation = pick(&base.relation, &x.relation, &y.relation);
                let bearing = pick(&base.bearing, &x.bearing, &y.bearing);
                let note = pick(&base.note, &x.note, &y.note);
                let judged = pick(&base.judged_on, &x.judged_on, &y.judged_on);
                let (Some(relation), Some(bearing), Some(note), Some(judged_on)) =
                    (relation, bearing, note, judged)
                else {
                    let failing = if pick(&base.relation, &x.relation, &y.relation).is_none() {
                        field("/relation", &|l| l.relation.word().to_owned())
                    } else if pick(&base.bearing, &x.bearing, &y.bearing).is_none() {
                        field("/bearing", &|l| l.bearing.word().to_owned())
                    } else if pick(&base.note, &x.note, &y.note).is_none() {
                        field("/note", &|l| l.note.clone().unwrap_or_default())
                    } else {
                        field("/judged_on", &|l| l.judged_on.to_string())
                    };
                    let (name, bt, tt, ot) = failing;
                    return Err(clash(&name, Some(bt), Some(tt), Some(ot)));
                };
                out.push(SourceLink {
                    source: id.clone(),
                    relation,
                    bearing,
                    note,
                    judged_on,
                });
            }
        }
    }
    Ok(out)
}

fn claim(base: &Claim, theirs: &Claim, ours: &Claim) -> Merged<Claim> {
    let kind = scalar(
        "kind",
        [&base.kind, &theirs.kind, &ours.kind],
        |k: &StatementKind| Some(k.word().to_owned()),
    )?;
    let lifecycle = scalar(
        "lifecycle",
        [&base.lifecycle, &theirs.lifecycle, &ours.lifecycle],
        |l: &Lifecycle| Some(l.word().to_owned()),
    )?;
    let community_status = scalar(
        "community_status",
        [
            &base.community_status,
            &theirs.community_status,
            &ours.community_status,
        ],
        |s| Some(standing_text(s)),
    )?;
    let mut merged = Claim {
        id: base.id.clone(),
        kind,
        expressions: presence::<Expression>(
            &base.expressions,
            &theirs.expressions,
            &ours.expressions,
        ),
        relations: presence::<Relation>(&base.relations, &theirs.relations, &ours.relations),
        lifecycle,
        community_status,
        sources: links(&base.sources, &theirs.sources, &ours.sources)?,
    };
    merged.normalize();
    Ok(merged)
}

fn source(base: &Source, theirs: &Source, ours: &Source) -> Merged<Source> {
    let text = |t: &Option<String>| t.clone();
    let reference = |r: &Option<super::Reference>| {
        r.as_ref()
            .map(|r| format!("type: {}\nvalue: {}\n", r.form.word(), r.value))
    };
    let snapshot = |s: &Option<super::Snapshot>| {
        s.as_ref().map(|s| match s {
            super::Snapshot::Attachment { path } => {
                format!("kind: attachment\nattachment: {path}\n")
            }
            super::Snapshot::Archive { address, digest } => {
                format!("kind: archive\naddress: {address}\ndigest: {digest}\n")
            }
        })
    };
    let reviewed = |r: &Option<super::Reviewed>| {
        r.as_ref().map(|r| match &r.proposal {
            Some(p) => format!("date: {}\nproposal: {p}\n", r.date),
            None => format!("date: {}\n", r.date),
        })
    };
    Ok(Source {
        id: base.id.clone(),
        kind: scalar("kind", [&base.kind, &theirs.kind, &ours.kind], |k| {
            Some(k.word().to_owned())
        })?,
        availability: scalar(
            "availability",
            [&base.availability, &theirs.availability, &ours.availability],
            |a| Some(a.word().to_owned()),
        )?,
        identity: super::Identity {
            title: scalar(
                "identity/title",
                [
                    &base.identity.title,
                    &theirs.identity.title,
                    &ours.identity.title,
                ],
                text,
            )?,
            author: scalar(
                "identity/author",
                [
                    &base.identity.author,
                    &theirs.identity.author,
                    &ours.identity.author,
                ],
                text,
            )?,
            date: scalar(
                "identity/date",
                [
                    &base.identity.date,
                    &theirs.identity.date,
                    &ours.identity.date,
                ],
                text,
            )?,
        },
        reference: scalar(
            "reference",
            [&base.reference, &theirs.reference, &ours.reference],
            reference,
        )?,
        snapshot: scalar(
            "snapshot",
            [&base.snapshot, &theirs.snapshot, &ours.snapshot],
            snapshot,
        )?,
        last_reviewed: scalar(
            "last_reviewed",
            [
                &base.last_reviewed,
                &theirs.last_reviewed,
                &ours.last_reviewed,
            ],
            reviewed,
        )?,
        superseded_by: scalar(
            "superseded_by",
            [
                &base.superseded_by,
                &theirs.superseded_by,
                &ours.superseded_by,
            ],
            |s: &Option<SourceId>| s.as_ref().map(ToString::to_string),
        )?,
    })
}

/// The three files, read; `None` when any of them is not wholly well formed or is not the family the path says.
fn read_three(path: &str, base: &str, theirs: &str, ours: &str) -> Option<[File; 3]> {
    let read = |text: &str| -> Result<File, Fault> { read_file(path, text.as_bytes()) };
    Some([read(base).ok()?, read(theirs).ok()?, read(ours).ok()?])
}

/// The merged text of one file the three sides have, or the clash.
fn merge_one(path: &str, base: &str, theirs: &str, ours: &str) -> Merged<String> {
    let whole = || {
        Box::new(Clash {
            key: String::new(),
            base: Some(base.to_owned()),
            theirs: Some(theirs.to_owned()),
            ours: Some(ours.to_owned()),
        })
    };
    let Some([b, t, o]) = read_three(path, base, theirs, ours) else {
        return Err(whole());
    };
    match (b, t, o) {
        (File::Claim(b), File::Claim(t), File::Claim(o)) => Ok(claim(&b, &t, &o)?.to_yaml()),
        (File::Source(b), File::Source(t), File::Source(o)) => Ok(source(&b, &t, &o)?.to_yaml()),
        _ => Err(whole()),
    }
}

/// Merge the knowledge files of three sides (paths already routed by [`is_merged_here`]): the files the merged
/// change leaves, in path order, and the overlaps. Files are matched by path (a claim's id is its name).
pub fn merge_files(
    base: &[PageText],
    theirs: &[PageText],
    ours: &[PageText],
) -> (Vec<MergedPage>, Vec<Overlap>) {
    let by = |side: &[PageText]| -> BTreeMap<String, String> {
        side.iter()
            .map(|p| (p.path.clone(), p.text.clone()))
            .collect()
    };
    let (b, t, o) = (by(base), by(theirs), by(ours));
    let paths: BTreeSet<&String> = b.keys().chain(t.keys()).chain(o.keys()).collect();
    let mut files = Vec::new();
    let mut overlaps = Vec::new();
    for path in paths {
        let overlap =
            |key: String, base: Option<String>, theirs: Option<String>, ours: Option<String>| {
                Overlap {
                    page: PageRef {
                        id: None,
                        path: path.clone(),
                    },
                    place: Place::Knowledge { key },
                    base,
                    theirs,
                    ours,
                    line_level: LineLevel::NotText,
                }
            };
        let take = |text: &String| MergedPage {
            path: path.clone(),
            text: text.clone(),
        };
        match (b.get(path), t.get(path), o.get(path)) {
            (_, None, None) => {}
            (None, Some(x), None) | (None, None, Some(x)) => files.push(take(x)),
            (None, Some(x), Some(y)) => {
                if x == y {
                    files.push(take(x));
                } else {
                    overlaps.push(overlap(
                        String::new(),
                        None,
                        Some(x.clone()),
                        Some(y.clone()),
                    ));
                }
            }
            // Removed on one side: taken if the other side left the file as it was.
            (Some(base), Some(x), None) | (Some(base), None, Some(x)) => {
                if x != base {
                    let (tt, ot) = if t.contains_key(path) {
                        (Some(x.clone()), None)
                    } else {
                        (None, Some(x.clone()))
                    };
                    overlaps.push(overlap(String::new(), Some(base.clone()), tt, ot));
                }
            }
            (Some(base), Some(x), Some(y)) => {
                if x == y || y == base {
                    files.push(take(x));
                } else if x == base {
                    files.push(take(y));
                } else {
                    match merge_one(path, base, x, y) {
                        Ok(text) => files.push(MergedPage {
                            path: path.clone(),
                            text,
                        }),
                        Err(c) => overlaps.push(overlap(c.key, c.base, c.theirs, c.ours)),
                    }
                }
            }
        }
    }
    (files, overlaps)
}
