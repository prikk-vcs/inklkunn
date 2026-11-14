//! Making a claim and attaching a source (RFC 005 part 2 D-3): the files a proposal carries so that a paragraph
//! becomes the place a claim is said and a source is attached to it, **in the same proposal as any page edit**.
//!
//! Pure: a page's text in, the page's new text and the claim and source files out. Nothing is written, read or
//! authorized here; the proposal that carries the files goes through the one path every proposal goes through (the
//! service, the review, the acceptance and its validation, [`crate::knowledge::check`]).
//!
//! What attaching does to the page: the paragraph is given its `^id` when it has none (the claim's expression is
//! this marker, part 1 D-1 a) and the page is given an `id` in its information block when it has none (an
//! expression names the page by it). Both are page edits of the same proposal, shown to the reviewer as such.
//! Everything written is read back before it is returned; a text that does not read back is a refusal, never a
//! file the acceptance would then refuse.

use std::collections::BTreeSet;
use std::ops::Range;

use inklkunn_markup::{BlockKind, blocks, frontmatter, parse_metadata};

use super::yaml::{MAX_LINKS, MAX_TEXT_CHARS, is_day};
use super::{
    AnchorState, Availability, Bearing, Claim, ClaimId, Expression, Identity, Lifecycle, Link,
    Reference, ReferenceForm, Source, SourceId, SourceKind, SourceLink, Standing, StatementKind,
    anchor_state, claim_path, source_path,
};

/// Why a claim or a source could not be written from what a person gave. Each is one plain fact the surface words;
/// none repeats what was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mistake {
    /// The part holds no paragraph a claim can be said in.
    NoParagraph,
    /// The page's information block cannot be read, so the page cannot be given an `id`.
    PageUnreadable,
    /// More than one claim is said in the paragraph: the form does not say which is meant.
    SeveralClaims,
    /// The claim has as many sources as a claim may.
    Full,
    /// The source says nothing: it needs a title or a reference.
    SourceSaysNothing,
    /// A text is longer than the format allows (the name of the field).
    TooLong(&'static str),
    /// A text holds a control character (the name of the field).
    ControlCharacter(&'static str),
    /// A text holds a bidirectional control or an invisible format character (the name of the field).
    Direction(&'static str),
    /// A web address that is not `http` or `https`.
    NotAWebAddress,
    /// A date that is not written `YYYY-MM-DD`.
    NotADay,
    /// A source the claim does not cite.
    UnknownSource,
    /// The claim is said nowhere now, so there is no wording to judge a source or a standing on.
    NoWording,
    /// The same link is both changed and removed.
    RemovedAndChanged,
    /// Who and what a source is, where it is found, or what replaces it is curation's to change **once the source was
    /// checked**; this source was not, so that is the page edit's.
    NotChecked,
    /// A source is not replaced by itself.
    ReplacedByItself,
    /// The source that replaces another is not a source of the wiki.
    UnknownSuccessor,
    /// What was made does not read back (a defect, not the person's doing).
    Unwritable,
}

/// What a person wrote about a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDraft {
    /// What kind of source.
    pub kind: SourceKind,
    /// A title, as written.
    pub title: Option<String>,
    /// An author or a publisher, as written.
    pub author: Option<String>,
    /// A date, as written (a year, a season, a day).
    pub date: Option<String>,
    /// Where it is found.
    pub reference: Option<Reference>,
}

/// A text of the format: one line, no control character, within the bound; an empty one is nothing.
pub fn clean_text(text: &str, field: &'static str) -> Result<Option<String>, Mistake> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(Mistake::TooLong(field));
    }
    if text.chars().any(char::is_control) {
        return Err(Mistake::ControlCharacter(field));
    }
    if text.chars().any(super::yaml::is_bidi_or_format) {
        return Err(Mistake::Direction(field));
    }
    Ok(Some(text.to_owned()))
}

/// A reference as it is written in a file: a web address is `http` or `https` (a reader may follow it), and none
/// is empty.
pub fn clean_reference(form: ReferenceForm, value: &str) -> Result<Option<Reference>, Mistake> {
    let Some(value) = clean_text(value, "reference")? else {
        return Ok(None);
    };
    if form == ReferenceForm::Url {
        let lower = value.to_ascii_lowercase();
        let web = lower.starts_with("https://") || lower.starts_with("http://");
        if !web || value.chars().any(char::is_whitespace) {
            return Err(Mistake::NotAWebAddress);
        }
    }
    Ok(Some(Reference { form, value }))
}

impl SourceDraft {
    /// The source file this draft is, under `id`, or why it cannot be one.
    pub fn into_source(self, id: SourceId) -> Result<Source, Mistake> {
        if self.title.is_none() && self.reference.is_none() {
            return Err(Mistake::SourceSaysNothing);
        }
        let mut source = Source::new(id);
        source.kind = self.kind;
        source.identity.title = self.title;
        source.identity.author = self.author;
        source.identity.date = self.date;
        source.reference = self.reference;
        Ok(source)
    }
}

/// What is attached to a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The source.
    pub source: SourceDraft,
    /// Supports or challenges.
    pub relation: Link,
    /// How directly the source speaks to the claim.
    pub bearing: Bearing,
    /// A note, in words.
    pub note: Option<String>,
    /// What kind of statement the claim is, when the paragraph has none yet.
    pub kind: StatementKind,
}

/// The files and the page text an attachment comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    /// The page, with the paragraph's `^id` and the page's `id` where they were missing.
    pub page: String,
    /// The claim file (path and text) and the source file (path and text).
    pub files: [(String, String); 2],
    /// The claim.
    pub claim: ClaimId,
    /// The new source.
    pub source: SourceId,
    /// The paragraph's `^id`.
    pub block: String,
    /// The claim was made here (the paragraph had none).
    pub created: bool,
}

/// The first paragraph of `page` that overlaps `region`, with its range and its `^id` if it has one.
fn paragraph_in(page: &str, region: &Range<usize>) -> Option<(Range<usize>, Option<String>)> {
    let (_, body, _) = frontmatter::split(page);
    let offset = page.len().checked_sub(body.len())?;
    blocks(body)
        .into_iter()
        .filter(|b| b.kind == BlockKind::Paragraph)
        .map(|b| (offset + b.range.start..offset + b.range.end, b.id))
        .find(|(r, _)| r.start < region.end && region.start < r.end)
        .map(|(r, id)| (r, id.map(str::to_owned)))
}

/// The `^id` of the paragraph at `region` of `page`, when it has one: the question *is a claim said here already*
/// asked before anything is attached.
pub fn paragraph_id(page: &str, region: &Range<usize>) -> Result<Option<String>, Mistake> {
    paragraph_in(page, region)
        .map(|(_, id)| id)
        .ok_or(Mistake::NoParagraph)
}

/// The `id` of a page's information block, when it has one.
pub fn page_id_of(page: &str) -> Option<String> {
    parse_metadata(page)
        .ok()
        .and_then(|m| m.id)
        .filter(|id| !id.is_empty())
}

/// `page` with `id: <id>` as the first line of its information block (made, if it has none).
fn with_page_id(page: &str, id: &str) -> Option<String> {
    match frontmatter::locate(page) {
        Some(layout) => {
            let eol = if page.get(layout.open.clone())?.ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            Some(format!(
                "{}id: {id}{eol}{}",
                page.get(..layout.yaml.start)?,
                page.get(layout.yaml.start..)?
            ))
        }
        None => {
            let bom = page.len() - page.strip_prefix('\u{feff}').unwrap_or(page).len();
            Some(format!(
                "{}---\nid: {id}\n---\n\n{}",
                page.get(..bom)?,
                page.get(bom..)?
            ))
        }
    }
}

/// `^id` for a paragraph that has none, derived from the claim's id and not taken on the page.
fn fresh_block_id(claim: &ClaimId, page: &str) -> String {
    let (_, body, _) = frontmatter::split(page);
    let taken: BTreeSet<&str> = blocks(body).iter().filter_map(|b| b.id).collect();
    let compact: String = claim.as_str().chars().filter(|c| *c != '-').collect();
    let tail: String = compact
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut id = format!("k{tail}");
    let mut n = 2;
    while taken.contains(id.as_str()) {
        id = format!("k{tail}-{n}");
        n += 1;
    }
    id
}

/// Attach a source to the paragraph at `region` of `page` (the page as the proposal leaves it).
///
/// `existing` is the claim already said in the paragraph, when there is one (the caller reads it: it is not
/// found here), and the source is attached to it; otherwise a claim is made. The digest the link is judged on is
/// that of the paragraph **as the page will have it**, marker and all.
pub fn attach(
    page: &str,
    region: &Range<usize>,
    existing: Option<&Claim>,
    what: &Attachment,
) -> Result<Attached, Mistake> {
    let note = what
        .note
        .as_deref()
        .map(|n| clean_text(n, "note"))
        .transpose()?
        .flatten();
    let source_id = SourceId::new();
    let source = what.source.clone().into_source(source_id.clone())?;
    let (range, had) = paragraph_in(page, region).ok_or(Mistake::NoParagraph)?;

    let claim_id = existing.map_or_else(ClaimId::new, |c| c.id.clone());
    let block = had
        .clone()
        .unwrap_or_else(|| fresh_block_id(&claim_id, page));
    let mut text = page.to_owned();
    if had.is_none() {
        let words = page.get(range.clone()).ok_or(Mistake::Unwritable)?;
        let marked = format!("{} ^{block}", words.trim_end());
        text = format!(
            "{}{marked}{}",
            page.get(..range.start).ok_or(Mistake::Unwritable)?,
            page.get(range.end..).ok_or(Mistake::Unwritable)?
        );
    }
    let page_id = match page_id_of(&text) {
        Some(id) => id,
        None => {
            let id = inklkunn_store::record::uuid_v7();
            text = with_page_id(&text, &id).ok_or(Mistake::PageUnreadable)?;
            id
        }
    };
    // Read it again: the page is readable, has the id, and has the paragraph once with its marker.
    if page_id_of(&text).as_deref() != Some(page_id.as_str()) {
        return Err(Mistake::PageUnreadable);
    }
    let AnchorState::Present(digest) = anchor_state(&text, &block) else {
        return Err(Mistake::Unwritable);
    };

    let created = existing.is_none();
    let mut claim = match existing {
        Some(found) => found.clone(),
        None => {
            let mut fresh = Claim::new(claim_id.clone());
            fresh.kind = what.kind;
            fresh.lifecycle = Lifecycle::Proposed;
            fresh
        }
    };
    let here = Expression::new(&page_id, &block).map_err(|_| Mistake::Unwritable)?;
    if !claim.expressions.contains(&here) {
        if created {
            claim.expressions.push(here);
        } else {
            // The claim found is not the one said here.
            return Err(Mistake::Unwritable);
        }
    }
    if claim.sources.len() >= MAX_LINKS {
        return Err(Mistake::Full);
    }
    claim.sources.push(SourceLink {
        source: source_id.clone(),
        relation: what.relation,
        bearing: what.bearing,
        note,
        judged_on: digest,
    });
    claim.normalize();
    let (claim_text, source_text) = (claim.to_yaml(), source.to_yaml());
    // Both read back as what was made.
    let claim_back =
        Claim::parse(claim_id.clone(), &claim_text).map_err(|_| Mistake::Unwritable)?;
    let source_back =
        Source::parse(source_id.clone(), &source_text).map_err(|_| Mistake::Unwritable)?;
    if claim_back != claim || source_back != source {
        return Err(Mistake::Unwritable);
    }
    Ok(Attached {
        page: text,
        files: [
            (claim_path(&claim_id), claim_text),
            (source_path(&source_id), source_text),
        ],
        claim: claim_id,
        source: source_id,
        block,
        created,
    })
}

/// The claim without its expressions at these blocks of the page `page_id`: what a person who removes a paragraph
/// others rely on has said, explicitly, when they ask for it. The claim stays (a claim with no expression is a
/// claim said nowhere, and says so); only the reviewer's decision retires it.
pub fn without_expressions(claim: &Claim, page_id: &str, blocks: &BTreeSet<String>) -> Claim {
    let mut out = claim.clone();
    out.expressions
        .retain(|e| !(e.page_id == page_id && blocks.contains(&e.block)));
    out
}

// ---------------------------------------------------------------------------------------------- curation

/// What a person changed about a link between a claim and a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEdit {
    /// The source (one the claim cites).
    pub source: SourceId,
    /// Supports or challenges.
    pub relation: Link,
    /// How directly.
    pub bearing: Bearing,
}

/// Who and what a source is, where it is found and which source replaces it, **as it should be** (K44): what only
/// curation changes once a source was checked (K30, K36). The texts are as a person wrote them; an empty one is
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// A title.
    pub title: String,
    /// An author or a publisher.
    pub author: String,
    /// A date, as written (a year, a season, a day).
    pub date: String,
    /// How the reference is written.
    pub reference_form: ReferenceForm,
    /// The reference; empty for none.
    pub reference: String,
    /// The source that replaces it.
    pub superseded_by: Option<SourceId>,
}

impl Work {
    /// What `source` has now, in the words of a form.
    pub fn of(source: &Source) -> Work {
        Work {
            title: source.identity.title.clone().unwrap_or_default(),
            author: source.identity.author.clone().unwrap_or_default(),
            date: source.identity.date.clone().unwrap_or_default(),
            reference_form: source
                .reference
                .as_ref()
                .map_or(ReferenceForm::Url, |r| r.form),
            reference: source
                .reference
                .as_ref()
                .map(|r| r.value.clone())
                .unwrap_or_default(),
            superseded_by: source.superseded_by.clone(),
        }
    }
}

/// Was this source checked by a person (it has a review day or an availability)? Only then is who and what it is, where
/// it is found and which source replaces it curation's (`check::Curated::departure`).
pub fn is_checked(source: &Source) -> bool {
    source.availability != Availability::Unknown || source.last_reviewed.is_some()
}

/// What a person changed about a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEdit {
    /// The source (one the claim cites).
    pub source: SourceId,
    /// The day a person last checked it, written `YYYY-MM-DD`; empty for none.
    pub reviewed: String,
    /// Whether it can be had, as the person who checked says.
    pub availability: Availability,
    /// Who and what it is, where it is found and what replaces it, as it should be; `None` leaves them as they are.
    /// Only for a source that was checked ([`Mistake::NotChecked`]).
    pub work: Option<Work>,
}

/// The curation proposal's form, read: **no page text**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Curation {
    /// What the community decides about the claim.
    pub status: Standing,
    /// Each link, as it should be.
    pub links: Vec<LinkEdit>,
    /// The links the claim lets go (K44): the sources it no longer cites. A link is changed or removed, not both
    /// ([`Mistake::RemovedAndChanged`]).
    pub removed: Vec<SourceId>,
    /// Each source, as it should be.
    pub sources: Vec<SourceEdit>,
    /// Judge the sources and the standing again on the wording the claim has now (the re-confirmation, as a
    /// proposal of its own).
    pub confirm: bool,
}

/// The files a curation changes: a claim and the sources it cites, as they are (`claim`, `cited`) and the
/// digest of the wording the claim has now (`now`, `None` when it is said nowhere). Only a file that differs is
/// returned. The standing's `proposal`, and a source's `last_reviewed` proposal, are left for the acceptance to
/// name (it is the proposal that decides; [`super::curation`]).
pub fn curate(
    claim: &Claim,
    cited: &[Source],
    now: Option<&super::Digest>,
    edit: &Curation,
) -> Result<Vec<(String, String)>, Mistake> {
    let mut next = claim.clone();
    let judged = |why: Mistake| now.ok_or(why);
    let status = &mut next.community_status;
    if edit.status != status.status {
        if edit.status == Standing::Unknown {
            *status = super::CommunityStatus::undecided();
        } else {
            status.status = edit.status;
            status.proposal = None;
            status.judged_on = Some(judged(Mistake::NoWording)?.clone());
        }
    } else if edit.confirm && edit.status != Standing::Unknown && status.judged_on.as_ref() != now {
        status.proposal = None;
        status.judged_on = Some(judged(Mistake::NoWording)?.clone());
    }
    for e in &edit.links {
        if edit.removed.contains(&e.source) {
            let at_base = claim.sources.iter().find(|l| l.source == e.source);
            if at_base.is_none_or(|l| l.relation != e.relation || l.bearing != e.bearing) {
                return Err(Mistake::RemovedAndChanged);
            }
            continue;
        }
        let link = next
            .sources
            .iter_mut()
            .find(|l| l.source == e.source)
            .ok_or(Mistake::UnknownSource)?;
        if link.relation != e.relation || link.bearing != e.bearing {
            link.relation = e.relation;
            link.bearing = e.bearing;
            link.judged_on = judged(Mistake::NoWording)?.clone();
        }
    }
    for gone in &edit.removed {
        let before = next.sources.len();
        next.sources.retain(|l| l.source != *gone);
        if next.sources.len() == before {
            return Err(Mistake::UnknownSource);
        }
    }
    if edit.confirm {
        for link in &mut next.sources {
            link.judged_on = judged(Mistake::NoWording)?.clone();
        }
    }
    next.normalize();

    let mut out = Vec::new();
    if next != *claim {
        let text = next.to_yaml();
        Claim::parse(claim.id.clone(), &text).map_err(|_| Mistake::Unwritable)?;
        out.push((claim_path(&claim.id), text));
    }
    for e in &edit.sources {
        let old = cited
            .iter()
            .find(|s| s.id == e.source)
            .ok_or(Mistake::UnknownSource)?;
        let mut source = old.clone();
        source.availability = e.availability;
        if let Some(work) = e.work.as_ref().filter(|w| **w != Work::of(old)) {
            let mut changed = old.clone();
            set_work(&mut changed, work)?;
            if changed.identity != old.identity
                || changed.reference != old.reference
                || changed.superseded_by != old.superseded_by
            {
                // Who and what an unchecked source is stays the page edit's.
                if !is_checked(old) {
                    return Err(Mistake::NotChecked);
                }
                source.identity = changed.identity;
                source.reference = changed.reference;
                source.superseded_by = changed.superseded_by;
            }
        }
        let day = e.reviewed.trim();
        let before = old.last_reviewed.as_ref().map(|r| r.date.as_str());
        if day.is_empty() {
            source.last_reviewed = None;
        } else if before != Some(day) {
            if !is_day(day) {
                return Err(Mistake::NotADay);
            }
            source.last_reviewed = Some(super::Reviewed {
                date: day.to_owned(),
                proposal: None,
            });
        }
        if source != *old {
            let text = source.to_yaml();
            Source::parse(source.id.clone(), &text).map_err(|_| Mistake::Unwritable)?;
            out.push((source_path(&source.id), text));
        }
    }
    Ok(out)
}

/// `source` with who and what it is, where it is found and what replaces it as `work` says, each text cleaned as a new
/// source's are. It still names a work (a title or a reference), and is not replaced by itself.
fn set_work(source: &mut Source, work: &Work) -> Result<(), Mistake> {
    source.identity = Identity {
        title: clean_text(&work.title, "title")?,
        author: clean_text(&work.author, "author")?,
        date: clean_text(&work.date, "date")?,
    };
    source.reference = clean_reference(work.reference_form, &work.reference)?;
    if work.superseded_by.as_ref() == Some(&source.id) {
        return Err(Mistake::ReplacedByItself);
    }
    if source.identity.title.is_none() && source.reference.is_none() {
        return Err(Mistake::SourceSaysNothing);
    }
    source.superseded_by.clone_from(&work.superseded_by);
    Ok(())
}
