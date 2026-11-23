//! Protections of pages and namespaces, as `authorize()` and the pages read them (RFC 006 D-7; handoff G-T3; threat model
//! C-3g). **Pure**: what a protection is, which protections cover a page, and what they ask of an acceptance. Where they
//! are kept, and the acts that set, renew and lift a temporary one, are [`Authorizer`](crate::Authorizer)'s
//! (`authorize/protections.rs`), because a protection is an outcome of `authorize()` and of nothing else: **no template and
//! no handler evaluates one**.
//!
//! # Two kinds, one display
//!
//! - **Standing** ([`Kind::Standing`]): the policy's `protect` list, changed only by a proposal to `governance/policy.yaml`.
//! - **Temporary** ([`Kind::Temporary`]): a direct act of the `moderate` rung, recorded append-only in the store, with a
//!   reason and an end of at most the platform's limit. **It ends by itself**: a protection is in force while `now < until`
//!   and nothing is written for it to end. Renewing it and lifting it early are new records.
//!
//! Both are read as a [`Notice`]: the kind, what it was set on, what it requires and why. **The page, the proposal form,
//! the proposal's page and every refusal show the same notices, once each.** Where both kinds apply, the stricter decides
//! and both are listed.
//!
//! # What a protection asks, and of whom
//!
//! A protection **never restricts who may propose**: an outsider's proposal is what a review is for, and a proposal on
//! a protected page waits to be accepted like any other. (`authorize(_, propose, page)` is decided as it is without
//! protections; the person is told what is asked at the proposal form, before they write.) It restricts **acceptance**, of
//! any proposal that **touches** the page (writes, removes, or renames it from or to a title it covers):
//!
//! - [`Require::Rung`]`(r)`: the account that accepts must hold the rung `r` or higher, **on top of** `rungs.accept`
//!   (the higher of the two decides). It is decided by the acceptance's `judge`, before the lease and under it, and nowhere else
//!   (`authorize()` does not look at protections: the product authorizes *accept* on a proposal, never on a page).
//! - [`Require::Review`]: the proposal's latest revision needs **at least one approval** before acceptance, from an account
//!   other than the proposer that still holds the review rung and is not disabled, counted exactly as the acceptance counts
//!   `acceptance.approvals`, and **whatever `acceptance.approvals` says**: the greater of the two applies.
//!
//! **Where several apply** (a page and its namespaces, a standing and a temporary one), the stricter decides: the highest
//! rung, and a review if any asks for one. **The admin is not exempt** (the hatch of D-4 is about `governance/` only and
//! covers no page); an admin may lift a temporary protection, as the `moderate` rung may, and may change a standing one
//! by proposal.
//!
//! **Which pages a protection covers** is G-T1's rule ([`namespace_covers`](crate::governance::namespace_covers)): whole
//! segments. A page protection covers exactly that title; a namespace covers the pages below it and **not the page of the
//! same name**. A page protection is written by its **title** (`Help/Start`); a repository path is mapped to its title
//! ([`titles_of`]) and **a rename counts both its old and its new path**.

use inklkunn_markup::path_to_title;
use inklkunn_store::{ProtectionEvent, ProtectionKind, ProtectionRequirement, Role};

use crate::governance::{Policy, Require, Target};

/// Which kind of protection a [`Notice`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// *By this wiki's rules*: the policy's `protect` list.
    Standing,
    /// *Temporarily, until `until`*: a record of the `moderate` rung, in force while the clock is before `until`.
    Temporary {
        /// When it ends: Unix seconds. At this instant it no longer applies.
        until: i64,
    },
}

/// One protection in force on a page, as every surface shows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Notice {
    kind: Kind,
    target: Target,
    require: Require,
    reason: String,
}

impl Notice {
    /// A protection of the policy's `protect` list.
    pub fn standing(target: Target, require: Require, reason: &str) -> Notice {
        Notice {
            kind: Kind::Standing,
            target,
            require,
            reason: reason.to_owned(),
        }
    }

    /// A temporary protection that ends at `until` (Unix seconds).
    pub fn temporary(target: Target, require: Require, reason: &str, until: i64) -> Notice {
        Notice {
            kind: Kind::Temporary { until },
            target,
            require,
            reason: reason.to_owned(),
        }
    }

    /// The notice of a temporary protection's record, **if the record puts a protection in place that is in force at `at`**
    /// (a set or renewed event whose end is after `at`): a record that ends one, or one that has ended, is none. A second
    /// check beside the store's own, so that expiry is decided here, by the clock, in one comparison.
    pub fn of_record(event: &ProtectionEvent, at: i64) -> Option<Notice> {
        if !event.event.puts_in_place() {
            return None;
        }
        let until = event.expires_at.filter(|until| *until > at)?;
        let require = match event.requirement? {
            ProtectionRequirement::Review => Require::Review,
            ProtectionRequirement::Rung(rung) => Require::Rung(rung),
        };
        let target = match event.kind {
            ProtectionKind::Page => Target::Path(event.target.clone()),
            ProtectionKind::Namespace => Target::Namespace(event.target.clone()),
        };
        Some(Notice::temporary(
            target,
            require,
            event.reason.as_deref().unwrap_or_default(),
            until,
        ))
    }

    /// Which kind it is.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// What it was set on: a page or a namespace, by title.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// What it requires.
    pub fn require(&self) -> Require {
        self.require
    }

    /// Why: the words of whoever set it.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// When it ends, for a temporary protection.
    pub fn until(&self) -> Option<i64> {
        match self.kind {
            Kind::Standing => None,
            Kind::Temporary { until } => Some(until),
        }
    }

    /// Does it cover the page with this title?
    pub fn covers_title(&self, title: &str) -> bool {
        self.target.covers_title(title)
    }
}

/// The titles of the pages the repository `paths` are: every path that is a page's, once. A path that is no page's (a file
/// under `governance/`, a knowledge file) has no title and no protection. **A rename's two paths are two paths of the
/// list**, so a page cannot be moved out of, or into, a protected title.
pub fn titles_of<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut titles: Vec<String> = Vec::new();
    for path in paths {
        if let Some(title) = path_to_title(path)
            && !titles.contains(&title)
        {
            titles.push(title);
        }
    }
    titles
}

/// The protections in force at `at` on any of these pages: the policy's, then the temporary ones that have not ended, **each
/// once** (the same protection found by two of the titles, or twice in the records, is listed once). `temporary` is what the
/// store holds for the wiki; each is judged against `at` again here.
pub fn notices_for(
    policy: &Policy,
    temporary: &[ProtectionEvent],
    titles: &[String],
    at: i64,
) -> Vec<Notice> {
    let standing = policy
        .protections()
        .iter()
        .map(|p| Notice::standing(p.target().clone(), p.require(), p.reason()));
    let temporary = temporary
        .iter()
        .filter_map(|event| Notice::of_record(event, at));
    let mut out: Vec<Notice> = Vec::new();
    for notice in standing.chain(temporary) {
        if titles.iter().any(|title| notice.covers_title(title)) && !out.contains(&notice) {
            out.push(notice);
        }
    }
    out
}

/// What the protections on the pages of a change ask of its acceptance: **the stricter of all of them**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Demand {
    /// The highest rung any protection requires of the acceptor, if one does.
    pub rung: Option<Role>,
    /// Does any protection require a review?
    pub review: bool,
}

/// What an acceptance lacks of what the protections ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lack {
    /// The acceptor's rung is below the one asked.
    Rung(Role),
    /// A review is asked and the latest revision has no approval.
    Review,
}

impl Demand {
    /// The demand of these notices: the highest rung, and a review if any asks for one.
    pub fn of(notices: &[Notice]) -> Demand {
        let mut demand = Demand::default();
        for notice in notices {
            match notice.require() {
                Require::Rung(rung) => demand.rung = demand.rung.max(Some(rung)),
                Require::Review => demand.review = true,
            }
        }
        demand
    }

    /// Does nothing ask anything?
    pub fn is_empty(&self) -> bool {
        self.rung.is_none() && !self.review
    }

    /// Does an acceptor who holds `rung` meet the rung asked? (No rung at all meets none.)
    pub fn rung_met(&self, rung: Option<Role>) -> bool {
        self.rung
            .is_none_or(|asked| rung.is_some_and(|held| held >= asked))
    }

    /// What is lacking, the rung before the review, for an acceptor who holds `rung` and a proposal that has `approvals`
    /// counted approvals; `None` when everything asked is met.
    pub fn lacks(&self, rung: Option<Role>, approvals: usize) -> Option<Lack> {
        if !self.rung_met(rung) {
            return self.rung.map(Lack::Rung);
        }
        (self.review && approvals == 0).then_some(Lack::Review)
    }
}

#[cfg(test)]
mod tests;
