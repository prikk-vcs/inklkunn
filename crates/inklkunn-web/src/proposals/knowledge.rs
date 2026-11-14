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
//! **No two subjects read alike** (K24, K32): a name is not unique (two claims can be said in one paragraph, two
//! sources can be called the same), and an identifier is written by whoever writes the file, so a shortened form could
//! be copied. A claim's and a source's name is therefore followed by its **whole** identifier, in `<code>`, linked to the
//! subject's page (a claim's page; a source's line on the page of a claim it is named for), and wrapping where the line
//! is narrow.

use std::cell::RefCell;
use std::collections::BTreeMap;

use inklkunn_core::knowledge::check::{PageLookup, Pages};
use inklkunn_core::knowledge::index::kept;
use inklkunn_core::knowledge::summary::{
    Cite, ClaimChange, Item, Leaves, Rejudgement, SourceChange,
};
use inklkunn_core::knowledge::{
    ClaimId, Digest, Expression, Family, File, Identity, Reference, Relation, SourceId, read_file,
};
use inklkunn_core::proposal::BaseError;
use inklkunn_core::proposal::files::ProposedPages;
use inklkunn_core::read::{Cause, ReadError};
use inklkunn_store::RevisionContent;

use super::labels::{
    availability_word, bearing_word, lifecycle_word, link_word, reference_option,
    relation_type_word, source_word, standing_word, statement_word,
};
use crate::catalog::{Key, Rich, T, bdi, cite, code, code_link};
use crate::health::rule_key;

/// One thing the proposal does to claims and sources, with the particulars under it. **Markup the catalog built from
/// escaped parts** (`<bdi>` for a paragraph's words, `<cite>` for a source's name), printed as it is.
#[derive(Clone)]
pub(crate) struct Line {
    pub(crate) text: Rich,
    pub(crate) more: Vec<Rich>,
}

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
    /// Is the link of `claim` to `source` that the proposal adds judged on a wording one of the claim's paragraphs has
    /// in the proposal's result (K25)? A proposal that is sent always is; a draft may not be, and then its line says so.
    fn judged_now(&self, claim: &ClaimId, source: &SourceId) -> bool;
    /// The address of the page of `claim`, when the wiki has that claim (a claim the proposal makes has no page yet).
    fn claim_href(&self, claim: &ClaimId) -> Option<String>;
    /// The address of the line of `source` on the page of `claim`: a source has no page of its own.
    fn cited_href(&self, claim: &ClaimId, source: &SourceId) -> Option<String>;
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
/// the places the proposal or the wiki give, that has words. **Followed by its whole identifier** (K32), linked to its
/// page when the wiki has one. A claim said in no paragraph that can be found is named by its identifier alone, which
/// is what is known of it.
fn claim_subject(t: T, know: &dyn Subjects, claim: &ClaimId, own: &[&[Expression]]) -> Rich {
    let others;
    let places: Vec<&Expression> = if own.iter().any(|places| !places.is_empty()) {
        own.iter().flat_map(|places| places.iter()).collect()
    } else {
        others = know.places(claim);
        others.iter().collect()
    };
    let mut excerpt = None;
    for at in places {
        if let Some(words) = know.paragraph(at) {
            let words = self::excerpt(&words, SUBJECT_CHARS);
            if !words.is_empty() {
                excerpt = Some(words);
                break;
            }
        }
    }
    let id = match know.claim_href(claim) {
        Some(href) => code_link(claim.as_str(), &href),
        None => code(claim.as_str()),
    };
    match excerpt {
        Some(words) => t
            .msg(Key::KnowSubjectClaimId)
            .rich("excerpt", bdi(&words))
            .rich("id", id)
            .html(),
        // Named by its identifier already.
        None => t.msg(Key::KnowSubjectClaim).rich("id", id).html(),
    }
}

/// A paragraph, by an excerpt of its words; by its marker when it cannot be found.
fn paragraph_subject(know: &dyn Subjects, at: &Expression) -> Rich {
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
/// each as it was written, joined as the claim page joins them; a source that names itself in no way says so (its
/// identifier follows every name a line prints, K32).
fn source_name(t: T, identity: Option<&Identity>) -> String {
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
        return t.get(Key::KnowSourceNameless);
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

/// A source, by its name, set apart (`<cite>`), **followed by its whole identifier** (K32). `own` is what the item
/// itself says of it; otherwise the proposal's file or the wiki says. `of` is the claim the line is about, when there is
/// one: a source has no page of its own, so its identifier leads to its line on that claim's page.
fn source_subject(
    t: T,
    know: &dyn Subjects,
    id: &SourceId,
    own: Option<&Identity>,
    of: Option<&ClaimId>,
) -> Rich {
    let known;
    let identity = match own {
        Some(identity) => Some(identity),
        None => {
            known = know.identity(id);
            known.as_ref()
        }
    };
    let name = source_name(t, identity);
    let shown = match of.and_then(|claim| know.cited_href(claim, id)) {
        Some(href) => code_link(id.as_str(), &href),
        None => code(id.as_str()),
    };
    t.msg(Key::KnowSubjectSourceId)
        .rich("name", cite(&name))
        .rich("id", shown)
        .html()
}

fn cite_line(t: T, know: &dyn Subjects, claim: &ClaimId, c: &Cite) -> Rich {
    let Cite {
        source,
        relation,
        bearing,
    } = c;
    // A new link is judged on the wording the claim has now (K25): the line says so only when it is true.
    let key = if know.judged_now(claim, source) {
        Key::KnowCite
    } else {
        Key::KnowCiteUnjudged
    };
    t.msg(key)
        .rich("source", source_subject(t, know, source, None, Some(claim)))
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
fn rejudged(
    t: T,
    know: &dyn Subjects,
    claim: &ClaimId,
    said: &[Expression],
    on: &Rejudgement,
    to: Option<&Digest>,
) -> Rich {
    let subject = match on {
        Rejudgement::Standing => t.msg(Key::KnowRejudgedStatus).html(),
        Rejudgement::Source(source) => t
            .msg(Key::KnowRejudgedSource)
            .rich("source", source_subject(t, know, source, None, Some(claim)))
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

fn claim_change(
    t: T,
    know: &dyn Subjects,
    claim: &ClaimId,
    change: &ClaimChange,
    said: &[Expression],
) -> Rich {
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
        ClaimChange::Rejudged { on, to } => rejudged(t, know, claim, said, on, to.as_ref()),
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
        ClaimChange::Cited(c) => cite_line(t, know, claim, c),
        ClaimChange::Uncited(source) => t
            .msg(Key::KnowChangeUncited)
            .rich("source", source_subject(t, know, source, None, Some(claim)))
            .html(),
        ClaimChange::Recited(c) => {
            let Cite {
                source,
                relation,
                bearing,
            } = c;
            t.msg(Key::KnowChangeRecited)
                .rich("source", source_subject(t, know, source, None, Some(claim)))
                .with("relation", t.get(link_word(*relation)))
                .with("bearing", t.get(bearing_word(*bearing)))
                .html()
        }
        ClaimChange::Noted(source) => t
            .msg(Key::KnowChangeNoted)
            .rich("source", source_subject(t, know, source, None, Some(claim)))
            .html(),
    }
}

/// A line about a relation to another claim, which it names.
fn relation_line(t: T, know: &dyn Subjects, key: Key, relation: &Relation) -> Rich {
    let Relation { kind, claim } = relation;
    t.msg(key)
        .rich("other", claim_subject(t, know, claim, &[]))
        .with("relation", t.get(relation_type_word(*kind)))
        .html()
}

fn source_change(t: T, know: &dyn Subjects, change: &SourceChange) -> Rich {
    match change {
        SourceChange::Kind(k) => t
            .msg(Key::KnowSourceChangeKind)
            .with("kind", t.get(source_word(*k)))
            .html(),
        SourceChange::Identity(identity) => t
            .msg(Key::KnowSourceChangeIdentity)
            .rich("identity", cite(&source_name(t, Some(identity))))
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
            .rich("source", source_subject(t, know, source, None, None))
            .html(),
        SourceChange::Superseded(None) => t.msg(Key::KnowSourceChangeUnsuperseded).html(),
    }
}

/// What goes with a removed file: the facts of it that a person decided or checked (K27), one line for each.
fn leaving(t: T, know: &dyn Subjects, claim: Option<&ClaimId>, leaves: &Leaves) -> Vec<Rich> {
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
                    .rich("source", source_subject(t, know, source, None, claim))
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

fn line(t: T, know: &dyn Subjects, item: &Item) -> Line {
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
                    claim,
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
            more.extend(cites.iter().map(|c| cite_line(t, know, claim, c)));
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
                .map(|c| claim_change(t, know, claim, c, said))
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
                        .rich("source", source_subject(t, know, later, None, None))
                        .html(),
                );
            }
            Line {
                text: t
                    .msg(Key::KnowSourceRecorded)
                    .rich(
                        "source",
                        source_subject(t, know, source, Some(identity), None),
                    )
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
                .rich("source", source_subject(t, know, source, Some(named), None))
                .html(),
            more: changes.iter().map(|c| source_change(t, know, c)).collect(),
        },
        Item::Removed { family, id, leaves } => Line {
            text: match family {
                Family::Claim => t
                    .msg(Key::KnowRemovedClaim)
                    .rich(
                        "claim",
                        match ClaimId::parse(id) {
                            Some(claim) => claim_subject(t, know, &claim, &[]),
                            None => t.msg(Key::KnowSubjectClaim).rich("id", code(id)).html(),
                        },
                    )
                    .html(),
                Family::Source => t
                    .msg(Key::KnowRemovedSource)
                    .rich(
                        "source",
                        match SourceId::parse(id) {
                            Some(source) => source_subject(t, know, &source, None, None),
                            None => code(id),
                        },
                    )
                    .html(),
            },
            more: leaves
                .as_ref()
                .map(|leaves| leaving(t, know, ClaimId::parse(id).as_ref(), leaves))
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

/// The lines of a proposal's page for these items.
pub(crate) fn lines(t: T, items: &[Item], know: &dyn Subjects) -> Vec<Line> {
    items.iter().map(|item| line(t, know, item)).collect()
}

/// What a proposal's knowledge files come to against the base they were written on.
pub(crate) struct Known {
    /// The lines of the proposal's page.
    pub(crate) lines: Vec<Line>,
    /// The files that do what only a curation proposal may ([`curated_by`], the acceptance's own rule).
    pub(crate) curated: Vec<inklkunn_core::knowledge::check::Refusal>,
    /// A source link the proposal adds is judged on a wording its claim does not have in the result (K25): the
    /// submission refuses it and the acceptance would.
    pub(crate) unjudged: bool,
}

/// Where the wording a link or a status was judged on is found: the view the proposal is for, the base it was written
/// on and the renames it declares. The pages it writes, removes and moves, with the pages of the base, are the pages
/// as the proposal leaves them (K34: read as the service reads them, [`ProposedPages`]).
pub(crate) struct Place<'a> {
    /// The view the proposal is for.
    pub(crate) view: Option<&'a str>,
    /// The base the proposal was written on.
    pub(crate) base: &'a inklkunn_store::Base,
    /// The renames the proposal declares.
    pub(crate) renames: &'a [inklkunn_store::Rename],
}

/// What names the subjects of a proposal's lines: the proposal's own files first (the pages it writes, the claims and
/// sources it records), then the wiki at the proposal's base. A read that fails is kept, never shown as a missing
/// subject.
struct Resolver<'a> {
    ctx: &'a super::Ctx,
    place: &'a Place<'a>,
    /// The pages of the proposal's result: the one reading the service has ([`ProposedPages`], K34).
    pages: ProposedPages<'a>,
    /// The claims the proposal writes: where each is said.
    claims: BTreeMap<String, Vec<Expression>>,
    /// The sources the proposal writes: what each calls itself.
    sources: BTreeMap<String, Identity>,
    /// The new links whose wording the claim does not have in the result, as (claim, source): found once the lines are
    /// asked for ([`Resolver::judge`]).
    unjudged: Vec<(String, String)>,
    /// The pages of the wiki at the base, by page id (`None`: no one page has it).
    base: RefCell<BTreeMap<String, Option<String>>>,
    failure: RefCell<Option<inklkunn_core::read::ReadError>>,
}

impl<'a> Resolver<'a> {
    fn new(
        ctx: &'a super::Ctx,
        place: &'a Place<'a>,
        content: &'a RevisionContent,
    ) -> Resolver<'a> {
        let mut claims = BTreeMap::new();
        let mut sources = BTreeMap::new();
        for f in &content.files {
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
            }
        }
        Resolver {
            ctx,
            place,
            pages: ProposedPages::new(&*ctx.services.read, &ctx.wiki, content),
            claims,
            sources,
            unjudged: Vec::new(),
            base: RefCell::new(BTreeMap::new()),
            failure: RefCell::new(None),
        }
    }

    /// The view a link to a page of the knowledge layer names: the proposal's, unless it is the wiki's default.
    fn view_name(&self) -> Option<String> {
        super::new::short(self.place.view.filter(|v| *v != self.ctx.wiki.default_view))
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

    /// The text of the page whose `id` is `page_id` as the proposal leaves it (K34): the pages it writes, moves and
    /// removes, else the base's; a page id that no page, or two pages, have there is no page.
    fn result_text(&self, page_id: &str) -> Option<String> {
        match self.pages.with_id(page_id) {
            PageLookup::One(path) => self.pages.text(&path),
            PageLookup::None | PageLookup::Several => None,
        }
    }

    /// The knowledge index **at the proposal's base** (K31), as the pages are read: the claims and sources the wiki had
    /// when the proposal was written, not what it has become. What the proposal makes itself is in `claims` and
    /// `sources`, which come first, so a claim or a source it creates (no anchor, no row at the base) is still named.
    fn index(&self) -> Option<std::sync::Arc<inklkunn_core::knowledge::index::KnowledgeIndex>> {
        match self.ctx.services.read.knowledge_index_at_base(
            &self.ctx.wiki,
            self.place.view,
            &self.place.base.state_id,
            &self.place.base.block_id,
        ) {
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

    fn judged_now(&self, claim: &ClaimId, source: &SourceId) -> bool {
        !self
            .unjudged
            .iter()
            .any(|(c, s)| c == claim.as_str() && s == source.as_str())
    }

    fn claim_href(&self, claim: &ClaimId) -> Option<String> {
        // A claim only the proposal makes has no page until it is accepted: a link would lead to *no such claim*.
        self.index()?.claim(claim.as_str())?;
        Some(crate::knowledge::paths::claim(
            self.ctx.wiki.slug.as_str(),
            claim.as_str(),
            self.view_name().as_deref(),
        ))
    }

    fn cited_href(&self, claim: &ClaimId, source: &SourceId) -> Option<String> {
        self.claim_href(claim)?;
        Some(crate::knowledge::paths::claim_source(
            self.ctx.wiki.slug.as_str(),
            claim.as_str(),
            source.as_str(),
            self.view_name().as_deref(),
        ))
    }
}

/// The pages as the proposal leaves them: the service's own reading ([`ProposedPages`]).
impl Pages for Resolver<'_> {
    fn with_id(&self, id: &str) -> PageLookup {
        self.pages.with_id(id)
    }

    fn text(&self, path: &str) -> Option<String> {
        self.pages.text(path)
    }
}

impl Resolver<'_> {
    /// Find the links the proposal adds that are judged on a wording their claim does not have in the result (K25), by
    /// the one rule the submission and the acceptance use ([`inklkunn_core::knowledge::check::unjudged_by`]).
    fn judge(&mut self, given: &[(String, Option<String>)], was: &dyn Fn(&str) -> Option<String>) {
        let found = inklkunn_core::knowledge::check::unjudged_by(given, was, &*self);
        self.unjudged = found
            .into_iter()
            // The community status names no source: no link line matches it, the page's note still says so.
            .map(|u| {
                (
                    u.claim.to_string(),
                    u.source.map_or_else(String::new, |s| s.to_string()),
                )
            })
            .collect();
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
    let content = RevisionContent {
        base: place.base.clone(),
        files: files.to_vec(),
        renames: place.renames.to_vec(),
        minor: false,
        automated: false,
    };
    let mut resolver = Resolver::new(ctx, place, &content);
    resolver.judge(&given, &was);
    let unjudged = !resolver.unjudged.is_empty();
    let lines = lines(t, &items, &resolver);
    if let Some(e) = resolver.failure.take() {
        return Err(e);
    }
    // A base that could not answer the service's reading of the pages is a read that failed, as any other.
    match resolver.pages.failure.take() {
        Some(BaseError::NoSuchView(view)) => Err(ReadError::NoSuchView(view)),
        Some(BaseError::Unavailable(why)) => Err(ReadError::CouldNotBeReadNow(Cause::Scratch(
            format!("the pages of a proposal at its base: {why}"),
        ))),
        None => Ok(Known {
            lines,
            curated,
            unjudged,
        }),
    }
}

#[cfg(test)]
mod tests;
