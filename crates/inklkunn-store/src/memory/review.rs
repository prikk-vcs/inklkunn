//! The in-memory review record: the reference the SQLite store must agree with ([`crate::review`] says what it is).
//! The trait methods that call these are in [`super::proposal`].

use super::MemoryStore;
use crate::acceptance::{AcceptanceOutcome, AcceptanceRecord};
use crate::record::{AcceptanceId, WikiId};
use crate::review::{ClaimHow, ClaimOutcome, ClaimReview, check_claims};
use crate::{Result, StoreError};

impl MemoryStore {
    /// The wiki of the acceptance `id`, when it has sealed.
    pub(super) fn sealed_change_wiki(&self, id: &AcceptanceId) -> Option<WikiId> {
        let d = self.acceptances_lock();
        let a = d.acceptances.iter().find(|a| a.id == *id)?;
        d.results
            .iter()
            .any(|r| r.acceptance == *id && matches!(r.outcome, AcceptanceOutcome::Sealed { .. }))
            .then(|| a.wiki.clone())
    }

    pub(super) fn claim_reviews_write(
        &self,
        acceptance: &AcceptanceId,
        outcomes: &[ClaimOutcome],
    ) -> Result<()> {
        check_claims(outcomes.iter().map(|o| o.claim.as_str()))?;
        let mut d = self.acceptances_lock();
        if !d.acceptances.iter().any(|a| a.id == *acceptance) {
            return Err(StoreError::NotFound);
        }
        if d.results.iter().any(|r| r.acceptance == *acceptance) {
            return Err(StoreError::StateChanged);
        }
        if outcomes.iter().any(|o| {
            d.claims
                .iter()
                .any(|(a, c)| a == acceptance && c.claim == o.claim)
        }) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.claims
            .extend(outcomes.iter().map(|o| (acceptance.clone(), o.clone())));
        Ok(())
    }

    pub(super) fn reconfirmations_write(
        &self,
        by: &AcceptanceId,
        change: &AcceptanceId,
        claims: &[String],
    ) -> Result<()> {
        check_claims(claims.iter().map(String::as_str))?;
        if by == change {
            return Err(StoreError::Backend(
                "a change is not re-confirmed by itself".to_owned(),
            ));
        }
        let mut d = self.acceptances_lock();
        if !d.acceptances.iter().any(|a| a.id == *by) {
            return Err(StoreError::NotFound);
        }
        if d.results.iter().any(|r| r.acceptance == *by) {
            return Err(StoreError::StateChanged);
        }
        // The change sealed, in the same wiki, and each claim was kept by it.
        let sealed_same_wiki = d.acceptances.iter().any(|a| {
            a.id == *change
                && d.acceptances
                    .iter()
                    .any(|b| b.id == *by && b.wiki == a.wiki)
                && d.results.iter().any(|r| {
                    r.acceptance == a.id && matches!(r.outcome, AcceptanceOutcome::Sealed { .. })
                })
        });
        if !sealed_same_wiki
            || !claims.iter().all(|claim| {
                d.claims
                    .iter()
                    .any(|(a, c)| a == change && c.claim == *claim && c.how == ClaimHow::Kept)
            })
        {
            return Err(StoreError::NotFound);
        }
        if claims.iter().any(|claim| {
            d.reconfirmations
                .iter()
                .any(|(c, k, b)| c == change && k == claim && b == by)
        }) {
            return Err(StoreError::AlreadyRecorded);
        }
        d.reconfirmations.extend(
            claims
                .iter()
                .map(|claim| (change.clone(), claim.clone(), by.clone())),
        );
        Ok(())
    }

    /// The records of the sealed acceptances `select` picks, oldest acceptance first.
    pub(super) fn claim_reviews_read(
        &self,
        select: impl Fn(&AcceptanceRecord) -> bool,
    ) -> Vec<ClaimReview> {
        let d = self.acceptances_lock();
        let sealed = |id: &AcceptanceId| {
            d.results.iter().any(|r| {
                r.acceptance == *id && matches!(r.outcome, AcceptanceOutcome::Sealed { .. })
            })
        };
        d.acceptances
            .iter()
            .filter(|a| select(a) && sealed(&a.id))
            .flat_map(|a| {
                let d = &d;
                d.claims
                    .iter()
                    .filter(move |(id, _)| *id == a.id)
                    .map(move |(_, outcome)| ClaimReview {
                        acceptance: a.id.clone(),
                        proposal: a.proposal.clone(),
                        claim: outcome.claim.clone(),
                        how: outcome.how,
                        reconfirmed_by: d
                            .reconfirmations
                            .iter()
                            .find(|(c, k, b)| *c == a.id && *k == outcome.claim && sealed(b))
                            .map(|(_, _, b)| b.clone()),
                    })
            })
            .collect()
    }
}
