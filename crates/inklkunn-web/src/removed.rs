//! A page that was removed (RFC 006 D-10; handoff G-T6): the page a removed page answers with, **`410 Gone`**, and the
//! way back that page offers.
//!
//! **The difference is the history's** ([`inklkunn_core::read::ReadService::removed_page`]): a title that was never a page
//! is the `404` that invites it, as before; a page whose newest change removed it is `410`, says when (the change, and the
//! moment its proposal was accepted when this wiki keeps the proposal), by which proposal and for what reason, and offers to
//! *propose restoring it*: a rollback of that change when it merges cleanly, or a proposal that writes the page again with its
//! last text when later changes block that (the form is [`crate::restore`]). **It is the wiki's own page, behind the wiki's
//! own read check**: a wiki the visitor may not read answers as one that is not there, before anything here runs, so a
//! restricted wiki's removed page is the absent wiki's answer to a stranger, byte for byte.
//!
//! A person who may not propose sees why, not the link: the sentences that say it are the refusal of a proposal's own
//! (the reason and the end of a restriction, told to the restricted person alone).
//!
//! **The reason is read in one place**, [`inklkunn_core::read::RemovedPage::reason`], which the tombstones of RFC 006 D-8
//! wrap; this module never reads the change's summary itself. A hidden summary is the tombstone's lines in its place. The page
//! names no author: the proposal is a link, and the people of the change are on the change's own page. When the text that
//! restoring would write again is hidden, the page says so and offers the empty new page only (INV-13).

use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::proposal::ProposalError;
use inklkunn_core::read::{ReadError, Reason, RemovedPage, Restore};
use inklkunn_core::{Action, Resource};
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};
use crate::pages::Page;
use crate::proposals::paths as proposal_paths;
use crate::services::Services;
use crate::time::utc_minutes;
use crate::url::{change_path, history_path};
use crate::wiki::{Choice, notice};

/// A removed page, with what the visitor may do about it.
pub(crate) struct Gone {
    wiki: Box<WikiRecord>,
    removed: RemovedPage,
    view_name: Option<String>,
    /// When the proposal the removing change names was accepted, when this wiki keeps it (UTC, to the minute).
    accepted_at: Option<String>,
    /// The proposal's address, when the message names one.
    proposal: Option<String>,
    /// May the visitor propose in this wiki? Only a hint for a link: the form and the act ask again.
    may_propose: bool,
    /// Why not, in the refusal's own sentences, when the visitor may not.
    why_not: Vec<String>,
}

/// What a title that leads to no page came to.
pub(crate) enum Look {
    /// A page that was removed.
    Gone(Box<Gone>),
    /// No page, and none that was removed: the page that invites it.
    Never,
    /// The history could not be asked now.
    Unavailable,
}

/// Is `title`, which led to no page of `wiki`'s `view`, a page that was removed? Asked after the wiki's read check.
pub(crate) fn look(
    services: &Services,
    headers: &HeaderMap,
    t: T,
    wiki: &WikiRecord,
    view_full: Option<&str>,
    view_name: Option<&str>,
    title: &str,
) -> Look {
    let removed = match services.read.removed_page(wiki, view_full, title) {
        Ok(Some(removed)) => removed,
        Ok(None) => return Look::Never,
        Err(ReadError::CouldNotBeReadNow(cause)) => {
            (services.log)(&format!(
                "the history of a missing page of wiki `{}` could not be read now: {cause}",
                wiki.slug
            ));
            return Look::Unavailable;
        }
        // A title, a view or a page the read refuses is no removed page.
        Err(_) => return Look::Never,
    };
    let principal = services.identity.principal(headers);
    let resource = Resource::Wiki {
        wiki: wiki.id.clone(),
    };
    let may_propose = services
        .authorizer
        .authorize(&principal, Action::Propose, &resource)
        .is_allowed();
    let why_not = if may_propose {
        Vec::new()
    } else {
        let error = services
            .authorizer
            .restriction_refusal(&principal, Action::Propose, &resource)
            .map_or(ProposalError::NotAllowed, ProposalError::Restricted);
        crate::proposals::words::refusal(t, &error)
            .map(|r| r.paragraphs)
            .unwrap_or_default()
    };
    // The proposal the change names, when this wiki keeps it and the visitor may read it.
    let proposal_id = removed
        .removal
        .proposal
        .as_deref()
        .and_then(crate::proposals::parse_id);
    let record = proposal_id
        .as_ref()
        .and_then(|id| services.proposals.get(&principal, wiki, id).ok());
    Look::Gone(Box::new(Gone {
        wiki: Box::new(wiki.clone()),
        accepted_at: record.as_ref().and_then(|r| r.closed_at).map(utc_minutes),
        proposal: proposal_id
            .zip(record)
            .map(|(id, _)| proposal_paths::proposal(wiki.slug.as_str(), id.as_str())),
        removed,
        view_name: view_name.map(str::to_owned),
        may_propose,
        why_not,
    }))
}

/// The page that says a page was removed: status `410`, never a bare `404`.
pub(crate) fn present(page: &Page, gone: Gone) -> Response {
    let t = page.t();
    let Gone {
        wiki,
        removed,
        view_name,
        accepted_at,
        proposal,
        may_propose,
        why_not,
    } = gone;
    let slug = wiki.slug.as_str();
    let view = view_name.as_deref();
    let number = removed.removal.number;
    let mut paragraphs = vec![
        t.msg(Key::RemovedBody)
            .with("title", &removed.title)
            .with("wiki", wiki.display_name.as_str())
            .text(),
        t.msg(Key::RemovedByChange).with("number", number).text(),
    ];
    if let Some(when) = &accepted_at {
        paragraphs.push(t.msg(Key::RemovedAcceptedAt).with("when", when).text());
    }
    match removed.reason() {
        Reason::Given(reason) => {
            paragraphs.push(t.msg(Key::RemovedReason).with("reason", reason).text());
        }
        // A hidden summary is the tombstone, never the words.
        Reason::Hidden(mark) => paragraphs.extend(crate::tombstone::mark_facts(t, mark)),
        Reason::NotGiven => paragraphs.push(t.get(Key::RemovedNoReason)),
    }
    paragraphs.push(match &removed.restore {
        Restore::Rollback { .. } => t.get(Key::RemovedRestoreRollback),
        Restore::LastText { .. } => t.get(Key::RemovedRestoreLastText),
        Restore::NotOffered => t.get(Key::RemovedRestoreNotOffered),
        Restore::TextHidden { titles } => crate::restore::text_hidden_words(t, titles),
    });
    let mut choices = vec![
        Choice {
            label: t.msg(Key::RemovedLinkChange).with("number", number).text(),
            href: change_path(slug, view, &removed.removal.block_id),
        },
        Choice {
            label: t.get(Key::PageLinkHistory),
            href: history_path(slug, view, &removed.title, None),
        },
    ];
    if let Some(href) = proposal {
        choices.insert(
            1,
            Choice {
                label: t.get(Key::RemovedLinkProposal),
                href,
            },
        );
    }
    let offer = if !may_propose {
        paragraphs.push(t.get(Key::RemovedCannotPropose));
        paragraphs.extend(why_not);
        None
    } else {
        Some(match removed.restore {
            Restore::Rollback { .. } | Restore::LastText { .. } => Choice {
                label: t.get(Key::RemovedRestoreLink),
                href: crate::restore::path(slug, &removed.title, view),
            },
            Restore::NotOffered | Restore::TextHidden { .. } => Choice {
                label: t.get(Key::ProposeNewPageLink),
                href: proposal_paths::new_page(slug, Some(&removed.title)),
            },
        })
    };
    notice(
        page,
        &wiki,
        StatusCode::GONE,
        Key::RemovedHeading,
        paragraphs,
        choices,
        offer,
    )
}
