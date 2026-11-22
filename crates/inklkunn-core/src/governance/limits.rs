//! The platform constitution (RFC 006 D-3): what the operator allows any wiki.
//!
//! Two settings: `INKLKUNN_GOVERNANCE_ENTRY_MAX` (the highest `entry` a wiki may set) and
//! `INKLKUNN_GOVERNANCE_LIMIT_DAYS` (the longest temporary protection or restriction). The binary reads them from the
//! environment ([`PlatformLimits`] is the value; the reading and its start-up refusals are `inklkunn`'s `config`), so
//! this module is pure.
//!
//! **A policy that exceeds a limit is refused at acceptance**, naming the limit ([`Refusal::PlatformLimit`]). **If
//! the operator later lowers a limit, a wiki above it is read as if it were at the limit**
//! ([`Policy::as_read_under`]), and the policy page says which values were lowered ([`Lowered`]).

use super::defaults;
use super::fault::{Fault, GovernanceFile, Limit, Refusal};
use super::{Entry, Policy};

/// The fewest days the operator may set as [`PlatformLimits::limit_days`].
pub const LIMIT_DAYS_MIN: u32 = 1;

/// The most days the operator may set as [`PlatformLimits::limit_days`] (ten years).
pub const LIMIT_DAYS_MAX: u32 = 3650;

/// What the operator allows any wiki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformLimits {
    /// `INKLKUNN_GOVERNANCE_ENTRY_MAX`: the highest `entry` a wiki may set. Default contributor.
    pub entry_max: Entry,
    /// `INKLKUNN_GOVERNANCE_LIMIT_DAYS`: the longest temporary protection or restriction, in days
    /// ([`LIMIT_DAYS_MIN`] to [`LIMIT_DAYS_MAX`]). Default 90. A standing protection has no expiry, so no policy
    /// exceeds it yet; G-T2 and G-T3 ask [`PlatformLimits::allows_days`].
    pub limit_days: u32,
}

impl Default for PlatformLimits {
    fn default() -> Self {
        PlatformLimits {
            entry_max: defaults::PLATFORM_ENTRY_MAX,
            limit_days: defaults::PLATFORM_LIMIT_DAYS,
        }
    }
}

impl PlatformLimits {
    /// Is a temporary protection or restriction of `days` days within the limit?
    pub const fn allows_days(&self, days: u32) -> bool {
        days <= self.limit_days
    }

    /// Does a policy stay within the limits? `Err` names the limit it exceeds. This is the **hook** for every limit
    /// a policy can exceed: only `entry` now; a period in a policy would be checked here.
    pub(super) fn check(&self, policy: &Policy) -> Result<(), Fault> {
        if policy.entry > self.entry_max {
            return Err(Fault::at(
                GovernanceFile::Policy,
                Refusal::PlatformLimit(Limit::EntryMax),
                "entry",
            ));
        }
        Ok(())
    }
}

/// A value of a policy that a platform limit lowered when the policy was read as if it were at the limit: what the
/// policy page says, so that nobody is told the policy says one thing and is governed by another (RFC 006 D-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lowered {
    /// `entry` was `was` and is read as `now`, by `INKLKUNN_GOVERNANCE_ENTRY_MAX`.
    Entry {
        /// What the policy says.
        was: Entry,
        /// What it is read as: the limit.
        now: Entry,
    },
}

impl Lowered {
    /// The limit that lowered it.
    pub const fn limit(&self) -> Limit {
        match self {
            Lowered::Entry { .. } => Limit::EntryMax,
        }
    }

    /// The key of the policy that was lowered.
    pub const fn field(&self) -> &'static str {
        match self {
            Lowered::Entry { .. } => "entry",
        }
    }
}

impl Policy {
    /// This policy **as it is in force under `limits`**: read as if it were at the limit wherever it is above one,
    /// with a [`Lowered`] for each value that was lowered (empty when the policy is within the limits, which is the
    /// case for every policy accepted under them). Pure, and idempotent: the result is within the limits and
    /// lowers nothing further.
    ///
    /// A policy comes to be above a limit only when the operator lowers the limit after the policy was accepted;
    /// acceptance refuses one that is above it ([`validate_policy`](super::validate_policy)).
    pub fn as_read_under(&self, limits: &PlatformLimits) -> (Policy, Vec<Lowered>) {
        let mut policy = self.clone();
        let mut lowered = Vec::new();
        if policy.entry > limits.entry_max {
            lowered.push(Lowered::Entry {
                was: policy.entry,
                now: limits.entry_max,
            });
            policy.entry = limits.entry_max;
        }
        (policy, lowered)
    }
}
