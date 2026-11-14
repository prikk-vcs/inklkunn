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
use crate::knowledge::{
    AnchorState, Claim, ClaimId, Digest, Expression, Family, File, KNOWLEDGE_DIR, Standing,
    anchor_state, claim_path, family_of, read_file,
};

/// Does this claim file say it was judged again on the wording its paragraph has now? Only when it carries at least
/// one judgement (its status's wording, a source link's) and **every** one of them names the wording of one of the
/// places the claim is said in **after the acceptance** (`now`: the digests the file's own expressions have in the
/// accepted result; a curation proposal writes no page, so the result's pages are the tip's). A revision that changes
/// a note, or judges only some of what the claim holds, judges nothing again: the record of *re-confirmed* is a
/// statement the page repeats (*judged again on the wording it has now*), so it is made for what is true of every
/// judgement and for nothing less (handoff 005 corrections, second round, K17).
///
/// **A decided status that names no wording is a judgement that fails** (third round, K28): it stands on nothing the
/// reconfirm can say it judged again.
pub(super) fn judged_on_now(claim: &Claim, now: &[Digest]) -> bool {
    let Claim {
        id: _,
        kind: _,
        expressions: _,
        relations: _,
        lifecycle: _,
        community_status,
        sources,
    } = claim;
    if community_status.status != Standing::Unknown && community_status.judged_on.is_none() {
        return false;
    }
    let judgements: Vec<&Digest> = community_status
        .judged_on
        .iter()
        .chain(sources.iter().map(|l| &l.judged_on))
        .collect();
    !judgements.is_empty() && judgements.into_iter().all(|on| now.contains(on))
}

/// The change a curation proposal re-confirms and the claims of it that this acceptance judges again.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reconfirmed {
    /// The acceptance that put the change in the history.
    pub(super) change: AcceptanceId,
    /// The claims (ids) the change kept and that the curation's files judge again **on the wording their paragraph has
    /// now** ([`judged_on_now`]), and that no earlier curation did.
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
    /// change's kept claims its files judge again, **on the wording the paragraph has now** ([`judged_on_now`]), and no
    /// earlier curation has. *Now* is the wording the claim's paragraphs have **after the acceptance**: the places the
    /// written file names (K28), read in the pages of the tip (a curation proposal writes no page, so those are the
    /// pages it is accepted on), and not the places the tip's own file for the claim names.
    pub(super) fn reconfirmed(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        (tip, tip_state): (&str, &str),
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
        let written: Vec<(String, Vec<u8>)> = files
            .iter()
            .filter_map(|f| Some((f.path.to_str()?, f.content.as_ref()?)))
            .filter(|(p, _)| family_of(p) == Some(Family::Claim))
            .map(|(p, bytes)| (p.to_owned(), bytes.clone()))
            .collect();
        let kept: Vec<String> = self
            .parts
            .proposals
            .claim_reviews_of(principal, wiki, &change)?
            .into_iter()
            .filter(|r| r.how == ClaimHow::Kept && r.reconfirmed_by.is_none())
            .map(|r| r.claim)
            .collect();
        let mut claims = Vec::new();
        let mut pages: Option<Pages> = None;
        for claim in kept {
            let Some(id) = ClaimId::parse(&claim) else {
                continue;
            };
            let path = claim_path(&id);
            let Some((_, bytes)) = written.iter().find(|(p, _)| *p == path) else {
                continue;
            };
            let Ok(File::Claim(file)) = read_file(&path, bytes) else {
                continue;
            };
            // The pages are read once, and only when there is a claim to judge.
            if pages.is_none() {
                pages = Some(self.pages_of(wiki, tip, tip_state)?);
            }
            let mut now: Vec<Digest> = Vec::new();
            for e in &file.expressions {
                if let Some(digest) = self.wording_of(wiki, pages.as_ref(), e)? {
                    now.push(digest);
                }
            }
            if judged_on_now(&file, &now) {
                claims.push(claim);
            }
        }
        Ok(Some(Reconfirmed { change, claims }))
    }

    /// The pages of `tip`, **the block the acceptance prepared on** (`state`, the state it belongs to, read with it): where
    /// each page id is, at that block. Asked of the title index of exactly that state, which the read service keeps (in
    /// memory and in the derived store) and builds from one extract only when it has none, so an acceptance does not
    /// extract the whole tree under the wiki's build lock every time (K54 U-l). The pair names the block itself, never
    /// the read service's held tip (K35): a write that did not go through this process, inside the hold, must not make
    /// the reconfirm judge an older wording than the one the result has.
    fn pages_of(&self, wiki: &WikiRecord, tip: &str, state: &str) -> Result<Pages, AcceptFailure> {
        let index = self
            .parts
            .reads
            .title_index_at_base(wiki, None, state, tip)
            .map_err(|e| AcceptFailure::Unavailable(e.to_string()))?;
        Ok(Pages {
            block: tip.to_owned(),
            index,
        })
    }

    /// The wording (the digest of the paragraph, marker and all) the place `at` has in the tip's pages: `None` when the
    /// page is not there once, or has not the block once.
    fn wording_of(
        &self,
        wiki: &WikiRecord,
        pages: Option<&Pages>,
        at: &Expression,
    ) -> Result<Option<Digest>, AcceptFailure> {
        let Some(pages) = pages else {
            return Ok(None);
        };
        let mut found = pages
            .index
            .entries()
            .iter()
            .filter(|e| e.page_id.as_deref() == Some(at.page_id.as_str()));
        let (Some(entry), None) = (found.next(), found.next()) else {
            return Ok(None);
        };
        let text = self
            .parts
            .reads
            .source_at(wiki, &pages.block, &entry.path)
            .map_err(|e| AcceptFailure::Unavailable(e.to_string()))?;
        Ok(
            match text.as_deref().map(|text| anchor_state(text, &at.block)) {
                Some(AnchorState::Present(digest)) => Some(digest),
                _ => None,
            },
        )
    }
}

/// The pages of one block: the block they are read at and the entries that say where each page id is.
struct Pages {
    block: String,
    index: std::sync::Arc<crate::read::TitleIndex>,
}

#[cfg(test)]
mod tests;
