//! The review record of RFC 005 part 1 D-1 (d) (handoff 005 K3 b; store schema 16): **what reviewers did with the
//! default**, kept so that the participation metrics can say how often it was rejected.
//!
//! The default (H-1 point 2) is that a claim's paragraph, reworded, keeps the claim: its `^id` marker continues. For
//! every accepted change and every claim it **touches** (a paragraph the claim is said in is reworded or goes), the
//! record says which of two things happened:
//!
//! - *kept*: the paragraph is reworded and its marker continues, so the claim does too (the default);
//! - *dropped*: the paragraph, and with it the expression of the claim, is removed (a removal somebody confirmed).
//!
//! and, **later**, whether the claim was *re-confirmed*: the curation proposal that judged it again on the wording
//! it has now, naming the change it re-confirms.
//!
//! The default is rejected by a confirmed drop, or by a *request changes* decision on a proposal that touches a claim
//! (the claims that decision's revision touched are kept with the decision). The two are counted apart, a drop per claim
//! and a request per decision, and never summed (handoff 005 K21). These are counts of what people did; no figure here
//! says that anyone was right.
//!
//! **Written once, with the act.** A change's claim records are written after the acceptance record and before the
//! signer is asked, keyed by the acceptance; a record whose acceptance never sealed (it failed, or is still pending)
//! is never read: [`ProposalStore::claim_reviews`](crate::ProposalStore::claim_reviews) answers from sealed acceptances only. So a failure to record stops
//! the acceptance before anything is signed, and a retry writes its own records; nothing is lost and nothing is
//! signed twice. Nothing here is ever updated or removed (the SQLite store's triggers refuse an `UPDATE`, a `DELETE`
//! and a replacing `INSERT`), and nothing may be added once the acceptance it belongs to has ended.

use crate::Result;
use crate::record::{AcceptanceId, ProposalId};

/// The most claims one change's record, or one decision's, may hold: a bound against a runaway, equal to the most claims
/// a change is asked about (`inklkunn-core`'s `knowledge::onpage::MAX_TOUCHED`).
pub const MAX_CLAIMS: usize = 200;

/// What an accepted change did to a claim it touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClaimHow {
    /// The paragraph is reworded and its marker continues: the claim continues (the default, H-1 point 2).
    Kept,
    /// The paragraph is removed with the claim's expression in it (a removal somebody confirmed).
    Dropped,
}

impl ClaimHow {
    /// Every kind.
    pub const ALL: [ClaimHow; 2] = [ClaimHow::Kept, ClaimHow::Dropped];

    /// The stored name.
    pub const fn as_str(self) -> &'static str {
        match self {
            ClaimHow::Kept => "kept",
            ClaimHow::Dropped => "dropped",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        ClaimHow::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// What a change did to one claim, as it is handed to [`ProposalStore::record_claim_reviews`](
/// crate::ProposalStore::record_claim_reviews).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    /// The claim's id (a version-7 UUID in lower case with hyphens, the name of its file).
    pub claim: String,
    /// What happened.
    pub how: ClaimHow,
}

/// One accepted change's record for one claim, with the re-judgement that followed it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimReview {
    /// The acceptance that put the change in the history: the change.
    pub acceptance: AcceptanceId,
    /// The proposal that was accepted.
    pub proposal: ProposalId,
    /// The claim.
    pub claim: String,
    /// What the change did to it.
    pub how: ClaimHow,
    /// The acceptance of the curation proposal that re-confirmed the claim for this change, when one has (the first, if
    /// more than one did).
    pub reconfirmed_by: Option<AcceptanceId>,
}

/// Is `claim` the spelling of a claim's id: 36 characters of lower-case hexadecimal digits and hyphens? The store
/// checks the form and nothing else (the claim files are `inklkunn-core`'s).
pub fn is_claim_id(claim: &str) -> bool {
    claim.len() == 36
        && claim
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'-'))
}

/// Check a list of claims for a record: each well formed, none twice, at most [`MAX_CLAIMS`]. The same check in both
/// stores, so that both refuse the same thing.
pub(crate) fn check_claims<'a>(claims: impl ExactSizeIterator<Item = &'a str>) -> Result<()> {
    use crate::StoreError;
    if claims.len() > MAX_CLAIMS {
        return Err(StoreError::Backend(format!(
            "a record names at most {MAX_CLAIMS} claims"
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for claim in claims {
        if !is_claim_id(claim) {
            return Err(StoreError::Backend(
                "a record names a claim by its id".to_owned(),
            ));
        }
        if !seen.insert(claim) {
            return Err(StoreError::AlreadyRecorded);
        }
    }
    Ok(())
}
