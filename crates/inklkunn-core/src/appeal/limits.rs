//! The quotas of appeals (RFC 006 D-9, RFC 004 D-11): **appeals cannot flood the rung that decides them.**
//!
//! Three limits, **checked at the boundary** from the records, as the proposals' are (`proposal::quota`), refused by name with the
//! same typed refusal ([`QuotaRefusal`](crate::proposal::QuotaRefusal)):
//!
//! - **a person's rate**: at most [`AppealLimits::per_person_per_day`] appeals filed by one person in a wiki in a rolling day
//!   (`appeals-per-day-per-principal`); the refusal says when the oldest one leaves the day and another fits;
//! - **a person's open bound**: at most [`AppealLimits::open_per_person`] of one person's appeals waiting for a decision in a
//!   wiki (`open-appeals-per-person`), so that a few accounts cannot fill the wiki's open appeals and shut the others out (A-6);
//!   the refusal says it clears when one of *theirs* is decided;
//! - **a wiki's open bound**: at most [`AppealLimits::open_per_wiki`] appeals waiting for a decision in a wiki
//!   (`open-appeals-per-wiki`), counted over every person's, so that the queue of the rung that decides is bounded whoever
//!   fills it. As the proposals' per-wiki limit does, the refusal does **not** say how many there are (it must not be a way to watch
//!   other people's activity): the count it shows is the limit itself.
//!
//! The operator's settings are `INKLKUNN_APPEAL_PER_PERSON_PER_DAY`, `INKLKUNN_APPEAL_OPEN_PER_PERSON` and
//! `INKLKUNN_APPEAL_OPEN_PER_WIKI`
//! (`inklkunn`'s `config.rs`, which holds them to the ranges below).

/// The most appeals one person files in a wiki in a rolling day, unless the operator says otherwise: an appeal is a grievance,
/// and a person with more than three in a day has a problem a decision will not fix.
pub const PER_DAY_DEFAULT: u64 = 3;
/// The most the operator may allow one person per day.
pub const PER_DAY_MAX: u64 = 100;
/// The most appeals one person holds open in a wiki at once, unless the operator says otherwise.
pub const OPEN_PER_PERSON_DEFAULT: u64 = 5;
/// The most the operator may allow one person to hold open.
pub const OPEN_PER_PERSON_MAX: u64 = 50;
/// The most appeals a wiki holds open at once, unless the operator says otherwise.
pub const OPEN_PER_WIKI_DEFAULT: u64 = 50;
/// The most the operator may allow open in a wiki: the store reads a page of 200 at most, and the bound is counted from it.
pub const OPEN_PER_WIKI_MAX: u64 = inklkunn_store::governance::MAX_PAGE as u64;

/// The operator's two limits on appeals. `Default` is the documented one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppealLimits {
    /// Appeals one person may file in a wiki in a rolling day (1 to [`PER_DAY_MAX`]).
    pub per_person_per_day: u64,
    /// Appeals one person may hold open in a wiki (1 to [`OPEN_PER_PERSON_MAX`]).
    pub open_per_person: u64,
    /// Appeals a wiki may hold open (1 to [`OPEN_PER_WIKI_MAX`]).
    pub open_per_wiki: u64,
}

impl Default for AppealLimits {
    fn default() -> Self {
        AppealLimits {
            per_person_per_day: PER_DAY_DEFAULT,
            open_per_person: OPEN_PER_PERSON_DEFAULT,
            open_per_wiki: OPEN_PER_WIKI_DEFAULT,
        }
    }
}
