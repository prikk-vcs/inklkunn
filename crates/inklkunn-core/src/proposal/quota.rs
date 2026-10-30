//! Quotas and rate limits (RFC 004 D-11; PR-14, C-9b), **checked at the boundary** and read from the records:
//! a refusal names the limit, the current value, what was asked and when it will be possible again, so that
//! a person (or an API client's `Retry-After`) knows what to do next.
//!
//! The defaults are the RFC's; the operator changes them in [`QuotaConfig`] (wiring it into the binary's
//! settings is handoff 004 T7/T8). **A new account has half** of every per-principal limit, for the first
//! [`QuotaConfig::new_account_secs`] after its creation. Rate limits are rolling windows over the records of
//! what was opened or accepted; a window is half-open, `(now - window, now]`, so a record stops counting at
//! exactly `its time + window`, which is the reset time a refusal gives.

use std::sync::Arc;

use inklkunn_store::{AcceptanceStore, AccountId, AccountStore, ProposalStore, WikiId};

use super::ProposalError;

/// The operator's limits. `Default` is RFC 004 D-11.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaConfig {
    /// Open proposals (drafts included) per principal per wiki.
    pub open_proposals: u64,
    /// Revisions per proposal.
    pub revisions_per_proposal: u64,
    /// Bytes of a principal's accepted changes per wiki in a rolling day.
    pub accepted_bytes_per_day: u64,
    /// Proposals a principal may open in a rolling hour, per wiki.
    pub proposals_per_hour_per_principal: u64,
    /// Proposals all principals together may open in a wiki in a rolling hour.
    pub proposals_per_hour_per_wiki: u64,
    /// Acceptances that may wait for a wiki's lease (read by the acceptance lease, handoff 004 T4).
    pub acceptance_queue: u64,
    /// How long after its creation an account counts as new, in seconds.
    pub new_account_secs: i64,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        QuotaConfig {
            open_proposals: 20,
            revisions_per_proposal: 50,
            accepted_bytes_per_day: 8 * 1024 * 1024,
            proposals_per_hour_per_principal: 30,
            proposals_per_hour_per_wiki: 300,
            acceptance_queue: 64,
            // The RFC says "new accounts" without saying how long an account is new; a week is the
            // architect's to confirm (reported with handoff 004 T1).
            new_account_secs: 7 * 24 * 3600,
        }
    }
}

/// The length of the hourly window, in seconds.
const HOUR: i64 = 3600;
/// The length of the daily window, in seconds.
const DAY: i64 = 24 * 3600;

/// Which limit refused. The names are fixed: a glossary row will give each its words (handoff 004 T3/T8,
/// "refused: the named limit").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuotaLimit {
    /// Too many open proposals.
    OpenProposals,
    /// Too many revisions on one proposal.
    RevisionsPerProposal,
    /// Too many bytes accepted in a day.
    AcceptedBytesPerDay,
    /// Too many proposals opened by one principal in an hour.
    ProposalsPerHourPerPrincipal,
    /// Too many proposals opened in the wiki in an hour.
    ProposalsPerHourPerWiki,
    /// Too many acceptances waiting for the wiki's lease.
    AcceptanceQueue,
}

impl QuotaLimit {
    /// Every limit.
    pub const ALL: [QuotaLimit; 6] = [
        QuotaLimit::OpenProposals,
        QuotaLimit::RevisionsPerProposal,
        QuotaLimit::AcceptedBytesPerDay,
        QuotaLimit::ProposalsPerHourPerPrincipal,
        QuotaLimit::ProposalsPerHourPerWiki,
        QuotaLimit::AcceptanceQueue,
    ];

    /// The fixed name (stable: logs, the API's problem type and the glossary key are made from it).
    pub const fn name(self) -> &'static str {
        match self {
            QuotaLimit::OpenProposals => "open-proposals",
            QuotaLimit::RevisionsPerProposal => "revisions-per-proposal",
            QuotaLimit::AcceptedBytesPerDay => "accepted-bytes-per-day",
            QuotaLimit::ProposalsPerHourPerPrincipal => "proposals-per-hour-per-principal",
            QuotaLimit::ProposalsPerHourPerWiki => "proposals-per-hour-per-wiki",
            QuotaLimit::AcceptanceQueue => "acceptance-queue",
        }
    }
}

/// When a refused act will be possible again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reset {
    /// At this Unix second, if nothing else is done in the meantime.
    At(i64),
    /// When one of the principal's open proposals ends (accepted, declined, withdrawn or superseded).
    WhenAProposalCloses,
    /// When a waiting acceptance gets its turn or gives up.
    WhenTheQueueMoves,
    /// Not by waiting: the proposal has all the revisions it can have (open a new one), or one request is
    /// larger than the limit allows at all (make it smaller).
    Never,
}

/// A limit that was reached: the typed refusal of D-11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaRefusal {
    /// Which limit.
    pub limit: QuotaLimit,
    /// What the limit allows this principal (already halved for a new account).
    pub allowed: u64,
    /// The value now: what is counted against the limit.
    pub current: u64,
    /// What the refused act would add (1 for a count, the bytes for a size).
    pub asked: u64,
    /// When it will be possible again.
    pub reset: Reset,
}

/// Checks the limits against the records. Cheap to share.
pub struct Quotas {
    config: QuotaConfig,
    proposals: Arc<dyn ProposalStore>,
    acceptances: Arc<dyn AcceptanceStore>,
    accounts: Arc<dyn AccountStore>,
    clock: fn() -> i64,
}

impl std::fmt::Debug for Quotas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Quotas")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn refuse(
    limit: QuotaLimit,
    allowed: u64,
    current: u64,
    asked: u64,
    reset: Reset,
) -> ProposalError {
    ProposalError::Quota(QuotaRefusal {
        limit,
        allowed,
        current,
        asked,
        reset,
    })
}

impl Quotas {
    /// Limits `config`, read from these records.
    pub fn new(
        config: QuotaConfig,
        proposals: Arc<dyn ProposalStore>,
        acceptances: Arc<dyn AcceptanceStore>,
        accounts: Arc<dyn AccountStore>,
    ) -> Self {
        Quotas {
            config,
            proposals,
            acceptances,
            accounts,
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    /// The limits in force.
    pub fn config(&self) -> &QuotaConfig {
        &self.config
    }

    /// What a per-principal limit of `base` allows `who`: half (at least one) while the account is new. An
    /// account that cannot be found counts as new.
    fn allowed_for(&self, who: &AccountId, base: u64) -> Result<u64, ProposalError> {
        let created = self.accounts.account(who)?.map(|a| a.created_at);
        let new = created
            .is_none_or(|at| (self.clock)().saturating_sub(at) < self.config.new_account_secs);
        Ok(if new { (base / 2).max(1) } else { base })
    }

    /// May `author` have another open proposal in the wiki?
    pub fn check_open(&self, wiki: &WikiId, author: &AccountId) -> Result<(), ProposalError> {
        let allowed = self.allowed_for(author, self.config.open_proposals)?;
        let current = self.proposals.open_count(wiki, author)?;
        if current >= allowed {
            return Err(refuse(
                QuotaLimit::OpenProposals,
                allowed,
                current,
                1,
                Reset::WhenAProposalCloses,
            ));
        }
        Ok(())
    }

    /// May `author` open another proposal in the wiki this hour, and may the wiki's contributors together?
    /// The principal's limit is asked first: it is the one the person can do something about.
    pub fn check_opening_rate(
        &self,
        wiki: &WikiId,
        author: &AccountId,
    ) -> Result<(), ProposalError> {
        let now = (self.clock)();
        let since = now - HOUR + 1;
        let allowed = self.allowed_for(author, self.config.proposals_per_hour_per_principal)?;
        let mine = self.proposals.opened_since(wiki, Some(author), since)?;
        if let Some(reset) = rolling_reset(&mine, allowed, HOUR) {
            return Err(refuse(
                QuotaLimit::ProposalsPerHourPerPrincipal,
                allowed,
                count(&mine),
                1,
                reset,
            ));
        }
        let allowed = self.config.proposals_per_hour_per_wiki;
        let all = self.proposals.opened_since(wiki, None, since)?;
        if let Some(reset) = rolling_reset(&all, allowed, HOUR) {
            return Err(refuse(
                QuotaLimit::ProposalsPerHourPerWiki,
                allowed,
                count(&all),
                1,
                reset,
            ));
        }
        Ok(())
    }

    /// May a proposal that already has `revisions` revisions have another, written by `author`?
    pub fn check_revisions(&self, author: &AccountId, revisions: u64) -> Result<(), ProposalError> {
        let allowed = self.allowed_for(author, self.config.revisions_per_proposal)?;
        if revisions >= allowed {
            return Err(refuse(
                QuotaLimit::RevisionsPerProposal,
                allowed,
                revisions,
                1,
                Reset::Never,
            ));
        }
        Ok(())
    }

    /// May `bytes` more of `proposer`'s changes be accepted in the wiki today? Read by the acceptance (handoff
    /// 004 T4), from the acceptance records: those pending seal and those sealed count, a failed one took
    /// nothing in.
    pub fn check_accepted_bytes(
        &self,
        wiki: &WikiId,
        proposer: &AccountId,
        bytes: u64,
    ) -> Result<(), ProposalError> {
        let now = (self.clock)();
        let allowed = self.allowed_for(proposer, self.config.accepted_bytes_per_day)?;
        let taken = self
            .acceptances
            .accepted_bytes_since(wiki, proposer, now - DAY + 1)?;
        let current: u64 = taken.iter().map(|(_, b)| *b).sum();
        if current.saturating_add(bytes) <= allowed {
            return Ok(());
        }
        // The earliest moment enough of the day's acceptances have aged out for `bytes` to fit.
        let mut remaining = current;
        let mut reset = Reset::Never;
        for (at, b) in &taken {
            remaining = remaining.saturating_sub(*b);
            if remaining.saturating_add(bytes) <= allowed {
                reset = Reset::At(at + DAY);
                break;
            }
        }
        Err(refuse(
            QuotaLimit::AcceptedBytesPerDay,
            allowed,
            current,
            bytes,
            reset,
        ))
    }

    /// May another acceptance wait for the wiki's lease while `waiting` already do?
    pub fn check_queue(&self, waiting: u64) -> Result<(), ProposalError> {
        let allowed = self.config.acceptance_queue;
        if waiting >= allowed {
            return Err(refuse(
                QuotaLimit::AcceptanceQueue,
                allowed,
                waiting,
                1,
                Reset::WhenTheQueueMoves,
            ));
        }
        Ok(())
    }
}

fn count(times: &[i64]) -> u64 {
    u64::try_from(times.len()).unwrap_or(u64::MAX)
}

/// For a rolling window of `window` seconds holding `times` (oldest first, all inside it) against a limit of
/// `allowed`: `None` when one more fits, else the moment it will, when enough of the oldest have aged out.
fn rolling_reset(times: &[i64], allowed: u64, window: i64) -> Option<Reset> {
    let n = count(times);
    if n < allowed {
        return None;
    }
    // One more fits when `n + 1 - k <= allowed`, that is after the `k = n + 1 - allowed` oldest have left.
    let k = usize::try_from(n + 1 - allowed).unwrap_or(usize::MAX);
    let oldest_to_leave = k.checked_sub(1).and_then(|i| times.get(i));
    Some(oldest_to_leave.map_or(Reset::Never, |t| Reset::At(t + window)))
}
