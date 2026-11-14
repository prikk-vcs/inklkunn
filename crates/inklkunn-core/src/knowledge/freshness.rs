//! Freshness: which sources a person should look at again (RFC 005 part 2, D-10; requirement KN-8; handoff K-T9).
//!
//! A source's `last_reviewed` is set by a curation proposal, by a person who checked, and so is its `availability`
//! (`unavailable` is a person's word, never a crawler's). **Nothing here fetches anything**: this reads the derived
//! knowledge index of a tip and a day, and names
//!
//! - every source marked `unavailable`;
//! - every source with **no** `last_reviewed` (*never reviewed*);
//! - every source whose `last_reviewed` is **older than the setting** (`knowledge.review_after`, three years unless
//!   the operator says otherwise; one value for every wiki of the instance until RFC 006's policy as data);
//! - every source whose `last_reviewed` is a day that **has not come**: a person cannot have looked on it, and a
//!   mistyped year would otherwise keep the source out of the list for years (K40);
//!
//! each with the claims that rely on it. The list is **not a ranking and not a verdict**: a listed source is one a
//! person has not looked at lately, which says nothing about whether it is right, and an unlisted one is not
//! thereby vouched for.
//!
//! **Time is a parameter.** The day comes from the caller (the read service's injected clock), never from here,
//! so a test fixes it. A source is *older than the setting* when more than `review_after_days` whole days have
//! passed since the day it was reviewed: reviewed exactly `review_after_days` days ago, it is not yet listed. A day
//! in the future (a mistyped year) is not old, and is listed on its own reason ([`LastReview::Ahead`]). A `last_reviewed` the index holds that does not read as a day (a row
//! written by another build) is read as *never reviewed*, so it is listed rather than trusted.
//!
//! **Bounded.** The rows are the index's sources (bounded as the index is, by the file count); the claims named
//! beside a source are cut at [`MAX_RELYING`] with their whole count kept. The index finds the links of a source by
//! a map built once, and the list is computed **once for a tip, a day and a setting** and kept with the index
//! ([`sources_to_review_kept`]): a reader who asks again, or asks for the next page, costs a slice of it (K40).

use std::sync::{Arc, PoisonError};

use inklkunn_store::{KnowledgeLinkRow, KnowledgeSourceRow};

use super::index::KnowledgeIndex;

/// The setting's default: three years, counting one leap day (`knowledge.review_after`, in days).
pub const DEFAULT_REVIEW_AFTER_DAYS: u32 = 1096;
/// The largest setting accepted: a hundred years, so that a mistyped number cannot switch the list off in
/// effect without saying so, and the day arithmetic stays far from any overflow.
pub const MAX_REVIEW_AFTER_DAYS: u32 = 36_500;
/// The most claims named beside one source; the whole count is kept with them.
pub const MAX_RELYING: usize = 50;

/// When a source was last reviewed, against the setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastReview {
    /// No person has said they checked it.
    Never,
    /// A person checked it on this day (as written, `YYYY-MM-DD`), which has come.
    On {
        /// The day, as the file has it.
        day: String,
        /// More than the setting's days ago.
        overdue: bool,
    },
    /// The file says a person checked it on a day that has not come yet: no one can have, so it counts as no look
    /// (K40).
    Ahead {
        /// The day, as the file has it.
        day: String,
    },
}

/// A claim that relies on a source, and how the claim says the source bears on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelyingClaim {
    /// The claim's id.
    pub claim: String,
    /// `supports` or `challenges`.
    pub relation: String,
    /// `direct`, `partial` or `indirect`: how directly, never how true.
    pub bearing: String,
}

/// A source that wants a person's look, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceToReview {
    /// The source, as the index has it.
    pub source: KnowledgeSourceRow,
    /// A person who checked marked it as unavailable.
    pub unavailable: bool,
    /// When it was last reviewed.
    pub review: LastReview,
    /// The claims that rely on it, by claim id, at most [`MAX_RELYING`].
    pub claims: Vec<RelyingClaim>,
    /// How many claims rely on it in all.
    pub claims_total: usize,
}

impl SourceToReview {
    /// The reasons it is listed, as stable words: `unavailable`, `never-reviewed`, `overdue`, `review-in-future`. At
    /// least one.
    pub fn reasons(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.unavailable {
            out.push("unavailable");
        }
        match &self.review {
            LastReview::Never => out.push("never-reviewed"),
            LastReview::On { overdue: true, .. } => out.push("overdue"),
            LastReview::Ahead { .. } => out.push("review-in-future"),
            LastReview::On { .. } => {}
        }
        out
    }
}

/// The list of one tip, with the index it was read from (a page names the claims and the sources from it) and the
/// setting it was judged by.
#[derive(Debug, Clone)]
pub struct SourcesToReview {
    /// The knowledge index of the tip.
    pub index: Arc<KnowledgeIndex>,
    /// The sources that want a look, by source id.
    pub rows: Vec<SourceToReview>,
    /// `knowledge.review_after`, in days, as the list was read under.
    pub review_after_days: u32,
}

/// The sources of `index` that want a look at `now` (Unix seconds, UTC) under a setting of `review_after_days`, by
/// source id. Reads the index and nothing else.
pub fn sources_to_review(
    index: &KnowledgeIndex,
    now: i64,
    review_after_days: u32,
) -> Vec<SourceToReview> {
    let today = now.div_euclid(86_400);
    let limit = i64::from(review_after_days);
    index
        .sources()
        .iter()
        .filter_map(|source| {
            let unavailable = source.availability == "unavailable";
            let review = match source.last_reviewed.as_deref().and_then(day_number) {
                None => LastReview::Never,
                // A day that has not come is not a look (K40).
                Some(reviewed) if reviewed > today => LastReview::Ahead {
                    day: source.last_reviewed.clone().unwrap_or_default(),
                },
                Some(reviewed) => LastReview::On {
                    day: source.last_reviewed.clone().unwrap_or_default(),
                    // Strictly more than the limit: reviewed `limit` days ago is not yet old.
                    overdue: today.saturating_sub(reviewed) > limit,
                },
            };
            let wanted = unavailable
                || matches!(
                    review,
                    LastReview::Never
                        | LastReview::Ahead { .. }
                        | LastReview::On { overdue: true, .. }
                );
            wanted.then(|| {
                let relying: Vec<&KnowledgeLinkRow> = index.claims_citing(&source.id).collect();
                SourceToReview {
                    source: source.clone(),
                    unavailable,
                    review,
                    claims_total: relying.len(),
                    claims: relying
                        .into_iter()
                        .take(MAX_RELYING)
                        .map(|l| RelyingClaim {
                            claim: l.claim.clone(),
                            relation: l.relation.clone(),
                            bearing: l.bearing.clone(),
                        })
                        .collect(),
                }
            })
        })
        .collect()
}

/// The list kept with an index: for which day and setting it was computed, and the list.
#[derive(Debug)]
pub(super) struct Memo {
    day: i64,
    review_after_days: u32,
    list: Arc<Vec<SourceToReview>>,
}

/// [`sources_to_review`], computed **once** for the index, the day of `now` and the setting, and kept with the index
/// (K40): the next reader of the same day, and the next page of the same walk, share the list. The first reader of
/// a day computes it while the others wait, so the work is not repeated by a crowd.
pub fn sources_to_review_kept(
    index: &KnowledgeIndex,
    now: i64,
    review_after_days: u32,
) -> Arc<Vec<SourceToReview>> {
    let day = now.div_euclid(86_400);
    let mut memo = index
        .freshness_memo()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(kept) = memo
        .as_ref()
        .filter(|m| m.day == day && m.review_after_days == review_after_days)
    {
        return Arc::clone(&kept.list);
    }
    let list = Arc::new(sources_to_review(index, now, review_after_days));
    *memo = Some(Memo {
        day,
        review_after_days,
        list: Arc::clone(&list),
    });
    list
}

/// The number of days from 1970-01-01 to the day `YYYY-MM-DD`; `None` when the text is not a day in that form.
/// (A month of fewer days than the day names rolls over into the next, as the proleptic calendar's arithmetic
/// does: the file's validator accepts `31` for any month, and a day is only compared, never shown from this.)
fn day_number(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 10 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return None;
    }
    let part = |from: usize, to: usize| -> Option<i64> {
        let s = text.get(from..to)?;
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse::<i64>().ok())
            .flatten()
    };
    let (year, month, day) = (part(0, 4)?, part(5, 7)?, part(8, 10)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Howard Hinnant's `days_from_civil`: (year, month, day) → days since 1970-01-01, proleptic Gregorian.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
