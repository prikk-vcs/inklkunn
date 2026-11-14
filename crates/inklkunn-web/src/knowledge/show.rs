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
//! *Other views:* names each other view where the claim reads differently from this one, with a link to compare
//! the two (handoff K-T8; `others`). The page says that no other view reads it differently only when it has looked
//! at every other view; a view it could not read, or did not reach, is said to be unchecked.

use std::collections::HashMap;

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::knowledge::index::{self, KnowledgeIndex};
use inklkunn_core::knowledge::{ClaimId, Standing};
use inklkunn_core::read::{Presentation, ReadError};
use inklkunn_markup::path::path_to_title;
use inklkunn_store::{KnowledgeClaimRow, KnowledgeSourceRow, WikiRecord};

use super::marks::distinct;
use super::others::{self, Others, Section};
use super::thread;
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
    /// What else the source says of itself, each as a sentence of its own (where it is found; the one way to
    /// tell a source that names itself in no way).
    details: Vec<String>,
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
    others: Option<Section>,
    thread: thread::Section,
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
    /// The other views where the claim reads differently; `None` when the wiki has no other open view.
    others: Option<Others>,
    /// The claim's own thread: a page of it, as read for this request (RFC 005 part 2 D-11).
    thread: thread::Read,
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
    // The tip is read once: the page's index and everything compared with it are of this one state.
    let read = services.read.tip(&wiki, full.as_deref()).and_then(|tip| {
        services
            .read
            .knowledge_at(&wiki, &tip)
            .map(|index| (tip, index))
    });
    let (tip, index) = match read {
        Ok(found) => found,
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
    let others = others::gather(services, &wiki, view.as_deref(), &tip, id);
    // The claim is in the index, so its id is one.
    let thread =
        ClaimId::parse(id).map(|claim| thread::read(services, headers, &wiki, &claim, params));
    Loaded::Claim(Box::new(Found {
        wiki,
        view,
        index,
        claim,
        proposal_known,
        others,
        thread: thread.unwrap_or_else(thread::Read::none),
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

/// A text a source gave of itself, on one line; `None` when nothing is left of it.
fn one_line(text: Option<&str>) -> Option<String> {
    let line = text?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then_some(line)
}

/// What a readable source is called on the claim's page: its title (else *a source without a title*), then its
/// author or publisher and its date, each as it was written; and the sentences that go after its line. The
/// identifier is told only of a source that names itself in no way, since nothing else would tell it from
/// another. Never a score; every text is printed escaped.
pub(crate) fn source_name(t: T, source: &KnowledgeSourceRow) -> (String, Vec<String>) {
    let (title, author, date, reference) = (
        one_line(source.title.as_deref()),
        one_line(source.author.as_deref()),
        one_line(source.date.as_deref()),
        one_line(source.reference.as_deref()),
    );
    let nameless = [&title, &author, &date, &reference]
        .iter()
        .all(|part| part.is_none());
    let first = match &title {
        Some(title) => t.msg(Key::ClaimSourceTitled).with("title", title).text(),
        None => t.get(Key::ClaimSourceUntitled),
    };
    let parts = [
        author.map(|a| t.msg(Key::ClaimSourceBy).with("author", a).text()),
        date.map(|d| t.msg(Key::ClaimSourceDated).with("date", d).text()),
    ];
    let name = parts.into_iter().flatten().fold(first, |name, part| {
        t.msg(Key::ClaimSourceJoin)
            .with("first", name)
            .with("second", part)
            .text()
    });
    let mut details = Vec::new();
    if let Some(reference) = reference {
        details.push(
            t.msg(Key::ClaimSourceFoundAt)
                .with("reference", reference)
                .text(),
        );
    }
    if nameless {
        details.push(
            t.msg(Key::ClaimSourceIdentifier)
                .with("id", &source.id)
                .text(),
        );
    }
    (name, details)
}

/// The claim's sources, each in a sentence: what the source is called, what kind, and how it bears on the claim.
fn source_lines(t: T, found: &Found) -> Vec<SourceLine> {
    found
        .index
        .links_of(&found.claim.id)
        .filter_map(|link| {
            // A phrase for a relation or a bearing this build does not know is not made up: the source is left out.
            let how = words::how(&link.relation, &link.bearing)?;
            let earlier =
                (link.judgement == index::EARLIER).then(|| t.get(Key::ClaimJudgedEarlier));
            Some(match found.index.source(&link.source) {
                Some(source) => {
                    let (name, details) = source_name(t, source);
                    SourceLine {
                        line: t
                            .msg(Key::ClaimSourceLineNamed)
                            .with("name", name)
                            .with("kind", t.get(words::source_kind(&source.kind)))
                            .with("how", t.get(how))
                            .text(),
                        details,
                        earlier,
                    }
                }
                // Its file cannot be read, so there is nothing it says of itself: the identifier is all there is.
                None => SourceLine {
                    line: t
                        .msg(Key::ClaimSourceLine)
                        .with("id", &link.source)
                        .with("kind", t.get(Key::ClaimSourceUnreadable))
                        .with("how", t.get(how))
                        .text(),
                    details: Vec::new(),
                    earlier,
                },
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
        others: found.others.as_ref().map(|o| o.section(t, wiki)),
        thread: thread::section(t, &found.thread, wiki, id, found.view.as_deref()),
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}
