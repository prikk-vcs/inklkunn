//! The curation proposal over the API (RFC 005 part 2 D-3, D-12; handoff 005 K-T11, K44): a proposal of its own, with no
//! page text, for a claim's community status, how each source bears on it (or that a link goes), when a person last
//! checked each source and whether it can be had, and, for a source that was checked, who and what it is, where it is
//! found and which source replaces it. It is the form's proposal (`proposals::curate`), made by the same core function ([`curate`]) from the claim and
//! its sources **as the base the caller named has them**, and judged by the same rules at submit, at every revision and at
//! acceptance; this only reads a JSON body instead of a form. The service is the one write path.
//!
//! A member left out is left as the base has it; one that equals what the base has changes nothing; a body that changes
//! nothing is refused. A link is changed with `relation` and `bearing`, or let go with `"remove": true` and nothing else;
//! in a source, an empty text is nothing. Nothing here sets a status: a status is set by a proposal that people accept.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::knowledge::compose::{Curation, LinkEdit, Mistake, SourceEdit, Work, curate};
use inklkunn_core::knowledge::{
    Availability, Bearing, ClaimId, Link, ReferenceForm, SourceId, Standing,
};
use inklkunn_core::proposal::{Invalid, ProposalEdit};
use inklkunn_store::FileChange;

use super::answer;
use crate::api::describe;
use crate::api::problem::{Code, Problem};
use crate::api::reply::Reply;
use crate::api::request::from_read;
use crate::api::write::body::{Fields, invalid};
use crate::api::write::{Request, serve_write};
use crate::catalog::Key;
use crate::csrf::Decided;
use crate::pages::PageState;
use crate::proposals::claims::mistake_text;
use crate::proposals::curate::{load_with, unknown_successor_with};
use crate::url::view_ref;

/// The most links or sources one body edits (the form's bound).
const MAX_EDITS: usize = 256;

/// `POST /{slug}/-/api/v1/proposals/curation`.
pub(crate) async fn create(
    State(state): State<PageState>,
    headers: HeaderMap,
    decided: Decided,
    uri: Uri,
    body: Body,
) -> Response {
    serve_write(
        state,
        headers,
        decided,
        uri,
        body,
        &describe::PROPOSE_CURATION,
        opened,
    )
    .await
}

/// A word of the format, from a member of the body: any other is `invalid-body` at that member.
fn word<T>(
    fields: &mut Fields,
    name: &str,
    parse: impl Fn(&str) -> Option<T>,
    at: &str,
) -> Result<T, Problem> {
    parse(&fields.text(name)?).ok_or_else(|| invalid(at))
}

fn opened(req: &Request, mut fields: Fields) -> Result<Reply, Problem> {
    let claim = fields.text("claim")?;
    let claim = ClaimId::parse(&claim).ok_or_else(|| invalid("claim"))?;
    let summary = fields.text("summary")?;
    let view = fields.opt_text("view")?;
    let submit = fields.flag("submit")?;
    let status = fields
        .opt_text("status")?
        .map(|w| Standing::from_word(&w).ok_or_else(|| invalid("status")))
        .transpose()?;
    let mut links = Vec::new();
    let mut removed = Vec::new();
    for (i, mut link) in fields.objects("links", MAX_EDITS)?.into_iter().enumerate() {
        let at = format!("links[{i}]");
        let source = word(
            &mut link,
            "source",
            SourceId::parse,
            &format!("{at}.source"),
        )?;
        // A link that goes names nothing else: how it relates is not asked of a link the claim lets go.
        if link.opt_flag("remove")? {
            link.end()?;
            removed.push(source);
            continue;
        }
        let relation = word(
            &mut link,
            "relation",
            Link::from_word,
            &format!("{at}.relation"),
        )?;
        let bearing = word(
            &mut link,
            "bearing",
            Bearing::from_word,
            &format!("{at}.bearing"),
        )?;
        link.end()?;
        links.push(LinkEdit {
            source,
            relation,
            bearing,
        });
    }
    let mut sources = Vec::new();
    let mut named = Vec::new();
    for (i, mut source) in fields
        .objects("sources", MAX_EDITS)?
        .into_iter()
        .enumerate()
    {
        let at = format!("sources[{i}]");
        let id = word(
            &mut source,
            "source",
            SourceId::parse,
            &format!("{at}.source"),
        )?;
        // Both are left as the base has them when they are left out (K59); `""` and `unknown` clear.
        let reviewed = source
            .opt_text("reviewed")?
            .map(|day| day.trim().to_owned());
        let availability = source
            .opt_text("availability")?
            .map(|w| {
                Availability::from_word(&w).ok_or_else(|| invalid(&format!("{at}.availability")))
            })
            .transpose()?;
        let work = WorkSent {
            title: source.opt_text("title")?,
            author: source.opt_text("author")?,
            date: source.opt_text("date")?,
            reference: match source.object("reference")? {
                None => None,
                Some(mut reference) => {
                    let form = word(
                        &mut reference,
                        "type",
                        ReferenceForm::from_word,
                        &format!("{at}.reference.type"),
                    )?;
                    let value = reference.text("value")?;
                    reference.end()?;
                    Some((form, value))
                }
            },
            superseded_by: source
                .opt_text("superseded_by")?
                .map(|text| {
                    let text = text.trim();
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        SourceId::parse(text)
                            .map(Some)
                            .ok_or_else(|| invalid(&format!("{at}.superseded_by")))
                    }
                })
                .transpose()?,
        };
        source.end()?;
        named.push((id, reviewed, availability, work));
    }
    let confirm = fields.opt_flag("confirm")?;
    fields.end()?;

    let full = match view.as_deref() {
        None => None,
        Some(name) => {
            Some(view_ref(name).ok_or_else(|| Problem::new(Code::NoSuchView).named("view", name))?)
        }
    };
    // The base: the tip the caller wrote against, if it is still the tip.
    let tip = super::tip_base(req, full.as_deref())?;
    if submit && summary.trim().is_empty() {
        return Err(req.unacceptable(Invalid::SummaryMissing));
    }

    // The claim and its sources as the base has them; a claim the base does not hold is `not-found`.
    let subject = load_with(
        &req.services.read,
        &req.wiki,
        full.as_deref(),
        &tip.block_id,
        &claim,
    )
    .map_err(|e| from_read(&req.services, &req.wiki, e))?
    .ok_or_else(|| Problem::new(Code::NotFound))?;
    // The members of a source that say who and what it is are left as the base has them when they are left out.
    let cited = subject.cited();
    for (id, reviewed, availability, work) in named {
        let old = cited.iter().find(|s| s.id == id);
        sources.push(SourceEdit {
            reviewed: reviewed.unwrap_or_else(|| {
                old.and_then(|s| s.last_reviewed.as_ref())
                    .map(|r| r.date.clone())
                    .unwrap_or_default()
            }),
            availability: availability
                .unwrap_or_else(|| old.map_or(Availability::Unknown, |s| s.availability)),
            work: old.and_then(|old| work.over(Work::of(old))),
            source: id,
        });
    }
    let edit = Curation {
        status: status.unwrap_or(subject.claim().community_status.status),
        links,
        removed,
        sources,
        confirm,
    };
    // A day that has not come is refused at its member, before anything is made (K49).
    if let Some(day) = subject
        .days_not_come(&req.services.read, &edit.sources)
        .first()
    {
        return Err(req
            .unacceptable_with("day-not-come", day.text(req.t))
            .named("member", format!("sources[{}].reviewed", day.at))
            .member("source", serde_json::json!(day.source.to_string())));
    }
    // A source that replaces another is a source the wiki has, as the base has it.
    if unknown_successor_with(
        &req.services.read,
        &req.wiki,
        &tip.block_id,
        &subject,
        &edit,
    )
    .map_err(|e| from_read(&req.services, &req.wiki, e))?
    {
        return Err(mistake(req, Mistake::UnknownSuccessor));
    }
    let files =
        curate(subject.claim(), &cited, subject.digest(), &edit).map_err(|m| mistake(req, m))?;
    if files.is_empty() {
        return Err(req.unacceptable_with("nothing-to-curate", req.t.get(Key::CurateNothing)));
    }

    let edit = ProposalEdit {
        summary,
        files: files
            .into_iter()
            .map(|(path, text)| FileChange {
                path,
                text: Some(text),
            })
            .collect(),
        base: Some(tip),
        ..ProposalEdit::default()
    };
    let service = &req.services.proposals;
    // Sent: made and sent in one step (K47, as the form's, K38), so a refusal leaves nothing behind, never a draft that
    // counts against the caller and that they would have to go and find.
    let (record, revision) = if submit {
        let sent = service
            .create_and_submit(&req.principal, &req.wiki, full.as_deref(), edit)
            .map_err(|error| req.refusal(error))?;
        (sent, 1)
    } else {
        let draft = service
            .create(&req.principal, &req.wiki, full.as_deref(), edit)
            .map_err(|error| req.refusal(error))?;
        (draft, 0)
    };
    Ok(answer::reply(&record, revision, req.account())?.created(req.proposal_path(&record.id)))
}

/// What the core refused to write, as the problem the form's words say. A source the claim does not cite is a source
/// the base does not show; a defect is the server's.
fn mistake(req: &Request, m: Mistake) -> Problem {
    let reason = match m {
        Mistake::UnknownSource => "unknown-source",
        Mistake::NoWording => "no-wording",
        Mistake::NotADay => "not-a-day",
        Mistake::RemovedAndChanged => "removed-and-changed",
        Mistake::NotChecked => "not-checked",
        Mistake::ReplacedByItself => "replaced-by-itself",
        Mistake::UnknownSuccessor => "unknown-successor",
        Mistake::Unwritable => return Problem::new(Code::Failed),
        _ => "not-curatable",
    };
    req.unacceptable_with(reason, mistake_text(req.t, m))
}

/// The members of a source that say who and what it is, where it is found and what replaces it, as a body gave them: a
/// member left out is not changed.
struct WorkSent {
    title: Option<String>,
    author: Option<String>,
    date: Option<String>,
    reference: Option<(ReferenceForm, String)>,
    /// `Some(None)`: nothing replaces it.
    superseded_by: Option<Option<SourceId>>,
}

impl WorkSent {
    /// `first` (what the base has) with the members the body gave; `None` when it gave none.
    fn over(self, first: Work) -> Option<Work> {
        let none = self.title.is_none()
            && self.author.is_none()
            && self.date.is_none()
            && self.reference.is_none()
            && self.superseded_by.is_none();
        if none {
            return None;
        }
        let (reference_form, reference) = self
            .reference
            .unwrap_or((first.reference_form, first.reference));
        Some(Work {
            title: self.title.unwrap_or(first.title),
            author: self.author.unwrap_or(first.author),
            date: self.date.unwrap_or(first.date),
            reference_form,
            reference,
            superseded_by: self.superseded_by.unwrap_or(first.superseded_by),
        })
    }
}
