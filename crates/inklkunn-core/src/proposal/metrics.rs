//! The participation metrics (RFC 004 D-10; PR-13, register item 18), **computed from the records alone**:
//! proposals by state, the median time to the first review and to a decision, the acceptance and decline
//! rates, the distinct and the first-time proposers, and the reviewers per proposal. A pure function over the
//! proposals and decisions of a wiki, so a hand count on a small fixture is the test. Exposing them (the
//! administration surface: `inklkunn wiki metrics`, one JSON Lines record per wiki and period, [`Metrics::json_line`]) is handoff 004 T8; the dashboard that interprets them is RFC 006's.
//!
//! **What is counted.** The *cohort* is the proposals **submitted** in the period `[from, to)`: a draft that
//! was never submitted is private to its author and appears nowhere, not even in a count. A *review* is a
//! decision by someone other than the proposal's author (a decision on one's own proposal is recorded, but it
//! is not a second pair of eyes). The *time to the first review* is from submission to the earliest review of
//! any revision; the *time to a decision* is from submission to the end of a proposal that was **accepted or
//! declined** (a withdrawn or a superseded proposal was not decided). The two *rates* are shares of the
//! proposals that were decided (accepted plus declined). Medians are the nearest rank, the lower of the two
//! middle values for an even count, so every figure is a whole number of seconds. A *first-time proposer* is
//! an author whose first-ever submission, over the whole record, falls in the period.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_store::{AccountId, DecisionRecord, ProposalRecord, ProposalState, WikiId};

/// A half-open period of Unix seconds, `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    /// The first second in the period.
    pub from: i64,
    /// The first second after it.
    pub to: i64,
}

impl Period {
    fn holds(self, at: i64) -> bool {
        self.from <= at && at < self.to
    }
}

/// A share kept as the two whole numbers it comes from, so no figure is rounded before a person reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    /// The part.
    pub num: u64,
    /// The whole; never 0 (a share of nothing is `None`, not a ratio).
    pub den: u64,
}

/// The figures for one wiki over one period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    /// The wiki.
    pub wiki: WikiId,
    /// The period.
    pub period: Period,
    /// The proposals submitted in the period, by the state they are in now. Every state is present; a draft
    /// is always 0 (a submitted proposal never goes back to a draft).
    pub by_state: BTreeMap<ProposalState, u64>,
    /// How many were submitted in the period.
    pub submitted: u64,
    /// The median seconds from submission to the first review, over those that have had one.
    pub median_secs_to_first_review: Option<i64>,
    /// The median seconds from submission to the end, over those that were accepted or declined.
    pub median_secs_to_decision: Option<i64>,
    /// Accepted, of the proposals that were decided.
    pub acceptance_rate: Option<Ratio>,
    /// Declined, of the proposals that were decided.
    pub decline_rate: Option<Ratio>,
    /// How many different people submitted.
    pub distinct_proposers: u64,
    /// Of them, how many submitted for the first time.
    pub first_time_proposers: u64,
    /// The reviewers a proposal had, on average: the distinct reviewers summed over the proposals, of the
    /// proposals submitted.
    pub reviewers_per_proposal: Option<Ratio>,
}

fn count<T>(items: &[T]) -> u64 {
    u64::try_from(items.len()).unwrap_or(u64::MAX)
}

/// The nearest-rank median: the lower of the two middle values when there is an even number.
fn median(mut values: Vec<i64>) -> Option<i64> {
    values.sort_unstable();
    let middle = values.len().checked_sub(1)? / 2;
    values.get(middle).copied()
}

/// The figures for `wiki` over `period`, from its submitted `proposals` (all time: first-time needs the
/// earliest) and their `decisions`.
pub fn compute(
    wiki: &WikiId,
    period: Period,
    proposals: &[ProposalRecord],
    decisions: &[DecisionRecord],
) -> Metrics {
    let submitted_in = |p: &&ProposalRecord| p.submitted_at.is_some_and(|at| period.holds(at));
    let cohort: Vec<&ProposalRecord> = proposals
        .iter()
        .filter(|p| p.wiki == *wiki)
        .filter(submitted_in)
        .collect();

    let mut by_state: BTreeMap<ProposalState, u64> =
        ProposalState::ALL.into_iter().map(|s| (s, 0)).collect();
    for p in &cohort {
        *by_state.entry(p.state).or_insert(0) += 1;
    }

    // The reviews of each proposal: its decisions by someone other than its author.
    let reviews = |p: &ProposalRecord| -> Vec<&DecisionRecord> {
        decisions
            .iter()
            .filter(|d| d.proposal == p.id && d.reviewer != p.author)
            .collect()
    };

    let first_review: Vec<i64> = cohort
        .iter()
        .filter_map(|p| {
            let submitted = p.submitted_at?;
            let first = reviews(p).iter().map(|d| d.created_at).min()?;
            Some((first - submitted).max(0))
        })
        .collect();

    let decided: Vec<&&ProposalRecord> = cohort
        .iter()
        .filter(|p| matches!(p.state, ProposalState::Accepted | ProposalState::Declined))
        .collect();
    let to_decision: Vec<i64> = decided
        .iter()
        .filter_map(|p| Some((p.closed_at? - p.submitted_at?).max(0)))
        .collect();
    let share = |state: ProposalState| {
        let den = count(&decided);
        let num = count(
            &decided
                .iter()
                .filter(|p| p.state == state)
                .collect::<Vec<_>>(),
        );
        (den > 0).then_some(Ratio { num, den })
    };

    let proposers: BTreeSet<&AccountId> = cohort.iter().map(|p| &p.author).collect();
    let first_time = proposers
        .iter()
        .filter(|author| {
            let earliest = proposals
                .iter()
                .filter(|p| p.wiki == *wiki && p.author == ***author)
                .filter_map(|p| p.submitted_at)
                .min();
            earliest.is_some_and(|at| period.holds(at))
        })
        .count();

    let reviewers: u64 = cohort
        .iter()
        .map(|p| {
            let distinct: BTreeSet<&AccountId> = reviews(p).iter().map(|d| &d.reviewer).collect();
            count(&distinct.into_iter().collect::<Vec<_>>())
        })
        .sum();
    let submitted = count(&cohort);

    Metrics {
        wiki: wiki.clone(),
        period,
        by_state,
        submitted,
        median_secs_to_first_review: median(first_review),
        median_secs_to_decision: median(to_decision),
        acceptance_rate: share(ProposalState::Accepted),
        decline_rate: share(ProposalState::Declined),
        distinct_proposers: count(&proposers.iter().collect::<Vec<_>>()),
        first_time_proposers: u64::try_from(first_time).unwrap_or(u64::MAX),
        reviewers_per_proposal: (submitted > 0).then_some(Ratio {
            num: reviewers,
            den: submitted,
        }),
    }
}

impl Metrics {
    /// The figures as **one line of JSON** (JSON Lines, CT-08): a record per wiki and period, ready to append to a
    /// file. Times are whole seconds, a share is `{"num","den"}` (no figure is rounded), an absent figure is `null`,
    /// every state is present, and the line carries nothing but numbers, the wiki's id and fixed words.
    pub fn json_line(&self) -> String {
        use serde_json::{Map, Value, json};
        let ratio = |r: &Option<Ratio>| match r {
            Some(r) => json!({ "num": r.num, "den": r.den }),
            None => Value::Null,
        };
        let by_state: Map<String, Value> = self
            .by_state
            .iter()
            .map(|(state, n)| (state.as_str().to_owned(), json!(n)))
            .collect();
        let line = json!({
            "kind": "participation",
            "wiki": self.wiki.as_str(),
            "from": self.period.from,
            "to": self.period.to,
            "submitted": self.submitted,
            "by_state": by_state,
            "median_secs_to_first_review": self.median_secs_to_first_review,
            "median_secs_to_decision": self.median_secs_to_decision,
            "acceptance_rate": ratio(&self.acceptance_rate),
            "decline_rate": ratio(&self.decline_rate),
            "distinct_proposers": self.distinct_proposers,
            "first_time_proposers": self.first_time_proposers,
            "reviewers_per_proposal": ratio(&self.reviewers_per_proposal),
        });
        line.to_string()
    }
}

/// The metrics of `wiki` over `period` **for the operator at the command line** (the administration surface of
/// RFC 004 D-10): the same computation as [`ProposalService::metrics`](super::ProposalService::metrics), without a
/// principal, because the command line's authority is the operator's own access to the host. Drafts are in no
/// figure.
pub fn operator_metrics(
    proposals: &dyn inklkunn_store::ProposalStore,
    wiki: &inklkunn_store::WikiRecord,
    period: Period,
) -> Result<Metrics, inklkunn_store::StoreError> {
    let all = proposals.proposals(&wiki.id, &inklkunn_store::ProposalFilter::default(), None)?;
    let decisions = proposals.wiki_decisions(&wiki.id)?;
    Ok(compute(&wiki.id, period, &all, &decisions))
}
