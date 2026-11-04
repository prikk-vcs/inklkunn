//! A claim's page (RFC 005 part 2 D-7; handoff K-T6): what kind of statement it is, where it is said, its
//! sources with the relation and the bearing in words, *judged on an earlier wording* where so, and its status in
//! this view with the proposal that decided it.
//!
//! **Read from the derived knowledge index and nothing else**: the page is made of the index's rows for the tip
//! of the view, never of the files again. **No word, colour, icon or number ranks**: a source is said to support
//! or challenge a claim and how directly it speaks to it (never how good it is), and a status is one of four
//! words with the proposal that decided it. The words *true*, *false*, *verified*, *proven*, *canonical*,
//! *adopted* and *official* are retired in both languages (`tests/retired_words.rs`).
//!
//! **Authorized as pages are, and existence-neutral**: the handler begins as every handler of a wiki does (a wiki
//! that is absent and one the visitor may not read are one page), and an address that names no claim of the wiki
//! (not an id, or an id the wiki does not hold) is one plain page that says so.
//!
//! *Other views:* is left out. Which views read a claim differently is the comparison of views (handoff K-T8), and
//! until it exists the page does not say that no view differs, which it has not checked.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::knowledge::index::{self, KnowledgeIndex};
use inklkunn_core::knowledge::Standing;
use inklkunn_core::read::{Presentation, ReadError};
use inklkunn_markup::path::path_to_title;
use inklkunn_store::{KnowledgeClaimRow, WikiRecord};

use super::marks::distinct;
use super::{paths, words};
use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::present::WikiLinks;
use crate::services::Services;
use crate::url::view_ref;
use crate::wiki::{
    Choice, Shell, Shown, blocking, generic_not_found, notice, open, present, render_failed,
    services_of,
};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// A place the claim is said: the page, at the paragraph.
struct Said {
    href: String,
    text: String,
}

/// One source of the claim, in a sentence.
struct SourceLine {
    line: String,
    earlier: Option<String>,
}

/// The claim's status in this view: its sentence, and who decided it.
struct StatusLine {
    sentence: String,
    decided: Option<Rich>,
    unlisted: Option<String>,
    earlier: Option<String>,
}

#[derive(Template)]
#[template(path = "claim.html")]
struct ClaimTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    kind: String,
    said_heading: String,
    said: Vec<Said>,
    said_nowhere: Option<String>,
    sources_heading: String,
    sources: Vec<SourceLine>,
    sources_none: Option<String>,
    status_heading: String,
    status: StatusLine,
}

/// What a request for a claim's page came to, before any template.
enum Loaded {
    /// A page the wiki already has a word for (no such wiki, no such view, could not be read now).
    Shown(Shown),
    /// The wiki is there and holds no such claim.
    NoClaim(Box<WikiRecord>),
    /// The claim.
    Claim(Box<Found>),
}

struct Found {
    wiki: WikiRecord,
    /// The view as the address spells it, `None` for the default view.
    view: Option<String>,
    index: std::sync::Arc<KnowledgeIndex>,
    claim: KnowledgeClaimRow,
    /// Whether the proposal that decided the status is one this wiki's records hold and the visitor may read.
    proposal_known: bool,
}

fn load(
    services: &Services,
    headers: &HeaderMap,
    slug: &str,
    id: &str,
    params: &HashMap<String, String>,
) -> Loaded {
    let Some(wiki) = open(services, headers, slug) else {
        return Loaded::Shown(Shown::NotFound);
    };
    let (view, full) = match params.get("view") {
        None => (None, None),
        Some(name) => match view_ref(name) {
            Some(full) => (Some(name.clone()), Some(full)),
            None => {
                return Loaded::Shown(Shown::NoSuchView {
                    wiki: Box::new(wiki),
                    view: name.clone(),
                });
            }
        },
    };
    let index = match services.read.knowledge_index(&wiki, full.as_deref()) {
        Ok(index) => index,
        Err(ReadError::NoSuchView(v)) => {
            return Loaded::Shown(Shown::NoSuchView {
                wiki: Box::new(wiki),
                view: v.strip_prefix("heads/").unwrap_or(&v).to_owned(),
            });
        }
        Err(e) => {
            (services.log)(&format!(
                "the claims of wiki `{}` could not be read now: {e}",
                wiki.slug
            ));
            return Loaded::Shown(Shown::Unavailable {
                wiki: Box::new(wiki),
            });
        }
    };
    let Some(claim) = index.claim(id).cloned() else {
        return Loaded::NoClaim(Box::new(wiki));
    };
    let proposal_known = claim
        .standing_proposal
        .as_deref()
        .and_then(crate::proposals::parse_id)
        .is_some_and(|p| {
            services
                .proposals
                .get(&services.identity.principal(headers), &wiki, &p)
                .is_ok()
        });
    Loaded::Claim(Box::new(Found {
        wiki,
        view,
        index,
        claim,
        proposal_known,
    }))
}

/// `GET /{slug}/-/claim/{id}`.
pub(crate) async fn claim_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, id))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let for_work = id.clone();
    let loaded = blocking(move || load(&services, &headers, &slug, &for_work, &params)).await;
    match loaded {
        None => render_failed(&page),
        Some(Loaded::Shown(shown)) => present(&page, shown),
        Some(Loaded::NoClaim(wiki)) => no_such_claim(&page, &wiki),
        Some(Loaded::Claim(found)) => render(&page, &found, &id),
    }
}

fn no_such_claim(page: &Page, wiki: &WikiRecord) -> Response {
    notice(
        page,
        wiki,
        StatusCode::NOT_FOUND,
        Key::ClaimNoneHeading,
        vec![page.get(Key::ClaimNoneBody)],
        Vec::<Choice>::new(),
        None,
    )
}

/// Where the claim is said: each page that has the paragraph now, linked to it. Two places on pages of one title
/// are told apart by a count.
fn said_in(t: T, found: &Found) -> Vec<Said> {
    let links = WikiLinks::new(t, found.wiki.slug.as_str(), found.view.as_deref());
    let mut used = HashMap::new();
    found
        .index
        .anchors_of(&found.claim.id)
        .filter(|a| a.state == index::PRESENT)
        .filter_map(|a| {
            let title = path_to_title(a.page_path.as_deref()?)?;
            Some(Said {
                href: paths::paragraph(&links.page_href(&title), &a.block),
                text: distinct(&mut used, title),
            })
        })
        .collect()
}

/// The claim's sources, each in a sentence: which source, what kind, and how it bears on the claim.
fn source_lines(t: T, found: &Found) -> Vec<SourceLine> {
    found
        .index
        .links_of(&found.claim.id)
        .filter_map(|link| {
            // A phrase for a relation or a bearing this build does not know is not made up: the source is left out.
            let how = words::how(&link.relation, &link.bearing)?;
            let kind = match found.index.source(&link.source) {
                Some(source) => t.get(words::source_kind(&source.kind)),
                None => t.get(Key::ClaimSourceUnreadable),
            };
            Some(SourceLine {
                line: t
                    .msg(Key::ClaimSourceLine)
                    .with("id", &link.source)
                    .with("kind", kind)
                    .with("how", t.get(how))
                    .text(),
                earlier: (link.judgement == index::EARLIER).then(|| t.get(Key::ClaimJudgedEarlier)),
            })
        })
        .collect()
}

/// The status in this view and who decided it. A status nobody decided names no proposal.
fn status_line(t: T, found: &Found) -> StatusLine {
    let (standing, key) = words::status(&found.claim.standing);
    let decided = standing != Standing::Unknown;
    let proposal = found.claim.standing_proposal.as_deref().filter(|_| decided);
    StatusLine {
        sentence: t.get(key),
        decided: proposal.filter(|_| found.proposal_known).map(|p| {
            t.msg(Key::ClaimDecidedBy)
                .link(
                    "proposal",
                    t.get(Key::ClaimDecidedLink),
                    crate::proposals::paths::proposal(found.wiki.slug.as_str(), p),
                )
                .html()
        }),
        unlisted: proposal
            .filter(|_| !found.proposal_known)
            .map(|_| t.get(Key::ClaimDecidedUnlisted)),
        earlier: (decided && found.claim.standing_judgement == index::EARLIER)
            .then(|| t.get(Key::ClaimJudgedEarlier)),
    }
}

fn render(page: &Page, found: &Found, id: &str) -> Response {
    let t = page.t();
    let wiki = &found.wiki;
    let shell = Shell::of(wiki);
    let at = paths::claim_tail(id, found.view.as_deref());
    let page = page.wiki_head(wiki, Subject::Surface(&at));
    let said = said_in(t, found);
    let sources = source_lines(t, found);
    let rendered = ClaimTpl {
        page: &page,
        shell: &shell,
        title: t
            .msg(Key::TitleClaim)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: page.get(Key::ClaimHeading),
        view_note: found
            .view
            .as_deref()
            .map(|v| t.msg(Key::WikiViewInUse).with("view", v).text()),
        kind: t
            .msg(Key::ClaimKind)
            .with("kind", t.get(words::kind(&found.claim.kind)))
            .text(),
        said_heading: page.get(Key::ClaimSaidHeading),
        said_nowhere: said.is_empty().then(|| page.get(Key::ClaimSaidNowhere)),
        said,
        sources_heading: page.get(Key::ClaimSourcesHeading),
        sources_none: sources.is_empty().then(|| page.get(Key::ClaimSourcesNone)),
        sources,
        status_heading: page.get(Key::ClaimStatusHeading),
        status: status_line(t, found),
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}
