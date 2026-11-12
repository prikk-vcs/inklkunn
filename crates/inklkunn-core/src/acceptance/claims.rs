//! The review record at acceptance (RFC 005 part 1 D-1 d; handoff 005 K3 b, K11): what the change did to the claims it
//! touches, and which of its claims a reconfirm curation re-confirms.
//!
//! Both are **read before anything is recorded** (a failure to read is a plain *unavailable*, nothing written) and
//! **written after the acceptance record and before the intent** ([`ProposalAcceptor::accept`]'s step 4): a record that
//! cannot be kept ends the acceptance as *failed* with nothing sealed, so an acceptance that signs always has its
//! record, and a failed record never signs a second time. The records are keyed by the acceptance and read only from
//! one that sealed ([`inklkunn_store::ProposalStore::claim_reviews`]).

use inklkunn_prikk::Point;
use inklkunn_signer::PreparedFile;
use inklkunn_store::{AcceptanceId, ClaimHow, ClaimOutcome, ProposalId, WikiRecord};

use super::{AcceptFailure, ProposalAcceptor};
use crate::authorize::Principal;
use crate::knowledge::onpage::touches;
use crate::knowledge::touched::{How, PageChange};
use crate::knowledge::{ClaimId, Family, KNOWLEDGE_DIR, claim_path, family_of};

/// The change a curation proposal re-confirms and the claims of it that this acceptance judges again.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reconfirmed {
    /// The acceptance that put the change in the history.
    pub(super) change: AcceptanceId,
    /// The claims (ids) the change kept and that the curation's files judge again, and that no earlier curation did.
    pub(super) claims: Vec<String>,
}

impl ProposalAcceptor {
    /// What the change does to the claims it touches (a paragraph a claim is said in is reworded or goes): *kept* for
    /// the first, *dropped* for the second. `files` are the files as they will be signed; the pages are read against the
    /// tip they will be written on.
    pub(super) fn claim_outcomes(
        &self,
        wiki: &WikiRecord,
        view: &str,
        tip: &str,
        files: &[PreparedFile],
    ) -> Result<Vec<ClaimOutcome>, AcceptFailure> {
        let unavailable = |e: crate::read::ReadError| AcceptFailure::Unavailable(e.to_string());
        // A tip that holds no claim has none to touch: one `tree` call answers, and nothing is extracted or read
        // (the same free answer the knowledge check gives a wiki with no knowledge layer).
        let repo = self.parts.layout.repo_dir(&wiki.id);
        let listing = self
            .parts
            .storage
            .tree(&repo, &Point::Block(tip.to_owned()), Some(KNOWLEDGE_DIR))
            .map_err(|e| AcceptFailure::Unavailable(e.to_string()))?;
        if !listing
            .entries
            .iter()
            .any(|e| e.kind == "file" && family_of(&e.path) == Some(Family::Claim))
        {
            return Ok(Vec::new());
        }
        let mut pages: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for file in files {
            let Some(path) = file.path.to_str() else {
                continue;
            };
            if family_of(path).is_some() || !path.starts_with("pages/") {
                continue;
            }
            let new = match &file.content {
                None => None,
                Some(bytes) => match std::str::from_utf8(bytes) {
                    Ok(text) => Some(text.to_owned()),
                    Err(_) => continue,
                },
            };
            let old = self
                .parts
                .reads
                .source_at(wiki, tip, path)
                .map_err(unavailable)?;
            pages.push((path.to_owned(), old, new));
        }
        if pages.is_empty() {
            return Ok(Vec::new());
        }
        let changes: Vec<PageChange<'_>> = pages
            .iter()
            .map(|(path, old, new)| PageChange {
                path,
                old: old.as_deref(),
                new: new.as_deref(),
            })
            .collect();
        let touched =
            touches(&self.parts.reads, wiki, Some(view), tip, &changes).map_err(unavailable)?;
        Ok(touched
            .into_iter()
            .map(|t| ClaimOutcome {
                claim: t.claim.to_string(),
                how: match t.how {
                    How::Reworded => ClaimHow::Kept,
                    How::Removed => ClaimHow::Dropped,
                },
            })
            .collect())
    }

    /// Write the review record of the acceptance `acceptance`: what its change did to each claim it touched, and the
    /// claims it re-confirms.
    pub(super) fn record_reviews(
        &self,
        acceptance: &AcceptanceId,
        outcomes: &[ClaimOutcome],
        reconfirmed: Option<&Reconfirmed>,
    ) -> Result<(), inklkunn_store::StoreError> {
        self.parts
            .proposals
            .record_claim_reviews(acceptance, outcomes)?;
        if let Some(r) = reconfirmed.filter(|r| !r.claims.is_empty()) {
            self.parts
                .proposals
                .record_reconfirmations(acceptance, &r.change, &r.claims)?;
        }
        Ok(())
    }

    /// When the proposal is the curation proposal of a reconfirm step: the change it re-confirms and which of that
    /// change's kept claims its files judge again and no earlier curation has.
    pub(super) fn reconfirmed(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalId,
        curation: bool,
        files: &[PreparedFile],
    ) -> Result<Option<Reconfirmed>, AcceptFailure> {
        if !curation {
            return Ok(None);
        }
        let Some(change) = self
            .parts
            .proposals
            .reconfirm_of(principal, wiki, proposal)?
        else {
            return Ok(None);
        };
        let written: Vec<String> = files
            .iter()
            .filter_map(|f| f.path.to_str())
            .filter(|p| family_of(p) == Some(Family::Claim))
            .map(str::to_owned)
            .collect();
        let claims = self
            .parts
            .proposals
            .claim_reviews_of(principal, wiki, &change)?
            .into_iter()
            .filter(|r| r.how == ClaimHow::Kept && r.reconfirmed_by.is_none())
            .filter(|r| {
                ClaimId::parse(&r.claim).is_some_and(|id| written.contains(&claim_path(&id)))
            })
            .map(|r| r.claim)
            .collect();
        Ok(Some(Reconfirmed { change, claims }))
    }
}
