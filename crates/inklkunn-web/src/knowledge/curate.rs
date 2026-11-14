//! The way to the curation form (RFC 005 part 2 D-3; handoff 005 K39).
//!
//! The curation form (`…/proposals/curate?claim=<id>`) is the only place a community status, a review day or an
//! availability can be proposed, so every page that tells a reader about a claim, or that tells them a change needs
//! a curation proposal, leads to it, **for those who may propose** (an account the authorizer lets propose to the
//! wiki) and for nobody else: a link a reader cannot use is not offered.

use inklkunn_core::knowledge::check::Refusal;
use inklkunn_core::knowledge::index::KnowledgeIndex;
use inklkunn_core::knowledge::{ClaimId, Family, family_of};
use inklkunn_core::read::ReadError;
use inklkunn_core::{Action, Principal, Resource};
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, Rich, T, code};
use crate::services::Services;

/// The most claims one refusal offers a form for: a proposal that touches more says so in its lines, and each claim's
/// page leads to its own form.
pub(crate) const MOST_OFFERED: usize = 5;

/// May this principal use the curation form of the wiki? An account, and one the authorizer lets propose to it: the
/// two things the form itself asks.
pub(crate) fn may_propose(services: &Services, principal: &Principal, wiki: &WikiRecord) -> bool {
    principal.account_id().is_some()
        && services
            .authorizer
            .authorize(
                principal,
                Action::Propose,
                &Resource::Wiki {
                    wiki: wiki.id.clone(),
                },
            )
            .is_allowed()
}

/// The link to a claim's curation form: its words and its address. `view` is the view's short name, `None` for the
/// default view.
pub(crate) fn link(t: T, slug: &str, claim: &str, view: Option<&str>) -> (String, String) {
    (
        t.get(Key::KnowCurateLink),
        crate::proposals::paths::curate(slug, claim, view),
    )
}

/// The link to a claim's curation form where a page that is not the claim's offers it: it names the claim, by its whole
/// identifier in `<code>` (U-i), so that the links are told apart, and so that a single one has an antecedent.
pub(crate) fn link_of(t: T, slug: &str, claim: &str, view: Option<&str>) -> (Rich, String) {
    (
        t.msg(Key::KnowCurateLinkOf)
            .rich("claim", code(claim))
            .html(),
        crate::proposals::paths::curate(slug, claim, view),
    )
}

/// What a refusal offers: a link to the curation form of each claim it touches (at most [`MOST_OFFERED`]), and, when
/// there are more, a sentence that says how many (U-i): a list is never cut without saying so.
#[derive(Clone, Debug, Default)]
pub(crate) struct Offers {
    /// Each link: its words and its address.
    pub(crate) links: Vec<(Rich, String)>,
    /// How many claims more the change touches than the links offered.
    pub(crate) more: Option<String>,
}

impl Offers {
    /// Whether nothing is offered.
    pub(crate) fn is_empty(&self) -> bool {
        self.links.is_empty() && self.more.is_none()
    }
}

/// What a refusal that says *this change needs a curation proposal* offers: the curation form of each claim the
/// refused files touch (see [`claims_of`]), for those who may propose, and nothing for others. `full_view` is the view
/// the proposal is for, as the service names it (`heads/<name>`). **Each link names its claim** (U-i), one claim too:
/// the proposal's page is not the claim's, so *its* would have nothing to refer to.
pub(crate) fn offers(
    ctx: &crate::proposals::Ctx,
    t: T,
    full_view: &str,
    refused: &[Refusal],
) -> Result<Offers, ReadError> {
    if refused.is_empty() || !may_propose(&ctx.services, &ctx.principal, &ctx.wiki) {
        return Ok(Offers::default());
    }
    let index = ctx
        .services
        .read
        .knowledge_index(&ctx.wiki, Some(full_view))?;
    let view = full_view
        .strip_prefix("heads/")
        .filter(|_| full_view != ctx.wiki.default_view);
    let claims = claims_of(&index, refused);
    let slug = ctx.slug();
    let more = claims.len().saturating_sub(MOST_OFFERED);
    Ok(Offers {
        links: claims
            .iter()
            .take(MOST_OFFERED)
            .map(|claim| link_of(t, slug, claim, view))
            .collect(),
        more: (more > 0).then(|| {
            t.msg(Key::KnowCurateMore)
                .n(u64::try_from(more).unwrap_or(u64::MAX))
                .text()
        }),
    })
}

/// The claims whose curation form a refusal leads to: the claim a refused file is, and the claims that cite a source a
/// refused file is. Only claims the index of the view holds (the form opens on one that is there), in the order the
/// files were refused, each once. All of them: [`offers`] offers the first [`MOST_OFFERED`] and counts the rest.
pub(crate) fn claims_of(index: &KnowledgeIndex, refused: &[Refusal]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in refused {
        let name = std::path::Path::new(&r.file)
            .file_stem()
            .and_then(|n| n.to_str());
        let found: Vec<String> = match (family_of(&r.file), name) {
            (Some(Family::Claim), Some(id)) => ClaimId::parse(id)
                .filter(|id| index.claim(id.as_str()).is_some())
                .map(|id| id.to_string())
                .into_iter()
                .collect(),
            (Some(Family::Source), Some(id)) => {
                index.claims_citing(id).map(|l| l.claim.clone()).collect()
            }
            _ => Vec::new(),
        };
        for claim in found {
            if !out.contains(&claim) {
                out.push(claim);
            }
        }
    }
    out
}
