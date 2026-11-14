//! The curation proposal over the API (RFC 005 part 2 D-3, D-12; handoff 005 K-T11): a proposal of its own, with no page
//! text, for a claim's community status, how each source bears on it, when a person last checked each and whether each can
//! be had. It is the form's proposal (`proposals::curate`), made by the same core function ([`curate`]) from the claim and
//! its sources **as the base the caller named has them**, and judged by the same rules at submit, at every revision and at
//! acceptance; this only reads a JSON body instead of a form. The service is the one write path.
//!
//! A member left out is left as the base has it; one that equals what the base has changes nothing; a body that changes
//! nothing is refused. Nothing here sets a status: a status is set by a proposal that people accept.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::knowledge::compose::{Curation, LinkEdit, Mistake, SourceEdit, curate};
use inklkunn_core::knowledge::{Availability, Bearing, ClaimId, Link, SourceId, Standing};
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
use crate::proposals::curate::load_with;
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
    for (i, mut link) in fields.objects("links", MAX_EDITS)?.into_iter().enumerate() {
        let at = format!("links[{i}]");
        let source = word(
            &mut link,
            "source",
            SourceId::parse,
            &format!("{at}.source"),
        )?;
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
        let reviewed = source.text("reviewed")?.trim().to_owned();
        let availability = word(
            &mut source,
            "availability",
            Availability::from_word,
            &format!("{at}.availability"),
        )?;
        source.end()?;
        sources.push(SourceEdit {
            source: id,
            reviewed,
            availability,
        });
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
    let edit = Curation {
        status: status.unwrap_or(subject.claim().community_status.status),
        links,
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
    let files = curate(subject.claim(), &subject.cited(), subject.digest(), &edit)
        .map_err(|m| mistake(req, m))?;
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
    let draft = service
        .create(&req.principal, &req.wiki, full.as_deref(), edit)
        .map_err(|error| req.refusal(error))?;
    let (record, revision) = if submit {
        // The draft was made; if it cannot be sent, say so and name it, so the caller can go on with it.
        let sent = service
            .submit(&req.principal, &req.wiki, &draft.id)
            .map_err(|error| {
                req.refusal(error)
                    .member("draft", serde_json::json!(draft.id.to_string()))
            })?;
        (sent, 1)
    } else {
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
        Mistake::Unwritable => return Problem::new(Code::Failed),
        _ => "not-curatable",
    };
    req.unacceptable_with(reason, mistake_text(req.t, m))
}
