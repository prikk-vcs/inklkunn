//! A retired key never freezes the wiki (handoff 004, P2; DR-10; ledger PK-34).
//!
//! **What the owner ruled** (DR-10): retiring a maintainer key costs the wiki its *Verified* label, because prikk
//! judges the whole history against today's trust set and the blocks the retired key sealed no longer verify. The
//! label is the known cost. A frozen wiki is not: before this check every acceptance after a retirement answered
//! *sealed, but not confirmed* and recovery's `verify` step always failed, so a retirement stopped the wiki's
//! writes for good.
//!
//! **What is excused, and nothing else.** `prikk verify` after a retirement fails *exactly and only* by
//! publication trust, for blocks the retired key sealed. In prikk's output ([`parse_publication_trust`]) that is:
//!
//! 1. the report is complete (no partial stage) and its failed conditions are the one condition
//!    `publication-trust`, so no other check failed;
//! 2. every `publication-trust` line of the text has the exact shape of prikk 0.55.0
//!    (`... [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: <block|ref-state|ref-update> <id> has no trusted MAINTAINER
//!    signature`) and their number is the `publication trust issues` count prikk prints;
//! 3. every **block** it names is in the history up to a tip the key record kept for a **retired** key at the
//!    moment of the rotation that replaced it (handoff 002, C11): a block newer than that point cannot have been
//!    sealed by the retired key while it was trusted, so it is not excused (an untrusted block beyond the
//!    point, a third key's seal, is still a stop);
//! 4. every **ref state** it names that is the state after a block is the state after an excused block; a ref
//!    state that no block's state matches (the publication of a branch) is excused up to one per view, and a ref
//!    update (prikk's reports give its id no link to a block) up to the number of ref states named.
//!
//! A retirement that left no recorded point (a record from before the point was kept) excuses nothing.
//! The badge still says *Verification failed*, as DR-10 ruled.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use inklkunn_prikk::storage::Verification;
use inklkunn_store::WikiId;

use super::{Signer, SignerError, ks};
use crate::writes::{PublicationTrust, parse_publication_trust};

/// The most blocks of one view read to find the retired keys' points.
const MAX_HISTORY: u64 = 1 << 20;
/// The first read of a view's history; each full read is followed by one four times as large.
const FIRST_READ: u64 = 4096;

/// The condition id prikk gives a failed publication-trust check.
const CONDITION: &str = "publication-trust";

/// Why a failed verification is **not** explained by a retirement, or how much of it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Judgement {
    /// Exactly and only the retired keys' publication trust: this many blocks, ref states and ref updates.
    Explained { issues: usize },
    /// Something else failed, or something the retirement does not account for (the reason is for operators).
    Unexplained(&'static str),
}

/// The shape of the failure alone, before any history is read: the report is complete, its one failed condition
/// is publication trust, and the lines prikk printed are the known shape and as many as it counts. Returns how
/// many findings there are.
pub(crate) fn only_publication_trust(
    report: &Verification,
    trust: &PublicationTrust,
) -> Result<usize, &'static str> {
    if report.verdict.ok {
        return Err("the repository verifies; there is nothing to explain");
    }
    if report.is_partial() {
        return Err("the verification did not run to the end");
    }
    let conditions = &report.verdict.failed_conditions;
    let only_trust = conditions.len() == 1
        && conditions
            .first()
            .and_then(|c| c.get("id"))
            .and_then(serde_json::Value::as_str)
            == Some(CONDITION);
    if !only_trust {
        return Err("a condition besides publication trust failed");
    }
    let named = trust.blocks.len() + trust.ref_states.len() + trust.ref_updates.len();
    if named == 0 {
        return Err("no publication-trust finding is named");
    }
    if trust.unrecognised != 0 {
        return Err("a publication-trust line is not of the known shape");
    }
    if trust.issues != Some(u64::try_from(named).unwrap_or(u64::MAX)) {
        return Err("the findings named are not the number prikk counts");
    }
    Ok(named)
}

/// Judge a verification against the publication-trust findings and the retired keys' history.
///
/// `known` is the set of blocks that existed when a retired key was replaced; `states` maps each ref state that is
/// the state after a block of some view to that block; `views` is how many views the wiki has.
pub(crate) fn judge(
    report: &Verification,
    trust: &PublicationTrust,
    known: &HashSet<String>,
    states: &HashMap<String, String>,
    views: usize,
) -> Judgement {
    let named = match only_publication_trust(report, trust) {
        Ok(n) => n,
        Err(why) => return Judgement::Unexplained(why),
    };
    if trust.blocks.iter().any(|b| !known.contains(b)) {
        return Judgement::Unexplained(
            "a block has no trusted signature and was not sealed before a retired key was replaced",
        );
    }
    // A ref state after a block must be the state after an excused block; the rest (a branch's publication) are
    // at most one per view.
    let mut unmatched = 0_usize;
    for state in &trust.ref_states {
        match states.get(state) {
            Some(block) if known.contains(block) => {}
            Some(_) => {
                return Judgement::Unexplained(
                    "a ref state after a block newer than a retired key's point has no trusted signature",
                );
            }
            None => unmatched += 1,
        }
    }
    if unmatched > views {
        return Judgement::Unexplained(
            "more ref states that follow no block have no trusted signature than the wiki has views",
        );
    }
    if trust.ref_updates.len() > trust.ref_states.len() {
        return Judgement::Unexplained(
            "more ref updates than ref states have no trusted signature",
        );
    }
    Judgement::Explained { issues: named }
}

impl Signer {
    /// Is a failed `verify` of this wiki's repository explained exactly and only by a retired key (see the module
    /// documentation)? `Ok(Some(n))` names how many findings are excused; `Ok(None)` is anything else, including
    /// a wiki with no retired key. Read-only, no keys.
    pub(crate) fn retirement_explains(
        &self,
        repo: &Path,
        wiki: &WikiId,
        report: &Verification,
    ) -> Result<Option<usize>, SignerError> {
        if report.verdict.ok {
            return Ok(None);
        }
        let meta = self.store.status(wiki).map_err(ks)?;
        let retired: Vec<_> = meta.superseded.iter().filter(|s| s.retired).collect();
        if retired.is_empty() {
            return Ok(None);
        }
        let trust = parse_publication_trust(&self.writes.verify_text(repo)?);
        // Cheap refusals first: nothing is read from the history unless the findings could be excused at all.
        if only_publication_trust(report, &trust).is_err() {
            return Ok(None);
        }
        let mut histories: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for view in self.writes.all_views(repo)? {
            let blocks = self.view_history_states(repo, &view)?;
            histories.push((view, blocks));
        }
        let mut known: HashSet<String> = HashSet::new();
        for key in &retired {
            for tip in &key.views {
                if tip.block_id.is_empty() {
                    continue;
                }
                let Some((_, blocks)) = histories.iter().find(|(v, _)| *v == tip.view) else {
                    continue;
                };
                if let Some(at) = blocks.iter().position(|(b, _)| *b == tip.block_id) {
                    known.extend(blocks.iter().skip(at).map(|(b, _)| b.clone()));
                }
            }
        }
        let mut states: HashMap<String, String> = HashMap::new();
        for (_, blocks) in &histories {
            for (block, state) in blocks {
                states.insert(state.clone(), block.clone());
            }
        }
        Ok(
            match judge(report, &trust, &known, &states, histories.len()) {
                Judgement::Explained { issues } => Some(issues),
                Judgement::Unexplained(_) => None,
            },
        )
    }

    /// A view's whole history as `(block, ref state)`, newest first (see [`MAX_HISTORY`]).
    fn view_history_states(
        &self,
        repo: &Path,
        view: &str,
    ) -> Result<Vec<(String, String)>, SignerError> {
        let mut limit = FIRST_READ;
        loop {
            let blocks = self.writes.view_block_states(repo, view, limit)?;
            if (blocks.len() as u64) < limit {
                return Ok(blocks);
            }
            if limit >= MAX_HISTORY {
                return Err(SignerError::Unavailable(format!(
                    "the history of {view} is longer than {MAX_HISTORY} blocks; it was not read whole"
                )));
            }
            limit = limit.saturating_mul(4).min(MAX_HISTORY);
        }
    }
}

#[cfg(test)]
mod tests;
