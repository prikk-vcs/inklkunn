//! *Other views:* on a claim's page (RFC 005 part 2 D-7, D-9; handoff 005 K-T8): each view where the claim reads
//! differently from the view being read, with a link to compare the two.
//!
//! **Checked, not assumed.** The page says that no other view reads the claim differently only when it looked at
//! every other view and found none; a view whose index could not be read just now, or that is past the bound, is
//! said to be unchecked, never silently counted as agreeing. The bound ([`CHECKED`]) is on how many other views one
//! page load reads (each tip's index is the derived one: held, stored, or built from one extract when it is not
//! there yet), and the page says so in words and points at the comparison of views for the rest.
//!
//! A wiki with no other open view has nothing to say here, and the page says nothing: no heading over an empty
//! section. No view is named better than another; the default view is first because the list of views puts it
//! first, and the rest follow by name.

use inklkunn_core::read::ComparedClaim;
use inklkunn_core::views::short_name;
use inklkunn_store::WikiRecord;

use crate::catalog::{Key, T};
use crate::proposals::render::LinkView;
use crate::services::Services;
use crate::views::compare::claim_facts;
use crate::views::{named, paths};

/// The most other views one claim page reads the index of.
pub(crate) const CHECKED: usize = 10;

/// What was found, as data (the words are made where the page is drawn, in the reader's language).
pub(crate) struct Others {
    /// The short name of the view being read.
    this: String,
    /// The views where the claim reads differently: `(view, how)`, in the order the views are listed.
    differing: Vec<(String, ComparedClaim)>,
    /// How many other views were left unchecked because of the bound.
    beyond: usize,
    /// Some other view's index could not be read just now.
    unreadable: bool,
}

/// Look at the other open views for the claim `id`, as read in the view `this` (`None`: the default view). `None` when
/// the wiki has no other open view or its views cannot be listed now (the page then says nothing, as it did before
/// views could be compared).
///
/// **The comparison is the one *Compare views* makes**, held by the two tips' blocks (handoff 005 K12): a claim page
/// that is read again, or by many, does not build the claims two views differ in again for a pair of tips it has seen.
pub(crate) fn gather(
    services: &Services,
    wiki: &WikiRecord,
    this: Option<&str>,
    id: &str,
) -> Option<Others> {
    let open = services.read.open_views(wiki).ok()?;
    let this_full = this.and_then(crate::url::view_ref);
    let this = this
        .map_or_else(|| short_name(&wiki.default_view), |v| v)
        .to_owned();
    let others: Vec<&str> = open
        .iter()
        .map(|v| short_name(v))
        .filter(|v| *v != this)
        .collect();
    if others.is_empty() {
        return None;
    }
    let mut found = Others {
        this,
        differing: Vec::new(),
        beyond: others.len().saturating_sub(CHECKED),
        unreadable: false,
    };
    for other in others.into_iter().take(CHECKED) {
        let Ok(differences) = services.read.claim_differences(
            wiki,
            this_full.as_deref(),
            Some(&format!("heads/{other}")),
        ) else {
            found.unreadable = true;
            continue;
        };
        if let Some(difference) = differences.iter().find(|d| d.id == id) {
            found.differing.push((
                other.to_owned(),
                ComparedClaim {
                    difference: difference.clone(),
                    said_in: None,
                },
            ));
        }
    }
    Some(found)
}

/// One view where the claim reads differently, in words.
pub(super) struct Line {
    pub(super) label: String,
    pub(super) facts: Vec<String>,
    pub(super) compare: LinkView,
}

/// The section, in words.
pub(super) struct Section {
    pub(super) heading: String,
    pub(super) lines: Vec<Line>,
    pub(super) none: Option<String>,
    pub(super) limit: Option<String>,
    pub(super) unchecked: Option<String>,
}

impl Others {
    /// The section as the page shows it.
    pub(super) fn section(&self, t: T, wiki: &WikiRecord) -> Section {
        let slug = wiki.slug.as_str();
        let checked_all = self.beyond == 0 && !self.unreadable;
        Section {
            heading: t.get(Key::ClaimOthersHeading),
            lines: self
                .differing
                .iter()
                .map(|(other, claim)| Line {
                    label: named(t, wiki, other),
                    facts: claim_facts(t, wiki, &self.this, other, claim),
                    compare: LinkView {
                        href: paths::compare(slug, Some(&self.this), Some(other)),
                        label: t
                            .msg(Key::ClaimOthersCompare)
                            .with("view", named(t, wiki, other))
                            .text(),
                    },
                })
                .collect(),
            none: (self.differing.is_empty() && checked_all).then(|| t.get(Key::ClaimOthersNone)),
            limit: (self.beyond > 0)
                .then(|| t.msg(Key::ClaimOthersLimit).with("shown", CHECKED).text()),
            unchecked: self.unreadable.then(|| t.get(Key::ClaimOthersUnchecked)),
        }
    }
}
