//! Validation at acceptance (RFC 005 part 2, D-5): a change that touches the knowledge layer is judged **before the
//! signer is asked**, on the files as the change would leave them.
//!
//! What is judged, for the knowledge files the change writes:
//!
//! - the schema and the ids ([`read_file`]: the path, the size, the YAML subset, the closed
//!   words, no number anywhere);
//! - every source and claim a file names is present, and readable, in the same view (the tip with the change on
//!   it);
//! - every expression names a page the view has once, and a block that page has once (`^id`), **at the proposal's
//!   result**;
//! - and, whenever the change touches the knowledge layer, no key that only the service writes
//!   ([`OWNED_PAGE_KEYS`]) is changed in any page the change writes.
//!
//! And for what the change leaves behind: a claim file the change does not write is judged again where the change
//! touched a page it is anchored in (a block taken out of a page cannot leave a claim pointing at nothing), and a
//! knowledge file the change removes must not be named by another file.
//!
//! A repository that already holds invalid files is not made unusable by them: only the change's own files, and
//! what it breaks, are refused. The files that are invalid are listed by the derived index
//! ([`crate::knowledge::index`]) and reported on the health list.
//!
//! **A refusal names the file and the rule, never what the file said** ([`Refusal`]); the name is escaped as the
//! import escapes a name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use inklkunn_markup::{Value, frontmatter, parse_metadata};
use inklkunn_prikk::{Point, Storage, StorageError};
use inklkunn_signer::PreparedFile;
use inklkunn_store::WikiRecord;

use super::tree::{TreeError, read_tree};
use super::{
    AnchorState, Availability, Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Fault,
    File, Identity, KNOWLEDGE_DIR, Link, Reference, Reviewed, Rule, SourceId, anchor_state,
    claim_path, family_of, is_keep, is_knowledge_path, read_file, source_path,
};
use crate::import::shown;
use crate::read::{ReadError, ReadService, entries_from_tree};

/// The page keys only the service writes (RFC 004 D-2; threat model C-4b), **the proposal service's own list**: one
/// constant, not a copy (handoff 005, follow-up K2). A change to the knowledge layer may not alter them in any page it
/// writes.
pub use crate::proposal::files::OWNED_KEYS as OWNED_PAGE_KEYS;

/// A file the change would write or leave, and the one rule it breaks. Plain text, safe to show: the file's name is
/// escaped, and nothing the file said is in it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Refusal {
    /// The file, escaped as the import escapes a name.
    pub file: String,
    /// The rule.
    pub rule: Rule,
    /// The key of the format the rule was about, from the format's own vocabulary, or empty.
    pub field: &'static str,
    /// The YAML line, when the parser gave one.
    pub line: Option<usize>,
}

impl Refusal {
    /// A refusal of `path` for `fault`.
    pub fn of(path: &str, fault: Fault) -> Refusal {
        Refusal {
            file: shown(path),
            rule: fault.rule,
            field: fault.field,
            line: fault.line,
        }
    }

    fn rule(path: &str, rule: Rule, field: &'static str) -> Refusal {
        Refusal::of(path, Fault::at(rule, field))
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.rule.sentence())?;
        if !self.field.is_empty() {
            write!(f, " (`{}`)", self.field)?;
        }
        if let Some(line) = self.line {
            write!(f, " (line {line})")?;
        }
        Ok(())
    }
}

/// Which page has a given `id` in the view the change would leave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageLookup {
    /// Exactly one page, at this path.
    One(String),
    /// No page.
    None,
    /// More than one page claims the id: the anchor does not say which.
    Several,
}

/// The pages of the view the change would leave.
pub trait Pages {
    /// The page whose frontmatter `id` is `id`.
    fn with_id(&self, id: &str) -> PageLookup;
    /// The whole text of the page at `path`, `None` when it has none or cannot be read.
    fn text(&self, path: &str) -> Option<String>;
}

/// What a claim's source link holds of what only curation changes (part 2 D-3): how the source bears on the claim,
/// and the wording that was judged. The link's note is not among them: it is words, and goes with the page edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    /// Supports or challenges.
    pub relation: Link,
    /// How directly the source speaks to the claim.
    pub bearing: Bearing,
    /// The wording the relation was judged on.
    pub judged_on: Digest,
}

/// What only a curation proposal sets (part 2 D-3, D-10, and the clarification of 2026-11-08): a claim's community
/// status and **every change to a source link it already has** (its relation, its bearing, the wording it was judged
/// on, or its removal; attaching a new link stays with the page edit it belongs to); a source's `last_reviewed` and
/// `availability`; and, **for a source that was checked** (it has a review day or an availability), who and what it is
/// and where it is found (K30: the check vouches for that work, so a page edit may not make it another). A file the
/// tip does not have has the baseline: nothing decided, no link, nothing checked.
///
/// **One rule for every surface**: acceptance ([`validate`]), the proposal service at submit and at every revision,
/// and the proposal page's warning all ask [`Curated::departure`] and [`Curated::removal_refused`] (through
/// [`curated_by`] where there is no tree), so a proposal the acceptance would refuse is never one that can be
/// submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Curated {
    /// A claim's standing and the judgements of the sources it cites.
    Claim {
        /// The standing, the proposal that decided it and the wording it was decided on.
        status: CommunityStatus,
        /// Its source links' judgements, by source.
        links: BTreeMap<SourceId, Judged>,
    },
    /// A source's freshness, and the work it names.
    Source {
        /// Whether it can be had, as a person who checked says.
        availability: Availability,
        /// When a person last checked it.
        reviewed: Option<Reviewed>,
        /// Who and what it is: curation's to change once the source was checked.
        identity: Identity,
        /// Where it is found: curation's to change once the source was checked.
        reference: Option<Reference>,
    },
}

impl Curated {
    /// What `file` holds of it.
    pub fn of(file: &File) -> Curated {
        match file {
            File::Claim(c) => Curated::Claim {
                status: c.community_status.clone(),
                links: c
                    .sources
                    .iter()
                    .map(|l| {
                        (
                            l.source.clone(),
                            Judged {
                                relation: l.relation,
                                bearing: l.bearing,
                                judged_on: l.judged_on.clone(),
                            },
                        )
                    })
                    .collect(),
            },
            File::Source(s) => Curated::Source {
                availability: s.availability,
                reviewed: s.last_reviewed.clone(),
                identity: s.identity.clone(),
                reference: s.reference.clone(),
            },
        }
    }

    /// What a file that was never there holds: nothing decided, no link and nothing checked.
    pub fn baseline(file: &File) -> Curated {
        match file {
            File::Claim(_) => Curated::Claim {
                status: CommunityStatus::undecided(),
                links: BTreeMap::new(),
            },
            File::Source(s) => Curated::Source {
                availability: Availability::Unknown,
                reviewed: None,
                // Nothing was checked, so the work it names is the page edit's whatever it is.
                identity: s.identity.clone(),
                reference: s.reference.clone(),
            },
        }
    }

    /// Does `self` (what a change leaves) depart from `was` (what the tip had) in what only curation sets? The key
    /// of the format the departure is about, or `None`. A link added is no departure; a link changed or taken
    /// away is.
    pub fn departure(&self, was: &Curated) -> Option<&'static str> {
        match (was, self) {
            (
                Curated::Claim {
                    status: a,
                    links: before,
                },
                Curated::Claim {
                    status: b,
                    links: after,
                },
            ) => {
                if a != b {
                    Some("community_status")
                } else if before
                    .iter()
                    .any(|(source, judged)| after.get(source) != Some(judged))
                {
                    Some("sources")
                } else {
                    None
                }
            }
            (
                Curated::Source {
                    availability: a,
                    reviewed: ra,
                    identity: ia,
                    reference: fa,
                },
                Curated::Source {
                    availability: b,
                    reviewed: rb,
                    identity: ib,
                    reference: fb,
                },
            ) => {
                if a != b {
                    Some("availability")
                } else if ra != rb {
                    Some("last_reviewed")
                } else if *a == Availability::Unknown && ra.is_none() {
                    // Not checked: who and what it is, and where, are the page edit's.
                    None
                } else if ia != ib {
                    Some("identity")
                } else if fa != fb {
                    Some("reference")
                } else {
                    None
                }
            }
            // A path names one family, so the two cannot differ for one path.
            _ => None,
        }
    }

    /// Is taking this away, outside a curation proposal, refused? **When it would take anything curated with it**
    /// (handoff 005 corrections, second round, K16): a claim file whose curated content is not what a fresh claim's is
    /// (a status that was decided, or an un-decided record, **or a source link**: the link is curation's to change
    /// or take away, and removing the file and writing the claim again would launder it out) and a source that was
    /// checked or marked available or not. A file that holds none of it may go with any proposal.
    pub fn removal_refused(&self) -> bool {
        self.removal_field().is_some()
    }

    /// The key of the format that makes taking this away curation's, or `None` when it holds nothing curated (K29: the
    /// refusal names the field it is about, not always the community status). The first that applies, in the order
    /// [`Curated::departure`] reads them: a claim's status, then its links; a source's availability, then its review.
    pub fn removal_field(&self) -> Option<&'static str> {
        match self {
            Curated::Claim { status, links } => {
                if *status != CommunityStatus::undecided() {
                    Some("community_status")
                } else if !links.is_empty() {
                    Some("sources")
                } else {
                    None
                }
            }
            Curated::Source {
                availability,
                reviewed,
                ..
            } => {
                if *availability != Availability::Unknown {
                    Some("availability")
                } else if reviewed.is_some() {
                    Some("last_reviewed")
                } else {
                    None
                }
            }
        }
    }
}

/// The field a refusal names when a file the tip has but cannot read is removed or written over: it is not a key of
/// the format, and what the file held is not repeated.
pub const UNREADABLE: &str = "unreadable";

/// The knowledge files a revision writes (`files`: path, and the new text or `None` for a removal) that do what only
/// a curation proposal may, judged against `before` (the text a path had at the proposal's base). The same rule as
/// the acceptance's ([`Curated::departure`], [`Curated::removal_refused`]), run **without a tree**: it is what the
/// proposal service asks at submit and at every revision, and what the proposal page's warning shows. A file that
/// does not read is not judged here (the acceptance refuses it by its own rule). In path order.
pub fn curated_by(
    files: &[(String, Option<String>)],
    before: &dyn Fn(&str) -> Option<String>,
) -> Vec<Refusal> {
    let mut sorted: Vec<&(String, Option<String>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (path, text) in sorted {
        if family_of(path).is_none() {
            continue;
        }
        let had = before(path);
        let was = had
            .as_ref()
            .and_then(|t| read_file(path, t.as_bytes()).ok());
        // A file the base has and the layer cannot read: what it held cannot be told from nothing, so taking it away
        // or writing over it is curation's (K16), never a page proposal's.
        if had.is_some() && was.is_none() {
            out.push(Refusal::rule(path, Rule::Status, UNREADABLE));
            continue;
        }
        match text {
            None => {
                if let Some(field) = was
                    .as_ref()
                    .and_then(|file| Curated::of(file).removal_field())
                {
                    out.push(Refusal::rule(path, Rule::Status, field));
                }
            }
            Some(text) => {
                let Ok(now) = read_file(path, text.as_bytes()) else {
                    continue;
                };
                let was = was
                    .as_ref()
                    .map_or_else(|| Curated::baseline(&now), Curated::of);
                if let Some(field) = Curated::of(&now).departure(&was) {
                    out.push(Refusal::rule(path, Rule::Status, field));
                }
            }
        }
    }
    out
}

/// A new source link whose `judged_on` names no wording any paragraph of its claim has in the result of the change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unjudged {
    /// The claim.
    pub claim: ClaimId,
    /// The source the link cites.
    pub source: SourceId,
}

/// The wordings (digests of the paragraph, marker and all) the places `claim` is said in have in `pages`: one for each
/// place whose page the view has once and whose block that page has once.
fn wordings_of(claim: &Claim, pages: &dyn Pages) -> Vec<Digest> {
    claim
        .expressions
        .iter()
        .filter_map(|e| {
            let PageLookup::One(at) = pages.with_id(&e.page_id) else {
                return None;
            };
            match anchor_state(&pages.text(&at)?, &e.block) {
                AnchorState::Present(digest) => Some(digest),
                _ => None,
            }
        })
        .collect()
}

/// The sources of the links `now` has that `was` (what the tip had of the claim; `None` for a claim it did not have)
/// does not, **whose `judged_on` is not a wording one of the claim's paragraphs has in the result** (`pages`; handoff
/// 005 corrections, third round, K25 and RFC 005 part 2, D-3, *a new link*). A digest must never name wording that is
/// not in the history: a link judged on words nobody has written would read *current* the day a page edit happened to
/// write them. What `compose` writes for a link is exactly such a wording (the paragraph as the page will have it).
///
/// **One rule for every surface**, like [`Curated::departure`]: the acceptance ([`validate`]) asks it of the files it
/// writes, and the proposal service at submit and at every revision asks it through [`unjudged_by`].
pub fn new_links_unjudged(now: &Claim, was: Option<&Curated>, pages: &dyn Pages) -> Vec<SourceId> {
    let had = match was {
        Some(Curated::Claim { links, .. }) => Some(links),
        _ => None,
    };
    let fresh: Vec<_> = now
        .sources
        .iter()
        .filter(|l| !had.is_some_and(|links| links.contains_key(&l.source)))
        .collect();
    if fresh.is_empty() {
        return Vec::new();
    }
    let wordings = wordings_of(now, pages);
    fresh
        .into_iter()
        .filter(|l| !wordings.contains(&l.judged_on))
        .map(|l| l.source.clone())
        .collect()
}

/// The new links of the claim files a revision writes (`files`: path, and the new text or `None` for a removal) that
/// are judged on no wording the claim has in the result (`pages`), against `before` (the text a path had at the
/// proposal's base): [`new_links_unjudged`] **without a tree**, what the proposal service asks at submit and at every
/// revision and what the proposal page says. A file that does not read is not judged here (the acceptance refuses it by
/// its own rule). In path order.
pub fn unjudged_by(
    files: &[(String, Option<String>)],
    before: &dyn Fn(&str) -> Option<String>,
    pages: &dyn Pages,
) -> Vec<Unjudged> {
    let mut sorted: Vec<&(String, Option<String>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (path, text) in sorted {
        if family_of(path) != Some(super::Family::Claim) {
            continue;
        }
        let Some(Ok(File::Claim(now))) = text.as_ref().map(|t| read_file(path, t.as_bytes()))
        else {
            continue;
        };
        let was = before(path)
            .and_then(|t| read_file(path, t.as_bytes()).ok())
            .map(|file| Curated::of(&file));
        out.extend(
            new_links_unjudged(&now, was.as_ref(), pages)
                .into_iter()
                .map(|source| Unjudged {
                    claim: now.id.clone(),
                    source,
                }),
        );
    }
    out
}

/// The change, as the validator needs it.
#[derive(Debug, Default)]
pub struct Touched {
    /// Knowledge paths the change writes (they are in the result's files).
    pub written: BTreeSet<String>,
    /// Knowledge paths the change removes.
    pub removed: BTreeSet<String>,
    /// The ids of the pages the change writes, removes or renames, at either side of it.
    pub page_ids: BTreeSet<String>,
    /// Paths of the pages the change writes whose owned keys it alters.
    pub owned: BTreeSet<String>,
    /// What the tip had, of [`Curated`], for each knowledge file the change writes or removes that the tip has.
    pub curated_before: BTreeMap<String, Curated>,
    /// The knowledge paths the change writes, removes or moves that the tip has and cannot read (K16): replacing one
    /// is curation's, since what it held is not known.
    pub unreadable_before: BTreeSet<String>,
    /// The change is a curation proposal (its files are knowledge files and nothing else,
    /// [`super::curation::is_curation`]); only then may it set what only curation sets. **Not by default.**
    pub curation: bool,
}

/// Judge a change. `files` is every knowledge file of the view as the change would leave it (path, first bytes),
/// `touched` what the change did, `pages` the pages as it would leave them and `tip_pages` as they are now (a file
/// the change does not write is refused only for an anchor the change broke, not for one that was already broken).
/// The refusals are in file order and empty when the change is sound.
pub fn validate(
    files: &BTreeMap<String, Vec<u8>>,
    touched: &Touched,
    pages: &dyn Pages,
    tip_pages: &dyn Pages,
) -> Vec<Refusal> {
    let read: BTreeMap<&str, Result<File, Fault>> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), read_file(path, bytes)))
        .collect();
    let readable = |path: String| matches!(read.get(path.as_str()), Some(Ok(_)));
    let mut out: BTreeSet<Refusal> = BTreeSet::new();

    // The files the change writes: all of it.
    for path in &touched.written {
        match read.get(path.as_str()) {
            Some(Ok(file)) => {
                out.extend(references(path, file, &readable));
                if !broken(file, pages, None).is_empty() {
                    out.insert(Refusal::rule(path, Rule::Anchor, "expressions"));
                }
                // What only a curation proposal sets (part 2 D-3): the file must hold what the tip had of it.
                let was = touched
                    .curated_before
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| Curated::baseline(file));
                if !touched.curation
                    && let Some(field) = Curated::of(file).departure(&was)
                {
                    out.insert(Refusal::rule(path, Rule::Status, field));
                }
                // A link the change adds is judged on a wording the claim has in the result (K25), in any proposal.
                if let File::Claim(claim) = file
                    && !new_links_unjudged(claim, touched.curated_before.get(path), pages)
                        .is_empty()
                {
                    out.insert(Refusal::rule(path, Rule::Judged, "sources"));
                }
            }
            Some(Err(fault)) => {
                out.insert(Refusal::of(path, fault.clone()));
            }
            // Not in the result: it was written as nothing; the change itself is not sound.
            None => {
                out.insert(Refusal::rule(path, Rule::Path, ""));
            }
        }
    }

    // The files the change leaves alone but may have broken: a page it touched, a file it removed.
    for (path, file) in &read {
        if touched.written.contains(*path) {
            continue;
        }
        let Ok(file) = file else { continue };
        let now = broken(file, pages, Some(&touched.page_ids));
        if !now.is_empty() {
            let before = broken(file, tip_pages, Some(&touched.page_ids));
            if now.iter().any(|e| !before.contains(e)) {
                out.insert(Refusal::rule(path, Rule::Anchor, "expressions"));
            }
        }
    }
    // Nothing curated is taken away outside a curation proposal either (K16): not a decided status, not a link, not a
    // review or an availability, with the file that holds it.
    for path in &touched.removed {
        if !touched.curation
            && let Some(field) = touched
                .curated_before
                .get(path)
                .and_then(Curated::removal_field)
        {
            out.insert(Refusal::rule(path, Rule::Status, field));
        }
    }
    // A file the tip has and cannot read is not replaced or taken away by any other proposal: nobody can say what
    // went with it.
    if !touched.curation {
        for path in touched.written.union(&touched.removed) {
            if touched.unreadable_before.contains(path) {
                out.insert(Refusal::rule(path, Rule::Status, UNREADABLE));
            }
        }
    }
    for path in &touched.removed {
        let Some(name) = path
            .rsplit('/')
            .next()
            .and_then(|n| n.strip_suffix(".yaml"))
        else {
            continue;
        };
        let named = |file: &File| match (family_of(path), file) {
            (Some(super::Family::Claim), File::Claim(c)) => {
                c.relations.iter().any(|r| r.claim.as_str() == name)
            }
            (Some(super::Family::Claim), File::Source(_)) => false,
            (Some(super::Family::Source), File::Claim(c)) => {
                c.sources.iter().any(|s| s.source.as_str() == name)
            }
            (Some(super::Family::Source), File::Source(s)) => {
                s.superseded_by.as_ref().is_some_and(|b| b.as_str() == name)
            }
            (None, _) => false,
        };
        let in_use = read.iter().any(|(other, file)| {
            !touched.written.contains(*other) && file.as_ref().is_ok_and(named)
        });
        if in_use {
            out.insert(Refusal::rule(path, Rule::InUse, ""));
        }
    }

    // Owned keys, whenever the knowledge layer is touched at all.
    if !touched.written.is_empty() || !touched.removed.is_empty() {
        for path in &touched.owned {
            out.insert(Refusal::rule(path, Rule::Owned, ""));
        }
    }
    out.into_iter().collect()
}

/// A written file's references to other files: each must be present and readable in the view.
fn references(path: &str, file: &File, readable: &dyn Fn(String) -> bool) -> Vec<Refusal> {
    let mut out = Vec::new();
    match file {
        File::Claim(claim) => {
            for link in &claim.sources {
                if !readable(source_path(&link.source)) {
                    out.push(Refusal::rule(path, Rule::Reference, "sources"));
                }
            }
            for relation in &claim.relations {
                if !readable(claim_path(&relation.claim)) {
                    out.push(Refusal::rule(path, Rule::Reference, "relations"));
                }
            }
        }
        File::Source(source) => {
            if let Some(later) = &source.superseded_by
                && !readable(source_path(later))
            {
                out.push(Refusal::rule(path, Rule::Reference, "superseded_by"));
            }
        }
    }
    out
}

/// A claim's expressions that do not name a page the view has once and a block that page has once. With `only`,
/// just the expressions in the pages with these ids (for a file the change did not write).
fn broken<'a>(
    file: &'a File,
    pages: &dyn Pages,
    only: Option<&BTreeSet<String>>,
) -> Vec<&'a Expression> {
    let File::Claim(claim) = file else {
        return Vec::new();
    };
    claim
        .expressions
        .iter()
        .filter(|e| {
            if only.is_some_and(|ids| !ids.contains(&e.page_id)) {
                return false;
            }
            match pages.with_id(&e.page_id) {
                PageLookup::One(at) => !pages.text(&at).is_some_and(|text| {
                    matches!(anchor_state(&text, &e.block), AnchorState::Present(_))
                }),
                PageLookup::None | PageLookup::Several => true,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------- the change

/// Why a change could not be judged (not why it is unsound: that is a list of [`Refusal`]s).
#[derive(Debug)]
pub enum CheckError {
    /// prikk could not be asked.
    Storage(StorageError),
    /// The tip could not be extracted or read.
    Read(ReadError),
    /// The extracted knowledge files could not be read.
    Tree(TreeError),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::Storage(e) => write!(f, "{e}"),
            CheckError::Read(e) => write!(f, "{e}"),
            CheckError::Tree(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CheckError {}

/// The pages of the tip with the change on them: every page of the extracted tree, a rename moving a page, a
/// written file replacing one, a removal taking one out.
struct ResultPages {
    root: PathBuf,
    /// Where each page of the result is: in the extracted tree, or written by the change.
    at: BTreeMap<String, Origin>,
    ids: BTreeMap<String, Vec<String>>,
}

enum Origin {
    Tree(String),
    Written(String),
}

impl Pages for ResultPages {
    fn with_id(&self, id: &str) -> PageLookup {
        match self.ids.get(id).map(Vec::as_slice) {
            Some([one]) => PageLookup::One(one.clone()),
            Some([]) | None => PageLookup::None,
            Some(_) => PageLookup::Several,
        }
    }

    fn text(&self, path: &str) -> Option<String> {
        match self.at.get(path)? {
            Origin::Written(text) => Some(text.clone()),
            Origin::Tree(from) => std::fs::read(self.root.join(from))
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok()),
        }
    }
}

/// The paths that have each page id.
fn by_id(ids_at: &BTreeMap<String, Option<String>>) -> BTreeMap<String, Vec<String>> {
    let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, id) in ids_at {
        if let Some(id) = id {
            ids.entry(id.clone()).or_default().push(path.clone());
        }
    }
    ids
}

fn page_id_of(text: &str) -> Option<String> {
    parse_metadata(text)
        .ok()
        .and_then(|m| m.id)
        .filter(|id| !id.is_empty())
}

/// The owned keys of a page text as JSON text, `None` for a text whose information block is not readable.
fn owned_keys(text: &str) -> Option<BTreeMap<&'static str, String>> {
    let (yaml, _, _) = frontmatter::split(text);
    let Some(yaml) = yaml else {
        return Some(BTreeMap::new());
    };
    let Value::Map(pairs) = frontmatter::parse_yaml(yaml).ok()? else {
        return Some(BTreeMap::new());
    };
    Some(
        OWNED_PAGE_KEYS
            .into_iter()
            .filter_map(|key| {
                pairs
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| (key, v.to_json().to_string()))
            })
            .collect(),
    )
}

fn read_bounded(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// What an acceptance is about to write.
#[derive(Debug, Clone, Copy)]
pub struct Written<'a> {
    /// The files to write or remove.
    pub files: &'a [PreparedFile],
    /// The renames.
    pub renames: &'a [(PathBuf, PathBuf)],
    /// The proposal is a curation proposal ([`super::curation::is_curation`]).
    pub curation: bool,
}

/// Judge a prepared change against the tip `tip_block`: `Ok(vec![])` when it is sound.
///
/// Free of cost for a wiki that has no knowledge layer: when neither the change nor the tip holds a path under
/// `knowledge/`, one `tree` call answers and nothing is extracted. Otherwise the tip is extracted once (the read
/// service's extract, under its build lock) and the change laid over it.
pub fn check_change(
    reads: &ReadService,
    storage: &dyn Storage,
    repo: &Path,
    wiki: &WikiRecord,
    tip_block: &str,
    change: &Written<'_>,
) -> Result<Vec<Refusal>, CheckError> {
    let Written {
        files,
        renames,
        curation,
    } = *change;
    let path_text = |p: &Path| p.to_string_lossy().into_owned();
    // A keep-file is no knowledge file: a new wiki's `knowledge/` holds nothing else.
    let mine = |p: &str| is_knowledge_path(p) && !is_keep(p);
    let touches = files.iter().any(|f| mine(&path_text(&f.path)))
        || renames
            .iter()
            .any(|(a, b)| mine(&path_text(a)) || mine(&path_text(b)));
    if !touches {
        let listing = storage
            .tree(
                repo,
                &Point::Block(tip_block.to_owned()),
                Some(KNOWLEDGE_DIR),
            )
            .map_err(CheckError::Storage)?;
        if !listing
            .entries
            .iter()
            .any(|e| e.kind == "file" && !is_keep(&e.path))
        {
            return Ok(Vec::new());
        }
    }
    reads
        .with_block_tree(wiki, tip_block, |root, config| {
            judge(
                root,
                config.index_max_files,
                config.page_max_bytes,
                files,
                renames,
                curation,
            )
        })
        .map_err(CheckError::Read)?
}

pub(crate) fn judge(
    root: &Path,
    max_entries: u64,
    page_max_bytes: u64,
    files: &[PreparedFile],
    renames: &[(PathBuf, PathBuf)],
    curation: bool,
) -> Result<Vec<Refusal>, CheckError> {
    let path_text = |p: &Path| p.to_string_lossy().into_owned();
    // The knowledge files of the result: the tip's, the change on them.
    let mut knowledge: BTreeMap<String, Vec<u8>> = read_tree(root, max_entries)
        .map_err(CheckError::Tree)?
        .into_iter()
        .map(|f| (f.path, f.bytes))
        .collect();
    let mut touched = Touched {
        curation,
        ..Touched::default()
    };
    // What the tip had of what only curation sets, for the files the change writes or moves.
    for path in files
        .iter()
        .map(|f| path_text(&f.path))
        .chain(renames.iter().map(|(from, _)| path_text(from)))
    {
        if let Some(bytes) = knowledge.get(&path) {
            match read_file(&path, bytes) {
                Ok(file) => {
                    touched.curated_before.insert(path, Curated::of(&file));
                }
                Err(_) => {
                    touched.unreadable_before.insert(path);
                }
            }
        }
    }
    for (from, to) in renames {
        let (from, to) = (path_text(from), path_text(to));
        if is_knowledge_path(&from)
            && !is_keep(&from)
            && let Some(bytes) = knowledge.remove(&from)
        {
            touched.removed.insert(from.clone());
            if is_knowledge_path(&to) {
                touched.written.insert(to.clone());
                knowledge.insert(to.clone(), bytes);
            }
        }
    }
    for f in files {
        let path = path_text(&f.path);
        if !is_knowledge_path(&path) || is_keep(&path) {
            continue;
        }
        match &f.content {
            Some(bytes) => {
                knowledge.insert(
                    path.clone(),
                    bytes
                        .iter()
                        .take(super::MAX_FILE_BYTES + 1)
                        .copied()
                        .collect(),
                );
                touched.written.insert(path);
            }
            None => {
                knowledge.remove(&path);
                touched.written.remove(&path);
                touched.removed.insert(path);
            }
        }
    }

    // The pages of the result.
    let tip_pages = entries_from_tree(root, max_entries).map_err(|e| match e {
        crate::read::TreeError::TooManyEntries(n) => CheckError::Tree(TreeError::TooMany(n)),
        crate::read::TreeError::Io(e) => CheckError::Tree(TreeError::Io(e)),
    })?;
    let mut at: BTreeMap<String, Origin> = BTreeMap::new();
    let mut ids_at: BTreeMap<String, Option<String>> = BTreeMap::new();
    for entry in &tip_pages {
        at.insert(entry.path.clone(), Origin::Tree(entry.path.clone()));
        ids_at.insert(entry.path.clone(), entry.page_id.clone());
    }
    let tip_ids = ids_at.clone();
    let tip = ResultPages {
        root: root.to_path_buf(),
        at: at
            .keys()
            .map(|p| (p.clone(), Origin::Tree(p.clone())))
            .collect(),
        ids: by_id(&ids_at),
    };
    let mut changed_pages: BTreeSet<String> = BTreeSet::new();
    for (from, to) in renames {
        let (from, to) = (path_text(from), path_text(to));
        changed_pages.insert(from.clone());
        changed_pages.insert(to.clone());
        if let Some(origin) = at.remove(&from) {
            at.insert(to.clone(), origin);
            let id = ids_at.remove(&from).flatten();
            ids_at.insert(to, id);
        }
    }
    let mut owned_candidates: Vec<(String, String)> = Vec::new();
    for f in files {
        let path = path_text(&f.path);
        if is_knowledge_path(&path) {
            continue;
        }
        changed_pages.insert(path.clone());
        match f.content.as_deref().map(|b| String::from_utf8(b.to_vec())) {
            Some(Ok(text)) => {
                ids_at.insert(path.clone(), page_id_of(&text));
                owned_candidates.push((path.clone(), text.clone()));
                at.insert(path, Origin::Written(text));
            }
            Some(Err(_)) => {
                // Not text: not a page the view can anchor in.
                at.remove(&path);
                ids_at.remove(&path);
            }
            None => {
                at.remove(&path);
                ids_at.remove(&path);
            }
        }
    }
    let ids = by_id(&ids_at);
    // Page ids on either side of the change, for the pages it touched.
    for path in &changed_pages {
        for id in [
            tip_ids.get(path).cloned().flatten(),
            ids_at.get(path).cloned().flatten(),
        ]
        .into_iter()
        .flatten()
        {
            touched.page_ids.insert(id);
        }
    }
    // Owned keys: a page the change writes that differs from the tip's in any of them.
    for (path, new) in owned_candidates {
        let old = read_bounded(&root.join(&path), page_max_bytes);
        let before = old.as_deref().and_then(owned_keys).unwrap_or_default();
        if let Some(after) = owned_keys(&new)
            && before != after
        {
            touched.owned.insert(path);
        }
    }
    let pages = ResultPages {
        root: root.to_path_buf(),
        at,
        ids,
    };
    Ok(validate(&knowledge, &touched, &pages, &tip))
}

impl From<ReadError> for CheckError {
    fn from(e: ReadError) -> Self {
        CheckError::Read(e)
    }
}
