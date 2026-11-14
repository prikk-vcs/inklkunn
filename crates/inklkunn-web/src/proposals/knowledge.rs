//! What a proposal does to the knowledge layer, in the catalog's words (RFC 005 part 2 D-3): the claims it makes,
//! the sources it records and what it changes of those that exist, as the lines a proposal's page lists. **Facts,
//! not verdicts**: no word here says a claim or a source is good, true or settled, and a change of a community
//! status is said as the change it is.
//!
//! **The page says everything the core describes, and which** (handoff 005 corrections, second round, K15): every
//! line names its subject, a claim by an excerpt of a paragraph it is said in (escaped, in `<bdi>`: it is page text)
//! and a source by its name (title, author, date, as the claim page names it, in `<cite>`: a title written to read like
//! another source's line is still one source's name), and a new claim's standing, a new source's availability and
//! review date, and a changed source's new identity and reference are on their lines. **No arm drops a field with
//! `..`**: a field added to an [`Item`] does not compile until this says what the page does with it.
//!
//! **No two subjects read alike** (K24): a name is not unique (two claims can be said in one paragraph, two sources can
//! be called the same), so a claim's and a source's name is followed by its identifier, shortened ([`names`]).

use std::cell::RefCell;
use std::collections::BTreeMap;

use inklkunn_core::knowledge::index::kept;
use inklkunn_core::knowledge::summary::{
    Cite, ClaimChange, Item, Leaves, Rejudgement, SourceChange,
};
use inklkunn_core::knowledge::{
    ClaimId, Digest, Expression, Family, File, Identity, Reference, Relation, SourceId, read_file,
};

use super::labels::{
    availability_word, bearing_word, lifecycle_word, link_word, reference_option,
    relation_type_word, source_word, standing_word, statement_word,
};
use crate::catalog::{Key, Rich, T, bdi, cite, code};
use crate::health::rule_key;

/// One thing the proposal does to claims and sources, with the particulars under it. **Markup the catalog built from
/// escaped parts** (`<bdi>` for a paragraph's words, `<cite>` for a source's name), printed as it is.
#[derive(Clone)]
pub(crate) struct Line {
    pub(crate) text: Rich,
    pub(crate) more: Vec<Rich>,
}

mod names;

use names::{Kind, Names};

/// What the lines need to name what they are about and to show the wording they speak of. The proposal's own files
/// come first, then the wiki.
pub(crate) trait Subjects {
    /// The wording a digest names, among the places a claim is said: `None` when no paragraph of them has that wording.
    fn wording(&self, said: &[Expression], digest: &Digest) -> Option<String>;
    /// The words of the paragraph a claim is said in (marker left out), as the proposal leaves it, else as the wiki had
    /// it.
    fn paragraph(&self, at: &Expression) -> Option<String>;
    /// Where a claim is said: in the proposal's own file for it, else in the wiki.
    fn places(&self, claim: &ClaimId) -> Vec<Expression>;
    /// What a source calls itself: in the proposal's own file for it, else in the wiki.
    fn identity(&self, source: &SourceId) -> Option<Identity>;
}

/// What one page of lines is written with: what names the subjects, and what the lines have named so far (K24).
struct Page<'a> {
    subjects: &'a dyn Subjects,
    names: Names,
}

impl Page<'_> {
    fn wording(&self, said: &[Expression], digest: &Digest) -> Option<String> {
        self.subjects.wording(said, digest)
    }
    fn paragraph(&self, at: &Expression) -> Option<String> {
        self.subjects.paragraph(at)
    }
    fn places(&self, claim: &ClaimId) -> Vec<Expression> {
        self.subjects.places(claim)
    }
    fn identity(&self, source: &SourceId) -> Option<Identity> {
        self.subjects.identity(source)
    }
}

/// The most characters of the wording a re-judgement moved to that a line prints.
const WORDING_CHARS: usize = 240;

/// The most characters of a paragraph that name a claim on a line.
const SUBJECT_CHARS: usize = 80;

/// Words as a line prints them: their white space made single spaces, and cut at `limit` characters with a mark that
/// they were. Whole, not their first line: two wordings that differ after a line break must not read alike.
fn excerpt(words: &str, limit: usize) -> String {
    let one_line = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = one_line.chars().take(limit).collect();
    if one_line.chars().count() > limit {
        out.push('…');
    }
    out
}

/// A claim, by an excerpt of a paragraph it is said in: the first of `own` (the places the item itself names) and then
/// the places the proposal or the wiki give, that has words. A claim said in no paragraph that can be found is named
/// by its identifier, which is what is known of it. **A claim whose name is another claim's on the page also carries
/// its identifier** (K24).
fn claim_subject(t: T, know: &Page, claim: &ClaimId, own: &[&[Expression]]) -> Rich {
    let others;
    let places: Vec<&Expression> = if own.iter().any(|places| !places.is_empty()) {
        own.iter().flat_map(|places| places.iter()).collect()
    } else {
        others = know.places(claim);
        others.iter().collect()
    };
    let mut name = claim.to_string();
    for at in places {
        if let Some(words) = know.paragraph(at) {
            let words = excerpt(&words, SUBJECT_CHARS);
            if !words.is_empty() {
                name = words;
                break;
            }
        }
    }
    know.names.note(Kind::Claim, claim.as_str());
    // A claim named by its identifier is told by it already.
    let told = know
        .names
        .short(Kind::Claim, claim.as_str())
        .filter(|_| name != claim.as_str());
    match told {
        Some(id) => t
            .msg(Key::KnowSubjectClaimId)
            .rich("excerpt", bdi(&name))
            .rich("id", code(&id))
            .html(),
        None => t
            .msg(Key::KnowSubjectClaim)
            .rich("excerpt", bdi(&name))
            .html(),
    }
}

/// A paragraph, by an excerpt of its words; by its marker when it cannot be found.
fn paragraph_subject(know: &Page, at: &Expression) -> Rich {
    match know
        .paragraph(at)
        .map(|words| excerpt(&words, SUBJECT_CHARS))
    {
        Some(words) if !words.is_empty() => bdi(&words),
        _ => bdi(&format!("^{}", at.block)),
    }
}

/// A text a source gave of itself, on one line and cut as the index cuts it; `None` when nothing is left of it.
fn one_line(text: Option<&str>) -> Option<String> {
    let line = text?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then(|| kept(&line))
}

/// What a source is called: its title (else *a source without a title*), then its author or publisher and its date,
/// each as it was written, joined as the claim page joins them; a source that names itself in no way is told by its
/// identifier, since nothing else would tell it from another.
fn source_name(t: T, identity: Option<&Identity>, id: &SourceId) -> String {
    let (title, author, date) = match identity {
        Some(Identity {
            title,
            author,
            date,
        }) => (
            one_line(title.as_deref()),
            one_line(author.as_deref()),
            one_line(date.as_deref()),
        ),
        None => (None, None, None),
    };
    if title.is_none() && author.is_none() && date.is_none() {
        return t.msg(Key::KnowSourceNameless).with("id", id).text();
    }
    let first = match &title {
        Some(title) => t.msg(Key::ClaimSourceTitled).with("title", title).text(),
        None => t.get(Key::ClaimSourceUntitled),
    };
    let parts = [
        author.map(|a| t.msg(Key::ClaimSourceBy).with("author", a).text()),
        date.map(|d| t.msg(Key::ClaimSourceDated).with("date", d).text()),
    ];
    parts.into_iter().flatten().fold(first, |name, part| {
        t.msg(Key::ClaimSourceJoin)
            .with("first", name)
            .with("second", part)
            .text()
    })
}

/// A source, by its name, set apart (`<cite>`). `own` is what the item itself says of it; otherwise the proposal's
/// file or the wiki says. **A source whose name is another source's on the page also carries its identifier** (K24).
fn source_subject(t: T, know: &Page, id: &SourceId, own: Option<&Identity>) -> Rich {
    let known;
    let identity = match own {
        Some(identity) => Some(identity),
        None => {
            known = know.identity(id);
            known.as_ref()
        }
    };
    let name = source_name(t, identity, id);
    know.names.note(Kind::Source, id.as_str());
    // A source with no name at all is told by its identifier already.
    let nameless = identity.is_none_or(|i| {
        [&i.title, &i.author, &i.date]
            .iter()
            .all(|part| one_line(part.as_deref()).is_none())
    });
    let told = know
        .names
        .short(Kind::Source, id.as_str())
        .filter(|_| !nameless);
    match told {
        Some(short) => t
            .msg(Key::KnowSubjectSourceId)
            .rich("name", cite(&name))
            .rich("id", code(&short))
            .html(),
        None => cite(&name),
    }
}

fn cite_line(t: T, know: &Page, c: &Cite) -> Rich {
    let Cite {
        source,
        relation,
        bearing,
    } = c;
    t.msg(Key::KnowCite)
        .rich("source", source_subject(t, know, source, None))
        .with("relation", t.get(link_word(*relation)))
        .with("bearing", t.get(bearing_word(*bearing)))
        .html()
}

fn reference_text(t: T, reference: &Reference) -> (String, Rich) {
    let Reference { form, value } = reference;
    (
        t.get(reference_option(*form)),
        bdi(&one_line(Some(value)).unwrap_or_default()),
    )
}

/// A re-judgement: what a status, or the link to a source, was judged on is now this wording.
fn rejudged(t: T, know: &Page, said: &[Expression], on: &Rejudgement, to: Option<&Digest>) -> Rich {
    let subject = match on {
        Rejudgement::Standing => t.msg(Key::KnowRejudgedStatus).html(),
        Rejudgement::Source(source) => t
            .msg(Key::KnowRejudgedSource)
            .rich("source", source_subject(t, know, source, None))
            .html(),
    };
    match to {
        None => t
            .msg(Key::KnowChangeRejudgedCleared)
            .rich("subject", subject)
            .html(),
        Some(digest) => match know.wording(said, digest) {
            Some(words) => t
                .msg(Key::KnowChangeRejudged)
                .rich("subject", subject)
                .rich("words", bdi(&excerpt(&words, WORDING_CHARS)))
                .html(),
            None => t
                .msg(Key::KnowChangeRejudgedUnseen)
                .rich("subject", subject)
                .html(),
        },
    }
}

fn claim_change(t: T, know: &Page, change: &ClaimChange, said: &[Expression]) -> Rich {
    match change {
        ClaimChange::Kind(k) => t
            .msg(Key::KnowChangeKind)
            .with("kind", t.get(statement_word(*k)))
            .html(),
        ClaimChange::Lifecycle(l) => t
            .msg(Key::KnowChangeLifecycle)
            .with("lifecycle", t.get(lifecycle_word(*l)))
            .html(),
        ClaimChange::Standing { from, to } => t
            .msg(Key::KnowChangeStanding)
            .with("from", t.get(standing_word(*from)))
            .with("to", t.get(standing_word(*to)))
            .html(),
        ClaimChange::Rejudged { on, to } => rejudged(t, know, said, on, to.as_ref()),
        ClaimChange::Said(at) => t
            .msg(Key::KnowChangeSaid)
            .rich("paragraph", paragraph_subject(know, at))
            .html(),
        ClaimChange::Unsaid(at) => t
            .msg(Key::KnowChangeUnsaid)
            .rich("paragraph", paragraph_subject(know, at))
            .html(),
        ClaimChange::Related(r) => relation_line(t, know, Key::KnowChangeRelated, r),
        ClaimChange::Unrelated(r) => relation_line(t, know, Key::KnowChangeUnrelated, r),
        ClaimChange::Cited(c) => cite_line(t, know, c),
        ClaimChange::Uncited(source) => t
            .msg(Key::KnowChangeUncited)
            .rich("source", source_subject(t, know, source, None))
            .html(),
        ClaimChange::Recited(c) => {
            let Cite {
                source,
                relation,
                bearing,
            } = c;
            t.msg(Key::KnowChangeRecited)
                .rich("source", source_subject(t, know, source, None))
                .with("relation", t.get(link_word(*relation)))
                .with("bearing", t.get(bearing_word(*bearing)))
                .html()
        }
        ClaimChange::Noted(source) => t
            .msg(Key::KnowChangeNoted)
            .rich("source", source_subject(t, know, source, None))
            .html(),
    }
}

/// A line about a relation to another claim, which it names.
fn relation_line(t: T, know: &Page, key: Key, relation: &Relation) -> Rich {
    let Relation { kind, claim } = relation;
    t.msg(key)
        .rich("other", claim_subject(t, know, claim, &[]))
        .with("relation", t.get(relation_type_word(*kind)))
        .html()
}

fn source_change(t: T, know: &Page, of: &SourceId, change: &SourceChange) -> Rich {
    match change {
        SourceChange::Kind(k) => t
            .msg(Key::KnowSourceChangeKind)
            .with("kind", t.get(source_word(*k)))
            .html(),
        SourceChange::Identity(identity) => t
            .msg(Key::KnowSourceChangeIdentity)
            .rich("identity", cite(&source_name(t, Some(identity), of)))
            .html(),
        SourceChange::Reference(Some(reference)) => {
            let (form, value) = reference_text(t, reference);
            t.msg(Key::KnowSourceChangeReference)
                .with("form", form)
                .rich("reference", value)
                .html()
        }
        SourceChange::Reference(None) => t.msg(Key::KnowSourceChangeReferenceCleared).html(),
        SourceChange::Snapshot => t.msg(Key::KnowSourceChangeSnapshot).html(),
        SourceChange::Reviewed(Some(day)) => {
            t.msg(Key::KnowSourceChangeReviewed).with("day", day).html()
        }
        SourceChange::Reviewed(None) => t.msg(Key::KnowSourceChangeUnreviewed).html(),
        SourceChange::Availability(a) => t
            .msg(Key::KnowSourceChangeAvailability)
            .with("availability", t.get(availability_word(*a)))
            .html(),
        SourceChange::Superseded(Some(source)) => t
            .msg(Key::KnowSourceChangeSuperseded)
            .rich("source", source_subject(t, know, source, None))
            .html(),
        SourceChange::Superseded(None) => t.msg(Key::KnowSourceChangeUnsuperseded).html(),
    }
}

/// What goes with a removed file: the facts of it that a person decided or checked (K27), one line for each.
fn leaving(t: T, know: &Page, leaves: &Leaves) -> Vec<Rich> {
    match leaves {
        Leaves::Claim { standing, cites } => {
            let mut more = vec![
                t.msg(Key::KnowRemovedHadStanding)
                    .with("standing", t.get(standing_word(*standing)))
                    .html(),
            ];
            more.extend(cites.iter().map(|c| {
                let Cite {
                    source,
                    relation,
                    bearing,
                } = c;
                t.msg(Key::KnowRemovedHadLink)
                    .rich("source", source_subject(t, know, source, None))
                    .with("relation", t.get(link_word(*relation)))
                    .with("bearing", t.get(bearing_word(*bearing)))
                    .html()
            }));
            more
        }
        Leaves::Source {
            reviewed,
            availability,
        } => {
            let mut more = Vec::new();
            if let Some(day) = reviewed {
                more.push(t.msg(Key::KnowRemovedHadReviewed).with("day", day).html());
            }
            more.push(
                t.msg(Key::KnowRemovedHadAvailability)
                    .with("availability", t.get(availability_word(*availability)))
                    .html(),
            );
            more
        }
    }
}

fn line(t: T, know: &Page, item: &Item) -> Line {
    match item {
        Item::ClaimMade {
            claim,
            kind,
            standing,
            judged_on,
            lifecycle,
            said,
            relations,
            cites,
        } => {
            let mut more = vec![
                t.msg(Key::KnowMadeLifecycle)
                    .with("lifecycle", t.get(lifecycle_word(*lifecycle)))
                    .html(),
            ];
            if let Some(digest) = judged_on {
                more.push(rejudged(
                    t,
                    know,
                    said,
                    &Rejudgement::Standing,
                    Some(digest),
                ));
            }
            more.extend(relations.iter().map(|relation| {
                let Relation { kind, claim } = relation;
                t.msg(Key::KnowMadeRelated)
                    .rich("other", claim_subject(t, know, claim, &[]))
                    .with("relation", t.get(relation_type_word(*kind)))
                    .html()
            }));
            more.extend(cites.iter().map(|c| cite_line(t, know, c)));
            Line {
                text: t
                    .msg(Key::KnowClaimMade)
                    .rich("claim", claim_subject(t, know, claim, &[said]))
                    .with("kind", t.get(statement_word(*kind)))
                    .with("standing", t.get(standing_word(*standing)))
                    .n(said.len() as u64)
                    .html(),
                more,
            }
        }
        Item::ClaimChanged {
            claim,
            said,
            was_said,
            changes,
        } => Line {
            text: t
                .msg(Key::KnowClaimChanged)
                .rich("claim", claim_subject(t, know, claim, &[said, was_said]))
                .html(),
            more: changes
                .iter()
                .map(|c| claim_change(t, know, c, said))
                .collect(),
        },
        Item::SourceRecorded {
            source,
            kind,
            identity,
            reference,
            snapshot,
            reviewed,
            superseded_by,
            availability,
        } => {
            let mut more = Vec::new();
            if let Some(reference) = reference {
                let (form, value) = reference_text(t, reference);
                more.push(
                    t.msg(Key::KnowSourceWhere)
                        .with("form", form)
                        .rich("reference", value)
                        .html(),
                );
            }
            more.push(
                t.msg(Key::KnowSourceMadeAvailability)
                    .with("availability", t.get(availability_word(*availability)))
                    .html(),
            );
            if let Some(day) = reviewed {
                more.push(t.msg(Key::KnowSourceChangeReviewed).with("day", day).html());
            }
            if *snapshot {
                more.push(t.msg(Key::KnowSourceCopy).html());
            }
            if let Some(later) = superseded_by {
                more.push(
                    t.msg(Key::KnowSourceChangeSuperseded)
                        .rich("source", source_subject(t, know, later, None))
                        .html(),
                );
            }
            Line {
                text: t
                    .msg(Key::KnowSourceRecorded)
                    .rich("source", source_subject(t, know, source, Some(identity)))
                    .with("kind", t.get(source_word(*kind)))
                    .html(),
                more,
            }
        }
        Item::SourceChanged {
            source,
            named,
            changes,
        } => Line {
            text: t
                .msg(Key::KnowSourceChanged)
                .rich("source", source_subject(t, know, source, Some(named)))
                .html(),
            more: changes
                .iter()
                .map(|c| source_change(t, know, source, c))
                .collect(),
        },
        Item::Removed { family, id, leaves } => Line {
            text: match family {
                Family::Claim => t
                    .msg(Key::KnowRemovedClaim)
                    .rich(
                        "claim",
                        match ClaimId::parse(id) {
                            Some(claim) => claim_subject(t, know, &claim, &[]),
                            None => t.msg(Key::KnowSubjectClaim).rich("excerpt", bdi(id)).html(),
                        },
                    )
                    .html(),
                Family::Source => t
                    .msg(Key::KnowRemovedSource)
                    .rich(
                        "source",
                        match SourceId::parse(id) {
                            Some(source) => source_subject(t, know, &source, None),
                            None => cite(id),
                        },
                    )
                    .html(),
            },
            more: leaves
                .as_ref()
                .map(|leaves| leaving(t, know, leaves))
                .unwrap_or_default(),
        },
        Item::UnreadableReplaced { family, id } => Line {
            text: t
                .msg(match family {
                    Family::Claim => Key::KnowReplacedUnreadableClaim,
                    Family::Source => Key::KnowReplacedUnreadableSource,
                })
                .rich("id", bdi(id))
                .html(),
            more: Vec::new(),
        },
        // The name says which kind of file it was meant to be.
        Item::Unreadable {
            family: _,
            id,
            rule,
        } => Line {
            text: t
                .msg(Key::KnowUnreadable)
                .rich("file", bdi(id))
                .with("reason", t.get(rule_key(*rule)))
                .html(),
            more: Vec::new(),
        },
    }
}

/// The lines of a proposal's page for these items. **Read twice**: the first pass learns every name the lines use, so
/// that the second can tell the subjects whose names are alike apart (K24).
pub(crate) fn lines(t: T, items: &[Item], know: &dyn Subjects) -> Vec<Line> {
    let page = Page {
        subjects: know,
        names: Names::default(),
    };
    for item in items {
        line(t, &page, item);
    }
    page.names.settle();
    items.iter().map(|item| line(t, &page, item)).collect()
}

/// What a proposal's knowledge files come to against the base they were written on.
pub(crate) struct Known {
    /// The lines of the proposal's page.
    pub(crate) lines: Vec<Line>,
    /// The files that do what only a curation proposal may ([`curated_by`], the acceptance's own rule).
    pub(crate) curated: Vec<inklkunn_core::knowledge::check::Refusal>,
}

/// Where the wording a re-judgement moves to is found: the view the proposal is for and the base it was written on. A
/// re-judgement is a curation proposal's, which writes no page, so the pages at the base are the pages as the proposal
/// leaves them.
pub(crate) struct Place<'a> {
    /// The view the proposal is for.
    pub(crate) view: Option<&'a str>,
    /// The base the proposal was written on.
    pub(crate) base: &'a inklkunn_store::Base,
}

/// What names the subjects of a proposal's lines: the proposal's own files first (the pages it writes, the claims and
/// sources it records), then the wiki at the proposal's base. A read that fails is kept, never shown as a missing
/// subject.
struct Resolver<'a> {
    ctx: &'a super::Ctx,
    place: &'a Place<'a>,
    /// The pages the proposal writes, by page id.
    pages: Vec<(String, String)>,
    /// The claims the proposal writes: where each is said.
    claims: BTreeMap<String, Vec<Expression>>,
    /// The sources the proposal writes: what each calls itself.
    sources: BTreeMap<String, Identity>,
    /// The pages of the wiki at the base, by page id (`None`: no one page has it).
    base: RefCell<BTreeMap<String, Option<String>>>,
    failure: RefCell<Option<inklkunn_core::read::ReadError>>,
}

impl<'a> Resolver<'a> {
    fn new(
        ctx: &'a super::Ctx,
        place: &'a Place<'a>,
        files: &[inklkunn_store::FileChange],
    ) -> Resolver<'a> {
        let mut pages = Vec::new();
        let mut claims = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for f in files {
            let Some(text) = &f.text else { continue };
            if inklkunn_core::knowledge::family_of(&f.path).is_some() {
                match read_file(&f.path, text.as_bytes()) {
                    Ok(File::Claim(c)) => {
                        claims.insert(c.id.to_string(), c.expressions);
                    }
                    Ok(File::Source(s)) => {
                        sources.insert(s.id.to_string(), s.identity);
                    }
                    Err(_) => {}
                }
            } else if let Some(id) = inklkunn_markup::parse_metadata(text)
                .ok()
                .and_then(|m| m.id)
                .filter(|id| !id.is_empty())
            {
                pages.push((id, text.clone()));
            }
        }
        Resolver {
            ctx,
            place,
            pages,
            claims,
            sources,
            base: RefCell::new(BTreeMap::new()),
            failure: RefCell::new(None),
        }
    }

    fn failed(&self, e: inklkunn_core::read::ReadError) {
        self.failure.borrow_mut().get_or_insert(e);
    }

    /// The text of the page whose `id` is `page_id`, at the proposal's base.
    fn base_text(&self, page_id: &str) -> Option<String> {
        if let Some(found) = self.base.borrow().get(page_id) {
            return found.clone();
        }
        let found = self.read_base(page_id);
        self.base
            .borrow_mut()
            .insert(page_id.to_owned(), found.clone());
        found
    }

    /// The text of the page whose `id` is `page_id` at the proposal's base. **Where the page is is read at the base
    /// too** (K22): a page renamed since is at its base path there; the tip's path would name nothing, or another page.
    fn read_base(&self, page_id: &str) -> Option<String> {
        let read = &self.ctx.services.read;
        let titles = match read.title_index_at_base(
            &self.ctx.wiki,
            self.place.view,
            &self.place.base.state_id,
            &self.place.base.block_id,
        ) {
            Ok(titles) => titles,
            Err(e) => {
                self.failed(e);
                return None;
            }
        };
        let mut at = titles
            .entries()
            .iter()
            .filter(|e| e.page_id.as_deref() == Some(page_id));
        let (Some(entry), None) = (at.next(), at.next()) else {
            return None;
        };
        match read.source_at(&self.ctx.wiki, &self.place.base.block_id, &entry.path) {
            Ok(text) => text,
            Err(e) => {
                self.failed(e);
                None
            }
        }
    }

    /// The text of the page whose `id` is `page_id` as the proposal leaves it: what it writes, else what the base has.
    fn result_text(&self, page_id: &str) -> Option<String> {
        self.pages
            .iter()
            .find(|(id, _)| id == page_id)
            .map(|(_, text)| text.clone())
            .or_else(|| self.base_text(page_id))
    }

    fn index(&self) -> Option<std::sync::Arc<inklkunn_core::knowledge::index::KnowledgeIndex>> {
        match self
            .ctx
            .services
            .read
            .knowledge_index(&self.ctx.wiki, self.place.view)
        {
            Ok(index) => Some(index),
            Err(e) => {
                self.failed(e);
                None
            }
        }
    }
}

impl Subjects for Resolver<'_> {
    fn wording(&self, said: &[Expression], digest: &Digest) -> Option<String> {
        use inklkunn_core::knowledge::{AnchorState, anchor_state, marker::words_with_id};
        said.iter().find_map(|e| {
            let text = self.result_text(&e.page_id)?;
            (anchor_state(&text, &e.block) == AnchorState::Present(digest.clone()))
                .then(|| words_with_id(&text, &e.block, true))
                .flatten()
        })
    }

    fn paragraph(&self, at: &Expression) -> Option<String> {
        use inklkunn_core::knowledge::marker::words_with_id;
        // As the proposal leaves it; a paragraph it takes out is named by the words it had.
        self.result_text(&at.page_id)
            .and_then(|text| words_with_id(&text, &at.block, true))
            .or_else(|| {
                self.base_text(&at.page_id)
                    .and_then(|text| words_with_id(&text, &at.block, true))
            })
    }

    fn places(&self, claim: &ClaimId) -> Vec<Expression> {
        if let Some(places) = self.claims.get(claim.as_str()) {
            return places.clone();
        }
        let Some(index) = self.index() else {
            return Vec::new();
        };
        index
            .anchors_of(claim.as_str())
            .filter_map(|a| Expression::new(&a.page_id, &a.block).ok())
            .collect()
    }

    fn identity(&self, source: &SourceId) -> Option<Identity> {
        if let Some(identity) = self.sources.get(source.as_str()) {
            return Some(identity.clone());
        }
        let index = self.index()?;
        index.source(source.as_str()).map(|row| Identity {
            title: row.title.clone(),
            author: row.author.clone(),
            date: row.date.clone(),
        })
    }
}

/// The lines for the knowledge files (`files`: path and new text, `None` for a removal) of a proposal written on
/// `place`: each file is compared with the text the base had. The pages among `files` and the claims and sources it
/// writes name the subjects of the lines before the wiki does.
pub(crate) fn file_lines(
    ctx: &super::Ctx,
    t: T,
    place: &Place<'_>,
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
                .source_at(&ctx.wiki, &place.base.block_id, &f.path)?,
        );
        given.push((f.path.clone(), f.text.clone()));
    }
    let was = |path: &str| before.get(path).cloned().flatten();
    let items = inklkunn_core::knowledge::summary::summarize(&given, &was);
    let curated = inklkunn_core::knowledge::check::curated_by(&given, &was);
    // The subjects and the wording a re-judgement moved to are looked for only when there is a line to name them; a
    // page that cannot be read is the page that says so, never a line that claims the paragraph is not there.
    let resolver = Resolver::new(ctx, place, files);
    let lines = lines(t, &items, &resolver);
    match resolver.failure.into_inner() {
        Some(e) => Err(e),
        None => Ok(Known { lines, curated }),
    }
}

#[cfg(test)]
mod tests;
