//! The approvals warning (RFC 006 D-4): a policy whose `acceptance.approvals` exceeds the number of people who could
//! give them today is **accepted**, with a warning rather than a refusal, because grants change and a refusal at
//! acceptance would be wrong a day later. The policy page and each proposal's page then say that *no proposal can
//! be accepted until more people hold the rung*. Computed on read from facts the caller counts; nothing is stored.

use super::Policy;

/// The data of the warning sentence: how many approvals the policy needs, and how many people could give them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApprovalsWarning {
    /// `acceptance.approvals`.
    pub needed: u8,
    /// The people who hold the review rung or above and are not the proposer.
    pub available: usize,
}

/// Does the policy need more approvals than `holders_at_review_or_above_not_proposer` people could give? The caller
/// counts the accounts that hold `rungs.review` or above, leaving out the proposer (for the policy page, no proposer
/// is left out). With no approvals needed it is never so.
pub fn approvals_unreachable(
    policy: &Policy,
    holders_at_review_or_above_not_proposer: usize,
) -> bool {
    usize::from(policy.acceptance().approvals()) > holders_at_review_or_above_not_proposer
}

/// The warning, when [`approvals_unreachable`].
pub fn approvals_warning(
    policy: &Policy,
    holders_at_review_or_above_not_proposer: usize,
) -> Option<ApprovalsWarning> {
    approvals_unreachable(policy, holders_at_review_or_above_not_proposer).then(|| {
        ApprovalsWarning {
            needed: policy.acceptance().approvals(),
            available: holders_at_review_or_above_not_proposer,
        }
    })
}
