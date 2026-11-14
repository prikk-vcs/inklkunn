//! The curation proposal's form (RFC 005 part 2 D-3): its own small form, with no page text, for a claim's
//! **community status**, how each of its sources relates to it and how directly, when a person last checked each
//! source, and whether it can be had. It is the only form that sets any of these, and the only proposal the
//! acceptance lets set a community status (`inklkunn_core::knowledge::curation`): a status is a recorded decision of
//! people, never a count or a timer.
//!
//! The claim and its sources are read **as the base the form opened on had them** and the wording they are judged
//! on is the claim's wording at that base, so what is proposed is exactly what the person saw. A field left as it
//! was changes nothing; a proposal that changes nothing is not sent. The proposal is an ordinary one: created and
//! sent for review through the service, accepted by someone who may, and the status it sets names it.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Action;
use inklkunn_core::knowledge::compose::{
    Curation, LinkEdit, Mistake, SourceEdit, Work, curate, is_checked, page_id_of,
};
use inklkunn_core::knowledge::marker::words_with_id;
use inklkunn_core::knowledge::{
    AnchorState, Availability, Bearing, Claim, ClaimId, Digest, File, Link, ReferenceForm, Source,
    SourceId, Standing, anchor_state, claim_path, read_file, source_path,
};
use inklkunn_core::proposal::{Invalid, ProposalEdit, ProposalError};
use inklkunn_core::read::ReadError;
use inklkunn_store::{Base, FileChange};

use super::claims::mistake_text;
use super::knowledge::{Line, Place, file_lines};
use super::labels::{
    OptionView, availability_word, bearing_option, link_word, options, reference_option,
    standing_word,
};
use super::new::{Button, Hidden, read_failure, short, view_of};
use super::render::LinkView;
use super::{
    Ctx, fields, fields_of, from_this_site, needs_account, notice_page, paths, refused, see, serve,
    words,
};
use crate::catalog::Key;
use crate::csrf::Decided;
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::wiki::{Choice, Shell, generic_not_found, services_of};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;
type Fields = super::urlencoded::Raw;

// ------------------------------------------------------------------------------------ the claim as it was

/// The claim, its sources and its wording, as a base had them.
pub(crate) struct Loaded {
    claim: Claim,
    /// Each source the claim cites (`None`: its file is not there or cannot be read), in the claim's order.
    cited: Vec<Option<Source>>,
    /// The claim's words, and the digest of them, in the first paragraph it is said in at the base.
    wording: Option<(String, Digest)>,
}

impl Loaded {
    /// The claim.
    pub(crate) fn claim(&self) -> &Claim {
        &self.claim
    }

    /// The digest of the wording the claim has, when it is said in a paragraph.
    pub(crate) fn digest(&self) -> Option<&Digest> {
        self.wording.as_ref().map(|(_, d)| d)
    }

    /// The sources the claim cites that could be read, in the claim's order.
    pub(crate) fn cited(&self) -> Vec<Source> {
        self.cited.iter().flatten().cloned().collect()
    }

    /// The edits that would record, as the day a person looked at a source, a day that has not come (after the
    /// current date in UTC+14 by the service's clock, K49), each with its place among `edits`. A day left as the base
    /// has it is not a new record, and a text that is no day is `Mistake::NotADay`'s business, not this one's.
    pub(crate) fn days_not_come(
        &self,
        read: &inklkunn_core::read::ReadService,
        edits: &[SourceEdit],
    ) -> Vec<DayNotCome> {
        edits
            .iter()
            .enumerate()
            .filter_map(|(at, e)| {
                let source = self.cited.iter().flatten().find(|s| s.id == e.source)?;
                let kept = source.last_reviewed.as_ref().map(|r| r.date.as_str());
                (!e.reviewed.is_empty()
                    && kept != Some(e.reviewed.as_str())
                    && read.day_has_not_come(&e.reviewed))
                .then(|| DayNotCome {
                    at,
                    source: e.source.clone(),
                    title: source.identity.title.clone(),
                    day: e.reviewed.clone(),
                })
            })
            .collect()
    }
}

/// A day that has not come, given as the day a source was looked at (K49).
pub(crate) struct DayNotCome {
    /// Its place among the edits of the body.
    pub(crate) at: usize,
    /// The source.
    pub(crate) source: SourceId,
    /// The source's title, when it has one.
    pub(crate) title: Option<String>,
    /// The day, as it was written.
    pub(crate) day: String,
}

impl DayNotCome {
    /// The refusal in the person's words: the source (as the form's own legend names it) and the day.
    pub(crate) fn text(&self, t: crate::catalog::T) -> String {
        let title = self
            .title
            .clone()
            .unwrap_or_else(|| t.get(Key::CurateSourceUntitled));
        t.msg(Key::CurateDayNotCome)
            .with("title", title)
            .with("day", &self.day)
            .text()
    }
}

/// Read the claim `id` and what it cites at `base_block` of `view`. `Ok(None)`: there is no such claim then.
pub(super) fn load(
    ctx: &Ctx,
    view: Option<&str>,
    base_block: &str,
    id: &ClaimId,
) -> Result<Option<Loaded>, ReadError> {
    load_with(&ctx.services.read, &ctx.wiki, view, base_block, id)
}

/// [`load`], for a caller that has the read service and the wiki and no form's context (the write API's curation).
pub(crate) fn load_with(
    read: &inklkunn_core::read::ReadService,
    wiki: &inklkunn_store::WikiRecord,
    view: Option<&str>,
    base_block: &str,
    id: &ClaimId,
) -> Result<Option<Loaded>, ReadError> {
    let path = claim_path(id);
    let Some(text) = read.source_at(wiki, base_block, &path)? else {
        return Ok(None);
    };
    let Ok(File::Claim(claim)) = read_file(&path, text.as_bytes()) else {
        return Ok(None);
    };
    let mut cited = Vec::new();
    for link in &claim.sources {
        let path = source_path(&link.source);
        cited.push(read.source_at(wiki, base_block, &path)?.and_then(|text| {
            match read_file(&path, text.as_bytes()) {
                Ok(File::Source(source)) => Some(*source),
                _ => None,
            }
        }));
    }
    // Where the claim is said: the index of the tip names the page of an expression; its text is read at the base.
    let index = read.knowledge_index(wiki, view)?;
    let mut wording = None;
    for e in &claim.expressions {
        let Some(at) = index
            .anchors_of(id.as_str())
            .find(|a| a.page_id == e.page_id && a.block == e.block)
            .and_then(|a| a.page_path.clone())
        else {
            continue;
        };
        let Some(page) = read.source_at(wiki, base_block, &at)? else {
            continue;
        };
        if page_id_of(&page).as_deref() != Some(e.page_id.as_str()) {
            continue;
        }
        if let AnchorState::Present(digest) = anchor_state(&page, &e.block) {
            let words = words_with_id(&page, &e.block, true).unwrap_or_default();
            wording = Some((words, digest));
            break;
        }
    }
    Ok(Some(Loaded {
        claim: *claim,
        cited,
        wording,
    }))
}

/// Does `edit` name, as the source that replaces another, one the base does not have or cannot read (K44)? Only a
/// successor the base does not already name is looked for: the acceptance refuses a reference to a source that is not
/// there, so the proposal is never one that cannot be accepted. Shared with the write API's curation.
pub(crate) fn unknown_successor_with(
    read: &inklkunn_core::read::ReadService,
    wiki: &inklkunn_store::WikiRecord,
    base_block: &str,
    subject: &Loaded,
    edit: &Curation,
) -> Result<bool, ReadError> {
    for e in &edit.sources {
        let Some(later) = e.work.as_ref().and_then(|w| w.superseded_by.as_ref()) else {
            continue;
        };
        let named = subject
            .cited
            .iter()
            .flatten()
            .find(|s| s.id == e.source)
            .and_then(|s| s.superseded_by.as_ref());
        if named == Some(later) {
            continue;
        }
        let path = source_path(later);
        let found = read
            .source_at(wiki, base_block, &path)?
            .is_some_and(|text| matches!(read_file(&path, text.as_bytes()), Ok(File::Source(_))));
        if !found {
            return Ok(true);
        }
    }
    Ok(false)
}

fn unknown_successor(
    ctx: &Ctx,
    base_block: &str,
    subject: &Loaded,
    edit: &Curation,
) -> Result<bool, ReadError> {
    unknown_successor_with(&ctx.services.read, &ctx.wiki, base_block, subject, edit)
}

// ------------------------------------------------------------------------------------ what the form says

/// What a post of the form says.
struct Submitted {
    base: Base,
    view: Option<String>,
    claim: ClaimId,
    status: Standing,
    links: Vec<LinkEdit>,
    /// The links whose box says the claim lets them go.
    removed: Vec<SourceId>,
    sources: Vec<SourceEdit>,
    /// What a checked source is, as the form sent it, by source.
    works: Vec<(SourceId, WorkText)>,
    confirm: bool,
    summary: String,
    op: Op,
}

/// What a checked source is, where it is found and what replaces it, as the form carries it: texts, with the successor
/// as typed (a text that is no identifier is said so on the form, not taken for an incomplete post).
#[derive(Clone, PartialEq, Eq)]
struct WorkText {
    title: String,
    author: String,
    date: String,
    reference_form: ReferenceForm,
    reference: String,
    superseded: String,
}

impl WorkText {
    /// As the source has them.
    fn of(source: &Source) -> WorkText {
        let work = Work::of(source);
        WorkText {
            title: work.title,
            author: work.author,
            date: work.date,
            reference_form: work.reference_form,
            reference: work.reference,
            superseded: work
                .superseded_by
                .map(|s| s.to_string())
                .unwrap_or_default(),
        }
    }

    /// The core's words for it: the successor, if typed, must be an identifier.
    fn work(&self) -> Result<Work, Mistake> {
        let superseded = self.superseded.trim();
        Ok(Work {
            title: self.title.clone(),
            author: self.author.clone(),
            date: self.date.clone(),
            reference_form: self.reference_form,
            reference: self.reference.clone(),
            superseded_by: if superseded.is_empty() {
                None
            } else {
                Some(SourceId::parse(superseded).ok_or(Mistake::UnknownSuccessor)?)
            },
        })
    }
}

/// A browser takes the line breaks out of the value of a one-line field, so a text that has one in the base comes back
/// without it. Where the text sent is the base's with its breaks taken out, the person changed nothing: it is the
/// base's (K59).
fn unbroken(sent: &mut Work, was: &Work) {
    let keep = |sent: &mut String, was: &str| {
        if sent != was && *sent == was.replace(['\n', '\r'], "") {
            was.clone_into(sent);
        }
    };
    keep(&mut sent.title, &was.title);
    keep(&mut sent.author, &was.author);
    keep(&mut sent.date, &was.date);
    keep(&mut sent.reference, &was.reference);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Changes,
    Submit,
}

/// The fields of a post, in the core's words; anything that is not what the form offers is `None`.
fn read(fields: &HashMap<String, String>) -> Option<Submitted> {
    let get = |name: &str| fields.get(name).map(String::as_str);
    let count = get(fields::LINKS)?.parse::<usize>().ok()?.min(256);
    let mut links = Vec::new();
    let mut removed = Vec::new();
    let mut sources = Vec::new();
    let mut works = Vec::new();
    for i in 0..count {
        let one = |what: &str| get(&fields::curated_field(i, what));
        let source = SourceId::parse(one(fields::C_SOURCE)?)?;
        links.push(LinkEdit {
            source: source.clone(),
            relation: Link::from_word(one(fields::C_RELATION)?)?,
            bearing: Bearing::from_word(one(fields::C_BEARING)?)?,
        });
        if one(fields::C_REMOVE) == Some("1") {
            removed.push(source.clone());
        }
        // A source whose file the form could not show is not asked about.
        if let Some(availability) = one(fields::C_AVAILABILITY) {
            sources.push(SourceEdit {
                source: source.clone(),
                reviewed: one(fields::C_REVIEWED).unwrap_or("").trim().to_owned(),
                availability: Availability::from_word(availability)?,
                work: None,
            });
        }
        // What a source is: offered for a source whose file could be read, so a form without it says nothing. The texts
        // are kept **as sent** (K59): the core compares each with what the source has and cleans only one that differs,
        // so a title the form carried back with a space at its end is not a change.
        if let Some(title) = one(fields::C_TITLE) {
            let text = |what: &str| one(what).unwrap_or("").to_owned();
            works.push((
                source,
                WorkText {
                    title: title.to_owned(),
                    author: text(fields::C_AUTHOR),
                    date: text(fields::C_DATE),
                    reference_form: ReferenceForm::from_word(one(fields::C_REF_FORM)?)?,
                    reference: text(fields::C_REF),
                    superseded: text(fields::C_SUPERSEDED),
                },
            ));
        }
    }
    Some(Submitted {
        base: Base::new(get(fields::BASE_STATE)?, get(fields::BASE_BLOCK)?)?,
        view: get(fields::VIEW)
            .filter(|v| !v.is_empty())
            .map(str::to_owned),
        claim: ClaimId::parse(get(fields::CLAIM)?)?,
        status: Standing::from_word(get(fields::STATUS)?)?,
        links,
        removed,
        sources,
        works,
        confirm: get(fields::CONFIRM) == Some("1"),
        summary: get(fields::SUMMARY).unwrap_or("").trim().to_owned(),
        op: match get(fields::OP)? {
            fields::OP_CHANGES => Op::Changes,
            fields::OP_SUBMIT => Op::Submit,
            _ => return None,
        },
    })
}

// ------------------------------------------------------------------------------------ the page

struct SourceView {
    legend: String,
    earlier: Option<String>,
    relation_name: String,
    relation_label: String,
    relations: Vec<OptionView>,
    bearing_name: String,
    bearing_label: String,
    bearings: Vec<OptionView>,
    remove_name: String,
    remove_label: String,
    remove_hint: String,
    removed: bool,
    checkable: bool,
    work: Option<WorkView>,
    reviewed_name: String,
    reviewed_label: String,
    reviewed_hint: String,
    reviewed: String,
    availability_name: String,
    availability_label: String,
    availability_hint: String,
    availabilities: Vec<OptionView>,
}

/// The fields of what a checked source is (K44).
struct WorkView {
    hint: String,
    title_name: String,
    title_label: String,
    title: String,
    author_name: String,
    author_label: String,
    author: String,
    date_name: String,
    date_label: String,
    date_hint: String,
    date: String,
    ref_form_name: String,
    ref_form_label: String,
    ref_forms: Vec<OptionView>,
    ref_name: String,
    ref_label: String,
    ref_hint: String,
    reference: String,
    superseded_name: String,
    superseded_label: String,
    superseded_hint: String,
    superseded: String,
}

#[derive(Template)]
#[template(path = "curate.html")]
struct CurateTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    problems: Vec<String>,
    intro: String,
    wording: String,
    action: String,
    hidden: Vec<Hidden>,
    status_name: &'static str,
    status_label: String,
    status_hint: String,
    statuses: Vec<OptionView>,
    status_earlier: Option<String>,
    sources: Vec<SourceView>,
    confirm_name: &'static str,
    confirm_label: String,
    confirm_hint: String,
    confirm_checked: bool,
    summary_label: String,
    summary_hint: String,
    summary_max: usize,
    summary: String,
    buttons: Vec<Button>,
    op_name: &'static str,
    changes_heading: String,
    changes: Vec<Line>,
    back: LinkView,
}

/// The values the form shows: those the wiki has, or those a person sent.
struct Shown {
    status: Standing,
    links: Vec<(Link, Bearing)>,
    reviewed: Vec<String>,
    availability: Vec<Availability>,
    /// Which links are to go, by link.
    removed: Vec<bool>,
    /// What each checked source is, by link; `None` for one that was not checked (the form does not offer it).
    works: Vec<Option<WorkText>>,
    confirm: bool,
    summary: String,
}

impl Shown {
    /// As the base has them.
    fn of(subject: &Loaded) -> Shown {
        Shown {
            status: subject.claim.community_status.status,
            links: subject
                .claim
                .sources
                .iter()
                .map(|l| (l.relation, l.bearing))
                .collect(),
            reviewed: subject
                .cited
                .iter()
                .map(|s| {
                    s.as_ref()
                        .and_then(|s| s.last_reviewed.as_ref())
                        .map(|r| r.date.clone())
                        .unwrap_or_default()
                })
                .collect(),
            availability: subject
                .cited
                .iter()
                .map(|s| s.as_ref().map_or(Availability::Unknown, |s| s.availability))
                .collect(),
            removed: vec![false; subject.claim.sources.len()],
            works: subject
                .cited
                .iter()
                .map(|s| s.as_ref().filter(|s| is_checked(s)).map(WorkText::of))
                .collect(),
            confirm: false,
            summary: String::new(),
        }
    }

    /// As a person sent them.
    fn sent(subject: &Loaded, sub: &Submitted) -> Shown {
        let mut shown = Shown::of(subject);
        shown.status = sub.status;
        for (i, link) in sub.links.iter().enumerate() {
            if let Some(slot) = shown.links.get_mut(i) {
                *slot = (link.relation, link.bearing);
            }
        }
        for e in &sub.sources {
            if let Some(i) = subject
                .claim
                .sources
                .iter()
                .position(|l| l.source == e.source)
            {
                if let Some(slot) = shown.reviewed.get_mut(i) {
                    slot.clone_from(&e.reviewed);
                }
                if let Some(slot) = shown.availability.get_mut(i) {
                    *slot = e.availability;
                }
            }
        }
        for (i, link) in subject.claim.sources.iter().enumerate() {
            if let Some(slot) = shown.removed.get_mut(i) {
                *slot = sub.removed.contains(&link.source);
            }
            if let Some((_, sent)) = sub.works.iter().find(|(s, _)| *s == link.source)
                && let Some(slot) = shown.works.get_mut(i)
            {
                *slot = Some(sent.clone());
            }
        }
        shown.confirm = sub.confirm;
        shown.summary.clone_from(&sub.summary);
        shown
    }
}

/// What a form is drawn from: the claim as a base had it, that base, the view and the values to show.
struct Draw<'a> {
    subject: &'a Loaded,
    base: &'a Base,
    view: Option<&'a str>,
    shown: &'a Shown,
}

fn render(
    page: &Page,
    ctx: &Ctx,
    draw: &Draw<'_>,
    problems: Vec<String>,
    changes: Vec<Line>,
    status: StatusCode,
) -> Response {
    let Draw {
        subject,
        base,
        view,
        shown,
    } = *draw;
    let t = page.t();
    let slug = ctx.slug();
    let now = subject.wording.as_ref().map(|(_, d)| d);
    let earlier = |judged: Option<&Digest>| {
        judged
            .is_some_and(|d| Some(d) != now)
            .then(|| t.get(Key::CurateEarlier))
    };
    let mut hidden = vec![
        Hidden {
            name: fields::BASE_STATE.to_owned(),
            value: base.state_id.clone(),
        },
        Hidden {
            name: fields::BASE_BLOCK.to_owned(),
            value: base.block_id.clone(),
        },
        Hidden {
            name: fields::CLAIM.to_owned(),
            value: subject.claim.id.to_string(),
        },
        Hidden {
            name: fields::LINKS.to_owned(),
            value: subject.claim.sources.len().to_string(),
        },
    ];
    if let Some(view) = view {
        hidden.push(Hidden {
            name: fields::VIEW.to_owned(),
            value: view.to_owned(),
        });
    }
    let label = |k: Key| t.get(k);
    let mut sources = Vec::new();
    for (i, link) in subject.claim.sources.iter().enumerate() {
        let name = |what: &str| fields::curated_field(i, what);
        hidden.push(Hidden {
            name: name(fields::C_SOURCE),
            value: link.source.to_string(),
        });
        let source = subject.cited.get(i).and_then(Option::as_ref);
        let title = source
            .and_then(|s| s.identity.title.clone())
            .unwrap_or_else(|| t.get(Key::CurateSourceUntitled));
        let (relation, bearing) = shown
            .links
            .get(i)
            .copied()
            .unwrap_or((link.relation, link.bearing));
        sources.push(SourceView {
            legend: t.msg(Key::CurateSourceLegend).with("title", title).text(),
            earlier: earlier(Some(&link.judged_on)),
            relation_name: name(fields::C_RELATION),
            relation_label: t.get(Key::CurateLabelRelation),
            relations: options(Link::ALL, Link::word, link_word, relation, label),
            bearing_name: name(fields::C_BEARING),
            bearing_label: t.get(Key::CurateLabelBearing),
            bearings: options(Bearing::ALL, Bearing::word, bearing_option, bearing, label),
            remove_name: name(fields::C_REMOVE),
            remove_label: t.get(Key::CurateLabelRemove),
            remove_hint: t.get(Key::CurateRemoveHint),
            removed: shown.removed.get(i).copied().unwrap_or(false),
            checkable: source.is_some(),
            work: shown
                .works
                .get(i)
                .and_then(Option::as_ref)
                .map(|w| WorkView {
                    hint: t.get(Key::CurateWorkHint),
                    title_name: name(fields::C_TITLE),
                    title_label: t.get(Key::AttachLabelTitle),
                    title: w.title.clone(),
                    author_name: name(fields::C_AUTHOR),
                    author_label: t.get(Key::AttachLabelAuthor),
                    author: w.author.clone(),
                    date_name: name(fields::C_DATE),
                    date_label: t.get(Key::AttachLabelDate),
                    date_hint: t.get(Key::AttachHintDate),
                    date: w.date.clone(),
                    ref_form_name: name(fields::C_REF_FORM),
                    ref_form_label: t.get(Key::AttachLabelRefForm),
                    ref_forms: options(
                        ReferenceForm::ALL,
                        ReferenceForm::word,
                        reference_option,
                        w.reference_form,
                        label,
                    ),
                    ref_name: name(fields::C_REF),
                    ref_label: t.get(Key::AttachLabelRef),
                    ref_hint: t.get(Key::AttachHintRef),
                    reference: w.reference.clone(),
                    superseded_name: name(fields::C_SUPERSEDED),
                    superseded_label: t.get(Key::CurateLabelSuperseded),
                    superseded_hint: t.get(Key::CurateSupersededHint),
                    superseded: w.superseded.clone(),
                }),
            reviewed_name: name(fields::C_REVIEWED),
            reviewed_label: t.get(Key::CurateLabelReviewed),
            reviewed_hint: t.get(Key::CurateReviewedHint),
            reviewed: shown.reviewed.get(i).cloned().unwrap_or_default(),
            availability_name: name(fields::C_AVAILABILITY),
            availability_label: t.get(Key::CurateLabelAvailability),
            availability_hint: t.get(Key::CurateAvailabilityHint),
            availabilities: options(
                Availability::ALL,
                Availability::word,
                availability_word,
                shown
                    .availability
                    .get(i)
                    .copied()
                    .unwrap_or(Availability::Unknown),
                label,
            ),
        });
    }
    let wording = match &subject.wording {
        Some((words, _)) => t.msg(Key::CurateWording).with("words", words).text(),
        None => t.get(Key::CurateWordingNone),
    };
    let head = page.wiki_head(&ctx.wiki, Subject::Wiki);
    let shell = Shell::of(&ctx.wiki);
    let rendered = CurateTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleCurate)
            .with("wiki", ctx.wiki.display_name.as_str())
            .product()
            .text(),
        heading: t.get(Key::CurateHeading),
        problems,
        intro: t.get(Key::CurateIntro),
        wording,
        action: paths::curate_action(slug),
        hidden,
        status_name: fields::STATUS,
        status_label: t.get(Key::CurateLabelStatus),
        status_hint: t.get(Key::CurateStatusHint),
        statuses: options(
            Standing::ALL,
            Standing::word,
            standing_word,
            shown.status,
            label,
        ),
        status_earlier: earlier(subject.claim.community_status.judged_on.as_ref()),
        sources,
        confirm_name: fields::CONFIRM,
        confirm_label: t.get(Key::CurateConfirm),
        confirm_hint: t.get(Key::CurateConfirmHint),
        confirm_checked: shown.confirm,
        summary_label: t
            .msg(Key::ProposeLabelSummary)
            .with("limit", inklkunn_core::proposal::MAX_SUMMARY_CHARS)
            .text(),
        summary_hint: t.get(Key::ProposeSummaryHint),
        summary_max: inklkunn_core::proposal::MAX_SUMMARY_CHARS,
        summary: shown.summary.clone(),
        buttons: vec![
            Button {
                value: fields::OP_CHANGES,
                label: t.get(Key::ProposeShowChanges),
            },
            Button {
                value: fields::OP_SUBMIT,
                label: t.get(Key::CurateSubmit),
            },
        ],
        op_name: fields::OP,
        changes_heading: t.get(Key::KnowHeading),
        changes,
        back: LinkView {
            href: paths::list(slug),
            label: t.get(Key::ProposalsBackToList),
        },
    }
    .render();
    answer(status, &head, rendered)
}

// ------------------------------------------------------------------------------------ the entry points

/// `GET /{slug}/-/proposals/curate?claim=<id>[&view=]`: the form, filled in with what the wiki has now.
pub(crate) async fn curate_form(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    params: Params,
    slug: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
) -> Response {
    let (Ok(axum::extract::Path(slug)), Some(services)) = (slug, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        if !ctx.may_wiki(Action::Propose) {
            return refused(page, &ctx, &ProposalError::NotAllowed, Vec::new());
        }
        let Ok(full_view) = view_of(params.get(fields::VIEW).map(String::as_str)) else {
            return generic_not_found(page);
        };
        let Some(id) = params.get(fields::CLAIM).and_then(|c| ClaimId::parse(c)) else {
            return generic_not_found(page);
        };
        let base = match ctx
            .services
            .proposals
            .capture_base(&ctx.wiki, full_view.as_deref())
        {
            Ok(base) => base,
            Err(e) => return refused(page, &ctx, &e, Vec::new()),
        };
        let subject = match load(&ctx, full_view.as_deref(), &base.block_id, &id) {
            Ok(Some(subject)) => subject,
            Ok(None) => return generic_not_found(page),
            Err(e) => return read_failure(page, &ctx, e),
        };
        let shown = Shown::of(&subject);
        render(
            page,
            &ctx,
            &Draw {
                subject: &subject,
                base: &base,
                view: short(full_view.as_deref()).as_deref(),
                shown: &shown,
            },
            Vec::new(),
            Vec::new(),
            StatusCode::OK,
        )
    })
    .await
}

/// `POST /{slug}/-/proposals/curate`.
pub(crate) async fn curate_post(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    decided: Decided,
    slug: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
    form: Fields,
) -> Response {
    if let Some(response) = from_this_site(&page, &headers) {
        return response;
    }
    let (Ok(axum::extract::Path(slug)), Some(services)) = (slug, services_of(&state)) else {
        return generic_not_found(&page);
    };
    serve(page, move |page| {
        let Some(ctx) = Ctx::open_deciding(&services, &decided, &slug) else {
            return generic_not_found(page);
        };
        if let Some(response) = needs_account(page, &ctx) {
            return response;
        }
        match fields_of(page, &ctx, &headers, form) {
            Ok(fields) => post(page, &ctx, &fields),
            Err(response) => *response,
        }
    })
    .await
}

fn post(page: &Page, ctx: &Ctx, fields: &HashMap<String, String>) -> Response {
    let t = page.t();
    let Some(sub) = read(fields) else {
        return notice_page(
            page,
            ctx,
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::RefusedInvalidHeading,
            vec![t.get(Key::InvalidIncomplete)],
            vec![Choice {
                label: t.get(Key::ProposalsBackToList),
                href: paths::list(ctx.slug()),
            }],
        );
    };
    if !ctx.may_wiki(Action::Propose) {
        return refused(page, ctx, &ProposalError::NotAllowed, Vec::new());
    }
    let Ok(full_view) = view_of(sub.view.as_deref()) else {
        return generic_not_found(page);
    };
    let subject = match load(ctx, full_view.as_deref(), &sub.base.block_id, &sub.claim) {
        Ok(Some(subject)) => subject,
        Ok(None) => return generic_not_found(page),
        Err(e) => return read_failure(page, ctx, e),
    };
    // The links the form carries are the claim's, in the claim's order.
    if sub.links.len() != subject.claim.sources.len()
        || sub
            .links
            .iter()
            .zip(&subject.claim.sources)
            .any(|(e, l)| e.source != l.source)
    {
        return notice_page(
            page,
            ctx,
            StatusCode::UNPROCESSABLE_ENTITY,
            Key::RefusedInvalidHeading,
            vec![t.get(Key::InvalidIncomplete)],
            Vec::new(),
        );
    }
    let cited: Vec<Source> = subject.cited.iter().flatten().cloned().collect();
    let shown = Shown::sent(&subject, &sub);
    // What a checked source is, in the core's words (a successor that is no identifier is said so, on the form).
    let mut sources = sub.sources.clone();
    let mut mistake = None;
    for (id, text) in &sub.works {
        match text.work() {
            Ok(mut work) => {
                if let Some(old) = cited.iter().find(|s| s.id == *id) {
                    unbroken(&mut work, &Work::of(old));
                }
                if let Some(edit) = sources.iter_mut().find(|e| e.source == *id) {
                    edit.work = Some(work);
                }
            }
            Err(m) => mistake = Some(m),
        }
    }
    let edit = Curation {
        status: sub.status,
        links: sub.links.clone(),
        removed: sub.removed.clone(),
        sources,
        confirm: sub.confirm,
    };
    let now = subject.wording.as_ref().map(|(_, d)| d);
    let view_name = short(full_view.as_deref())
        .filter(|_| full_view.as_deref() != Some(ctx.wiki.default_view.as_str()));
    let draw = |problems: Vec<String>, changes: Vec<Line>, status: StatusCode| {
        render(
            page,
            ctx,
            &Draw {
                subject: &subject,
                base: &sub.base,
                view: view_name.as_deref(),
                shown: &shown,
            },
            problems,
            changes,
            status,
        )
    };
    // A day that has not come is refused at its field, before anything is made (K49).
    let not_come = subject.days_not_come(&ctx.services.read, &edit.sources);
    if !not_come.is_empty() {
        return draw(
            not_come.iter().map(|d| d.text(t)).collect(),
            Vec::new(),
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    // A source that replaces another is a source the wiki has, as the base has it.
    let missing = match mistake {
        Some(m) => Some(m),
        None => match unknown_successor(ctx, &sub.base.block_id, &subject, &edit) {
            Ok(missing) => missing.then_some(Mistake::UnknownSuccessor),
            Err(e) => return read_failure(page, ctx, e),
        },
    };
    let files = match missing.map_or_else(|| curate(&subject.claim, &cited, now, &edit), Err) {
        Ok(files) => files,
        Err(m) => {
            return draw(
                vec![mistake_text(t, m)],
                Vec::new(),
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    };
    // The lines the proposal's page will have, re-judgements with the wording they move to included.
    let changes = {
        let given: Vec<FileChange> = files
            .iter()
            .map(|(p, text)| FileChange {
                path: p.clone(),
                text: Some(text.clone()),
            })
            .collect();
        let place = Place {
            view: full_view.as_deref(),
            base: &sub.base,
            renames: &[],
        };
        match file_lines(ctx, t, &place, &given) {
            Ok(known) => known.lines,
            Err(e) => return read_failure(page, ctx, e),
        }
    };
    if sub.op == Op::Changes {
        return draw(Vec::new(), changes, StatusCode::OK);
    }
    let mut problems = Vec::new();
    if sub.summary.is_empty() {
        problems.push(words::invalid_text(t, Invalid::SummaryMissing));
    }
    if files.is_empty() {
        problems.push(t.get(Key::CurateNothing));
    }
    if !problems.is_empty() {
        return draw(problems, changes, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let proposed = ProposalEdit {
        summary: sub.summary.clone(),
        files: files
            .into_iter()
            .map(|(path, text)| FileChange {
                path,
                text: Some(text),
            })
            .collect(),
        base: Some(sub.base.clone()),
        ..ProposalEdit::default()
    };
    let service = &ctx.services.proposals;
    // Made and sent in one step (K38): a refusal leaves nothing, never an unsent draft that counts against its author.
    match service.create_and_submit(&ctx.principal, &ctx.wiki, full_view.as_deref(), proposed) {
        Ok(made) => see(&paths::proposal(ctx.slug(), made.id.as_str())),
        Err(ProposalError::Invalid(why)) => draw(
            vec![words::invalid_text(t, why)],
            changes,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(error) => refused(page, ctx, &error, Vec::new()),
    }
}
