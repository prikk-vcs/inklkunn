//! The defaults, in one place. **Every constant here is 0.3.0's behaviour**, and the test that holds the rule above
//! every task (RFC 006 acceptance criterion 1) reads them against a literal table written in the test, not derived
//! from this code.
//!
//! A wiki with no `governance/` files, and a file that leaves a key out, is decided by these values.

use inklkunn_store::Role;

use super::{AppealRung, Entry, GrantUpTo, SelfAcceptance};

/// `entry`: the rung every signed-in account holds in a wiki without a grant. **None**: in 0.3.0 an account holds
/// only the rung it was granted, and with none it may read what the wiki's visibility lets it read and do nothing
/// else.
pub const ENTRY: Entry = Entry::None;

/// `rungs.comment`: 0.3.0's `Action::Comment` needs a questioner (RFC 002 D-2).
pub const COMMENT: Role = Role::Questioner;

/// `rungs.propose`: 0.3.0's `Action::Propose` needs a contributor.
pub const PROPOSE: Role = Role::Contributor;

/// `rungs.review`: 0.3.0's `Action::Review` needs a reviewer.
pub const REVIEW: Role = Role::Reviewer;

/// `rungs.create_view`: 0.3.0's `Action::CreateView` needs a reviewer (the owner's ruling of 2026-10-31).
pub const CREATE_VIEW: Role = Role::Reviewer;

/// `rungs.accept`: 0.3.0's `Action::Accept` needs a maintainer.
pub const ACCEPT: Role = Role::Maintainer;

/// `rungs.moderate`: 0.3.0's `Action::Moderate` needs a maintainer.
pub const MODERATE: Role = Role::Maintainer;

/// `rungs.decide_appeal`: 0.3.0 has no appeals; the rung that decides one is the admin (RFC 006 D-2).
pub const DECIDE_APPEAL: AppealRung = AppealRung::Admin;

/// `rungs.grant_up_to`: in 0.3.0 only an admin grants a rung, so a maintainer may grant none.
pub const GRANT_UP_TO: GrantUpTo = GrantUpTo::None;

/// `acceptance.approvals`: 0.3.0 accepts a proposal on the accepter's word alone, with no approval counted.
pub const APPROVALS: u8 = 0;

/// `acceptance.self`: 0.3.0 lets a maintainer accept their own proposal while the wiki is small (RFC 004's
/// bootstrap rule).
pub const SELF_ACCEPTANCE: SelfAcceptance = SelfAcceptance::SmallWiki;

/// The number of maintainers below which a maintainer may accept their own proposal under `acceptance.self:
/// small-wiki`: three, as in 0.3.0.
///
/// **Written once, here**: `acceptance::SELF_ACCEPTANCE_BELOW` is this constant re-exported, and the acceptance judges a
/// maintainer's own proposal by [`SelfAcceptance::allows`](super::SelfAcceptance::allows) over it (a test holds the number
/// against a frozen copy of 0.3.0's).
pub const SELF_ACCEPTANCE_BELOW: usize = 3;

/// `knowledge.review_after`: no value of the wiki's own; the operator's `INKLKUNN_KNOWLEDGE_REVIEW_AFTER_DAYS` is
/// the interval (RFC 005 part 2 D-10; RFC 006 D-14).
pub const REVIEW_AFTER_DAYS: Option<u32> = None;

/// `INKLKUNN_GOVERNANCE_ENTRY_MAX`: the highest `entry` a wiki may set. Contributor, as RFC 006 D-3 says.
pub const PLATFORM_ENTRY_MAX: Entry = Entry::Contributor;

/// `INKLKUNN_GOVERNANCE_LIMIT_DAYS`: the longest temporary protection or restriction, in days (RFC 006 D-3).
pub const PLATFORM_LIMIT_DAYS: u32 = 90;

/// `policy_change.rung` of the constitution: a policy change needs a maintainer (RFC 006 D-1).
pub const POLICY_CHANGE_RUNG: Role = Role::Maintainer;

/// `policy_change.approvals` of the constitution: none (RFC 006 D-1).
pub const POLICY_CHANGE_APPROVALS: u8 = 0;

/// `constitution_change.approvals`: none (RFC 006 D-1). The rung is fixed at admin and is no key.
pub const CONSTITUTION_CHANGE_APPROVALS: u8 = 0;

/// The rung a constitution change needs. **Fixed**: nothing writes it, and no file can change it (INV-14).
pub const CONSTITUTION_CHANGE_RUNG: Role = Role::Admin;
