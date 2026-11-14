//! *Why is this here?* (RFC 005 part 2 D-8; handoff K-T7): one page per wiki page, listing its paragraphs, each
//! with an anchor so that a single paragraph can be linked to, and for each the change that last changed it and
//! the change that first wrote it, as far as the page's history says.
//!
//! **Derived from prikk's history and the service's records, and nothing else.** The chain of a paragraph is the
//! read service's ([`inklkunn_core::read::ReadService::page_why`]: the history page's own bounds, the merge's own
//! matching), the people of a change are what its message records and no more than the history page says, and the
//! rest of a change (the proposal, its review, its acceptor's record, the sources the proposer gave) is read from
//! the proposal records **as the visitor may read them**. A proposal the message names and the visitor may not read
//! is one plain sentence; a proposal that the acceptance records do not tie to the change has no review shown.
//!
//! **It never claims more than the method has.** A paragraph with an `^id` is said to be followed through its
//! anchor; any other is said to be *traced by matching the text*, in those words, with what that means, and the
//! sentences about when it was written are worded as what was found (*earliest wording found*), not as what
//! happened. A page whose history was read only as far as the history page reads says so, in words, with the
//! number of changes it read; so does a history that could not be followed.
//!
//! **Authorized as pages are, and existence-neutral for the wiki**: the handler begins as every handler of a wiki
//! does ([`open`]), so a wiki that is absent and one the visitor may not read are one page; a title no page has is
//! the page every missing title gets. No JavaScript, and no colour, icon or number that ranks: the page is a list
//! of sentences.

use std::collections::{BTreeSet, HashMap};

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::Principal;
use inklkunn_core::read::{
    Basis, Chain, MAX_WHY_PARAGRAPHS, Presentation, Reach, WhyOutcome, WhyPage,
};
use inklkunn_store::{AcceptanceOutcome, DecisionKind, ProposalId, ProposalRecord, WikiRecord};

use super::marks::{ParagraphClaim, distinct, offered};
use super::paths;
use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::history::{flags, people};
use crate::pages::{Page, PageState, answer};
use crate::present::WikiLinks;
use crate::services::Services;
use crate::url::{change_path, history_path, view_ref};
use crate::wiki::{
    Shell, Shown, blocking, generic_not_found, open, present, render_failed, services_of,
};

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// The most items of a proposal's review (decisions, then comments) one change shows; the rest are counted and
/// left to the proposal's own page.
const MAX_REVIEW_ITEMS: usize = 20;
/// The most characters of a comment, a decision's words or a source that are shown; the proposal's page has all.
const MAX_SHOWN_CHARS: usize = 300;
/// The most characters of a paragraph's beginning that name its claim link.
const START_CHARS: usize = 40;

// ------------------------------------------------------------------------------------ the view

struct Link {
    href: String,
    label: String,
}

struct ClaimLinkView {
    href: String,
    text: String,
    name: String,
}

struct ParagraphView {
    id: String,
    label: String,
    href: String,
    excerpt: String,
    claims: Vec<ClaimLinkView>,
    facts: Vec<Rich>,
    basis: Option<String>,
}

struct ReviewView {
    heading: String,
    none: Option<String>,
    items: Vec<String>,
    more: Option<String>,
}

struct SourcesView {
    heading: String,
    none: Option<String>,
    items: Vec<String>,
}

struct ChangeView {
    id: String,
    heading: String,
    summary: Option<String>,
    people: Vec<String>,
    open: Link,
    proposal: Option<Rich>,
    note: Option<String>,
    review: Option<ReviewView>,
    sources: Option<SourcesView>,
}

#[derive(Template)]
#[template(path = "why.html")]
struct WhyTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    intro: String,
    links: Vec<Link>,
    notes: Vec<String>,
    paragraphs_heading: String,
    none: Option<String>,
    paragraphs: Vec<ParagraphView>,
    changes_heading: String,
    changes_none: Option<String>,
    changes: Vec<ChangeView>,
}

/// What a request came to, before any template: the page asked about, with everything its sentences need already
/// read (the store and prikk are asked on the blocking thread, the template only prints).
enum Loaded {
    /// A page the wiki already has a word for (no such wiki, no such view, no such page, could not be read now).
    Shown(Shown),
    /// The paragraphs of the page.
    Page(Box<Prepared>),
}

struct Prepared {
    wiki: WikiRecord,
    view: Option<String>,
    title: String,
    notes: Vec<String>,
    paragraphs: Vec<ParagraphView>,
    changes: Vec<ChangeView>,
}

// ------------------------------------------------------------------------------------ small words

/// `text` on one line and at most [`MAX_SHOWN_CHARS`] characters, with an ellipsis where it was cut.
fn shortened(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_SHOWN_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(MAX_SHOWN_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// The first [`START_CHARS`] characters of an excerpt, with an ellipsis where it was cut.
fn beginning(excerpt: &str) -> String {
    let mut chars = excerpt.chars();
    let start: String = chars.by_ref().take(START_CHARS).collect();
    if chars.next().is_some() {
        format!("{start}…")
    } else {
        start
    }
}

/// The name a person is shown by: the account's display name, else a word for someone who is not known.
fn person(services: &Services, t: T, id: &inklkunn_store::AccountId) -> String {
    match services.accounts.account(id) {
        Ok(Some(account)) => account.display_name.as_str().to_owned(),
        _ => t.get(Key::PersonUnknown),
    }
}

// ------------------------------------------------------------------------------------ one change

/// What the visitor may read of the proposal a change's message names, and what this wiki's records tie to it.
struct Reading<'a> {
    services: &'a Services,
    principal: &'a Principal,
    wiki: &'a WikiRecord,
    t: T,
}

/// Do the acceptance records say that this proposal was sealed as this block? A revision of it, whichever. (The API's
/// provenance asks the same, so that a message that names a proposal is never taken for the proof that it was one.)
pub(crate) fn sealed_as(
    services: &Services,
    principal: &Principal,
    wiki: &WikiRecord,
    id: &ProposalId,
    block: &str,
) -> bool {
    let Ok(revisions) = services.proposals.revisions(principal, wiki, id) else {
        return false;
    };
    revisions.iter().any(|revision| {
        services
            .acceptances
            .acceptances_of(id, revision.seq)
            .is_ok_and(|entries| {
                entries.iter().any(|entry| {
                    matches!(
                        entry.result.as_ref().map(|r| &r.outcome),
                        Some(AcceptanceOutcome::Sealed { block_id, .. }) if block_id == block
                    )
                })
            })
    })
}

impl Reading<'_> {
    /// Do the acceptance records say that this proposal was sealed as this block?
    fn sealed_as(&self, id: &ProposalId, block: &str) -> bool {
        sealed_as(self.services, self.principal, self.wiki, id, block)
    }

    /// The review of a proposal: its decisions, then its comments, oldest first, at most [`MAX_REVIEW_ITEMS`] of
    /// them. `None` when the records cannot be read.
    fn review(&self, id: &ProposalId) -> Option<ReviewView> {
        let t = self.t;
        let service = &self.services.proposals;
        let decisions = service.decisions(self.principal, self.wiki, id).ok()?;
        let comments = service.comments(self.principal, self.wiki, id).ok()?;
        let total = decisions.len() + comments.len();
        let decided = decisions.iter().map(|d| {
            let sentence = t
                .msg(match d.kind {
                    DecisionKind::Approve => Key::DecisionApproved,
                    DecisionKind::RequestChanges => Key::DecisionRequested,
                })
                .with("reviewer", person(self.services, t, &d.reviewer))
                .with("number", d.revision)
                .text();
            let words = shortened(&d.body);
            if words.is_empty() {
                sentence
            } else {
                format!(
                    "{sentence} {}",
                    t.msg(Key::WhyDecisionWords).with("text", words).text()
                )
            }
        });
        let commented = comments.iter().map(|c| {
            t.msg(Key::WhyComment)
                .with("author", person(self.services, t, &c.author))
                .with("number", c.revision)
                .with("text", shortened(&c.body))
                .text()
        });
        let items: Vec<String> = decided.chain(commented).take(MAX_REVIEW_ITEMS).collect();
        let more = total.saturating_sub(items.len());
        Some(ReviewView {
            heading: t.get(Key::WhyReviewHeading),
            none: items.is_empty().then(|| t.get(Key::WhyReviewNone)),
            items,
            more: (more > 0).then(|| t.msg(Key::WhyReviewMore).n(more as u64).text()),
        })
    }
}

/// A change, once: its people as its message records them (what the history page says, and no more), the proposal
/// it names with the review and the sources given with it where the visitor may read them.
fn change_view(
    reading: &Reading<'_>,
    view: Option<&str>,
    info: &inklkunn_core::read::ChangeInfo,
) -> ChangeView {
    let t = reading.t;
    let slug = reading.wiki.slug.as_str();
    let number = info.number;
    let mut people_lines = people(t, &info.provenance);
    people_lines.extend(flags(t, info));
    let mut out = ChangeView {
        id: paths::why_change_id(number),
        heading: t.msg(Key::HistoryChangeLabel).with("number", number).text(),
        summary: (!info.summary.is_empty()).then(|| info.summary.clone()),
        people: people_lines,
        open: Link {
            href: change_path(slug, view, &info.block_id),
            label: t.msg(Key::WhyChangeOpen).with("number", number).text(),
        },
        proposal: None,
        note: None,
        review: None,
        sources: None,
    };
    let Some(raw) = info.proposal.as_deref() else {
        return out;
    };
    let record: Option<(ProposalId, ProposalRecord)> =
        crate::proposals::parse_id(raw).and_then(|id| {
            let record = reading
                .services
                .proposals
                .get(reading.principal, reading.wiki, &id)
                .ok()?;
            Some((id, record))
        });
    let Some((id, record)) = record else {
        out.note = Some(t.get(Key::WhyProposalUnlisted));
        return out;
    };
    let label = if record.summary.is_empty() {
        t.get(Key::ProposalNoSummary)
    } else {
        record.summary.clone()
    };
    // *Made from* is said only of a proposal the acceptance records seal as this change; the message of a change
    // can name any proposal, and then it is the message that is quoted.
    let sealed = reading.sealed_as(&id, &info.block_id);
    out.proposal = Some(
        t.msg(if sealed {
            Key::WhyProposal
        } else {
            Key::WhyProposalNamed
        })
        .link(
            "proposal",
            label,
            crate::proposals::paths::proposal(slug, id.as_str()),
        )
        .html(),
    );
    if !sealed {
        out.note = Some(t.get(Key::WhyProposalUnconfirmed));
        return out;
    }
    out.review = reading.review(&id);
    out.sources = Some(SourcesView {
        heading: t.get(Key::WhySourcesHeading),
        none: record
            .sources
            .is_empty()
            .then(|| t.get(Key::WhySourcesNone)),
        items: record.sources.iter().map(|s| shortened(s)).collect(),
    });
    out
}

// ------------------------------------------------------------------------------------ one paragraph

/// The sentences about a paragraph's chain: the changes as links to their place further down (the page's own
/// address `own`, and the fragment: a message's link is a path, never a bare fragment), and how it was followed. What was found is said as found, never as what happened, where only the text carried the chain.
fn facts(t: T, own: &str, page: &WhyPage, chain: &Chain) -> (Vec<Rich>, Option<String>) {
    let changes = &page.changes;
    let by_anchor = chain.basis == Basis::Anchor;
    let reference = |key: Key, step: usize| -> Option<Rich> {
        let info = changes.get(step)?;
        let label = t
            .msg(Key::HistoryChangeLabel)
            .with("number", info.number)
            .text();
        Some(
            t.msg(key)
                .link(
                    "change",
                    label,
                    format!("{own}#{}", paths::why_change_id(info.number)),
                )
                .html(),
        )
    };
    let plain = |key: Key| t.msg(key).html();
    let mut lines = Vec::new();
    match (chain.first_written, chain.last_changed) {
        (Some(first), Some(last)) if first == last => {
            let key = if by_anchor {
                Key::WhyWrittenUnchanged
            } else {
                Key::WhyFoundWrittenUnchanged
            };
            lines.extend(reference(key, first));
        }
        (Some(first), last) => {
            lines.extend(last.and_then(|step| {
                reference(
                    if by_anchor {
                        Key::WhyLastChanged
                    } else {
                        Key::WhyFoundLast
                    },
                    step,
                )
            }));
            lines.extend(reference(
                if by_anchor {
                    Key::WhyFirstWritten
                } else {
                    Key::WhyFoundFirst
                },
                first,
            ));
        }
        (None, Some(last)) => {
            lines.extend(reference(
                if by_anchor {
                    Key::WhyLastChanged
                } else {
                    Key::WhyFoundLast
                },
                last,
            ));
            lines.push(plain(Key::WhyFirstBefore));
        }
        (None, None) => lines.push(plain(Key::WhyUnchangedRead)),
    }
    // How it was followed is said only where something was: with no change read there was nothing to follow.
    let basis = (!changes.is_empty()).then(|| {
        t.get(if by_anchor {
            Key::WhyBasisAnchor
        } else {
            Key::WhyBasisText
        })
    });
    (lines, basis)
}

fn paragraph_views(
    t: T,
    wiki: &WikiRecord,
    view: Option<&str>,
    page: &WhyPage,
    claims: &[ParagraphClaim],
) -> Vec<ParagraphView> {
    let mut used = HashMap::new();
    let own = paths::why(wiki.slug.as_str(), view, &page.title);
    // An `^id` that two paragraphs of the page carry names neither: each is named by its number, so that no two
    // elements of the page have the same id.
    let mut carried: HashMap<&str, usize> = HashMap::new();
    for id in page
        .paragraphs
        .iter()
        .filter_map(|p| p.paragraph.id.as_deref())
    {
        *carried.entry(id).or_default() += 1;
    }
    page.paragraphs
        .iter()
        .map(|p| {
            let paragraph = &p.paragraph;
            let anchor = paragraph
                .id
                .as_deref()
                .filter(|b| carried.get(b).copied() == Some(1));
            let id = paths::why_paragraph_id(paragraph.ordinal, anchor);
            let here: Vec<&ParagraphClaim> = paragraph.id.as_deref().map_or_else(Vec::new, |b| {
                claims.iter().filter(|c| c.block == b).collect()
            });
            let named = beginning(&paragraph.excerpt);
            let phrase = if named.is_empty() {
                t.get(Key::ClaimParagraphEmpty)
            } else {
                t.msg(Key::ClaimParagraphStart).with("text", named).text()
            };
            let claim_links = here
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let name = if here.len() == 1 {
                        t.msg(Key::ClaimLinkName).with("paragraph", &phrase).text()
                    } else {
                        t.msg(Key::ClaimLinkNameNth)
                            .with("n", i + 1)
                            .with("total", here.len())
                            .with("paragraph", &phrase)
                            .text()
                    };
                    ClaimLinkView {
                        href: paths::claim(wiki.slug.as_str(), &c.claim, view),
                        text: t.get(Key::ClaimLink),
                        name: distinct(&mut used, name),
                    }
                })
                .collect();
            let (facts, basis) = facts(t, &own, page, &p.chain);
            ParagraphView {
                href: format!("#{id}"),
                id,
                label: t
                    .msg(Key::WhyParagraphLabel)
                    .with("n", paragraph.ordinal)
                    .text(),
                excerpt: paragraph.excerpt.clone(),
                claims: claim_links,
                facts,
                basis,
            }
        })
        .collect()
}

/// The steps of the changes the paragraphs' chains name, newest first.
fn named_steps(page: &WhyPage) -> BTreeSet<usize> {
    page.paragraphs
        .iter()
        .flat_map(|p| [p.chain.first_written, p.chain.last_changed])
        .flatten()
        .collect()
}

// ------------------------------------------------------------------------------------ the request

fn prepare(
    services: &Services,
    headers: &HeaderMap,
    t: T,
    slug: &str,
    title: &str,
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
    let found = match services.read.page_why(&wiki, full.as_deref(), title) {
        Ok(WhyOutcome::Page(page)) => *page,
        Ok(WhyOutcome::Missing { title }) => {
            return Loaded::Shown(Shown::Missing {
                wiki: Box::new(wiki),
                title,
            });
        }
        Ok(WhyOutcome::Ambiguous { candidates }) => {
            let links = WikiLinks::new(t, wiki.slug.as_str(), view.as_deref());
            return Loaded::Shown(Shown::Ambiguous {
                candidates: candidates
                    .iter()
                    .map(|c| (c.title.clone(), links.page_href(&c.title)))
                    .collect(),
                wiki: Box::new(wiki),
                title: title.to_owned(),
            });
        }
        Err(e) => {
            return Loaded::Shown(crate::history::shown_for(
                services,
                Box::new(wiki),
                "the paragraphs of a page",
                e,
            ));
        }
    };

    // The claims said in the page's paragraphs, from the knowledge index of the same view; the page is served
    // without the links when it cannot be read.
    let claims = found
        .page_id
        .as_deref()
        .map(
            |id| match services.read.knowledge_index(&wiki, full.as_deref()) {
                Ok(index) => offered(&index, id),
                Err(e) => {
                    (services.log)(&format!(
                        "the claims of a page of wiki `{}` could not be read now: {e}",
                        wiki.slug
                    ));
                    Vec::new()
                }
            },
        )
        .unwrap_or_default();

    let principal = services.identity.principal(headers);
    let reading = Reading {
        services,
        principal: &principal,
        wiki: &wiki,
        t,
    };
    let changes = named_steps(&found)
        .into_iter()
        .filter_map(|step| found.changes.get(step))
        .map(|info| change_view(&reading, view.as_deref(), info))
        .collect();
    let mut notes = Vec::new();
    match found.reach {
        Reach::Whole => {}
        Reach::Bounded if found.changes.is_empty() => notes.push(t.get(Key::WhyBoundedNone)),
        Reach::Spent if found.changes.is_empty() => notes.push(t.get(Key::WhySpentNone)),
        // The changes listed are the ones that were read: the count is the same one the bounds' note gives.
        Reach::Bounded | Reach::Spent => {
            notes.push(t.msg(Key::WhyBounded).n(found.changes.len() as u64).text());
        }
        Reach::Unfollowed => notes.push(t.get(Key::WhyUnfollowed)),
    }
    if found.left_out > 0 {
        notes.push(
            t.msg(Key::WhyLeftOut)
                .n(found.left_out as u64)
                .with("limit", MAX_WHY_PARAGRAPHS)
                .text(),
        );
    }
    let paragraphs = paragraph_views(t, &wiki, view.as_deref(), &found, &claims);
    Loaded::Page(Box::new(Prepared {
        view,
        title: found.title,
        notes,
        paragraphs,
        changes,
        wiki,
    }))
}

/// `GET /{slug}/-/why/{*title}`.
pub(crate) async fn why_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path((slug, title))), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let params = params.map(|Query(p)| p).unwrap_or_default();
    let t = page.t();
    let loaded = blocking(move || prepare(&services, &headers, t, &slug, &title, &params)).await;
    match loaded {
        None => render_failed(&page),
        Some(Loaded::Shown(shown)) => present(&page, shown),
        Some(Loaded::Page(prepared)) => render(&page, *prepared),
    }
}

fn render(page: &Page, prepared: Prepared) -> Response {
    let t = page.t();
    let wiki = &prepared.wiki;
    let view = prepared.view.as_deref();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let at = paths::why_tail(&prepared.title, view);
    let page = page.wiki_head(wiki, Subject::Surface(&at));
    let no_changes = prepared.changes.is_empty();
    let rendered = WhyTpl {
        page: &page,
        shell: &shell,
        title: t
            .msg(Key::TitleWhy)
            .with("page", &prepared.title)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: page.get(Key::WhyHeading),
        view_note: view.map(|v| t.msg(Key::WikiViewInUse).with("view", v).text()),
        intro: t.msg(Key::WhyIntro).with("page", &prepared.title).text(),
        links: vec![
            Link {
                href: WikiLinks::new(t, slug, view).page_href(&prepared.title),
                label: t.msg(Key::WhyBack).with("page", &prepared.title).text(),
            },
            Link {
                href: history_path(slug, view, &prepared.title, None),
                label: t
                    .msg(Key::WhyHistoryLink)
                    .with("page", &prepared.title)
                    .text(),
            },
        ],
        notes: prepared.notes,
        paragraphs_heading: page.get(Key::WhyParagraphsHeading),
        none: prepared
            .paragraphs
            .is_empty()
            .then(|| page.get(Key::WhyNoParagraphs)),
        paragraphs: prepared.paragraphs,
        changes_heading: page.get(Key::WhyChangesHeading),
        changes_none: no_changes.then(|| page.get(Key::WhyChangesNone)),
        changes: prepared.changes,
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}
