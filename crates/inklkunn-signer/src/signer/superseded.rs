//! Seals made by a superseded maintainer key after the rotation that replaced it (corrections handoff C11;
//! DR-10; ledger PK-34, PK-39).
//!
//! **What the owner ruled** (DR-10): a rotation keeps the old maintainer key trusted by default, because
//! removing it makes everything it ever sealed fail prikk's verification. That leaves a hole prikk confirmed it
//! cannot close: a leaked copy of the old seed can still seal anything that verifies, and a copy of the
//! repository cannot prove a block was sealed by the old key *after* the new one was adopted, because adoption
//! is local and recorded nowhere a copy can check. inklkunn can: it knows the moment. The key record keeps, for
//! every superseded key, the tip block of every view at that moment; this check reads who sealed which block
//! (`prikk verify`'s `sealed-block` lines) and reports every block a superseded key sealed that is **not** in
//! the history up to those tips.
//!
//! **What it covers.** A leaked seed used against the wiki's own repository while the service's records are
//! intact (a leaked backup, a leaked seed file). **What it does not.** A host that is taken over: the records
//! are the attacker's then, and only `--retire-old` helps. It never changes anything by itself (RFC 002 D-7,
//! INV-1): it reports.

use std::collections::HashSet;
use std::path::Path;

use inklkunn_store::WikiId;

use super::{Signer, SignerError, ks};
use crate::key_store::RecordedTip;
use crate::writes::Logged;

/// The most blocks of one view the check reads before it gives up and says so.
const MAX_HISTORY: u64 = 1 << 20;
/// The first read of a view's history; each full read is followed by one four times as large.
const FIRST_READ: u64 = 4096;

/// A maintainer key a rotation replaced, as the key record keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersededKey {
    /// The generation of the pair it belonged to.
    pub generation: u32,
    /// The maintainer key's id.
    pub maintainer_key_id: String,
    /// Unix seconds of the rotation that replaced it.
    pub superseded_at: i64,
    /// Was it also removed from the repository's trust?
    pub retired: bool,
    /// How many views had their tip recorded.
    pub views_recorded: usize,
    /// How many blocks, ref states and ref updates the retirement recorded as failing publication trust for want
    /// of this key (handoff 004, P2′); 0 for a key that was kept, and for a retirement recorded before the set was
    /// kept, which excuses nothing.
    pub findings_recorded: usize,
}

/// A block sealed by a superseded maintainer key **after** the rotation that replaced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LateSeal {
    /// The generation of the superseded pair.
    pub generation: u32,
    /// The superseded maintainer key's id.
    pub maintainer_key_id: String,
    /// The block.
    pub block_id: String,
}

impl std::fmt::Display for LateSeal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "block {} was sealed by the superseded maintainer key {} (generation {}) after the rotation that replaced it",
            self.block_id, self.maintainer_key_id, self.generation
        )
    }
}

/// One view and its log, newest first: each block and the id of the ref state after it.
pub(super) type ViewHistory = (String, Vec<Logged>);

/// What existed when a superseded key was replaced (handoff 004, Y1, Z1): from each tip the key record kept then,
/// back through that view's log, the blocks and the ref states after them. **The one definition of *known***: the
/// late-seal check ([`Signer::seals_by_superseded_keys`]) and a retirement ([`Signer::retire_key`]) both ask it, so
/// whatever the check calls a late seal no retirement records, and the other way round.
pub(super) struct Known<'a> {
    /// The blocks.
    pub(super) blocks: HashSet<&'a str>,
    /// The ref states after them.
    pub(super) ref_states: HashSet<&'a str>,
    /// The recorded tips that are not in their view's log now, as `(view, block)`: what the key record says was
    /// there and the repository does not show. Nothing of such a view is known.
    pub(super) lost: Vec<(&'a str, &'a str)>,
}

/// The blocks and ref states that existed when a superseded key was replaced. Counted from the **oldest** entry whose
/// block is the recorded tip: a block that stood twice in a log leaves the later entries unknown, never known.
pub(super) fn known_when_replaced<'a>(
    kept: &'a [RecordedTip],
    histories: &'a [ViewHistory],
) -> Known<'a> {
    let mut known = Known {
        blocks: HashSet::new(),
        ref_states: HashSet::new(),
        lost: Vec::new(),
    };
    for tip in kept {
        if tip.block_id.is_empty() {
            continue;
        }
        let found = histories
            .iter()
            .find(|(v, _)| *v == tip.view)
            .and_then(|(_, log)| {
                log.iter()
                    .rposition(|(b, _)| *b == tip.block_id)
                    .map(|at| (log, at))
            });
        let Some((log, at)) = found else {
            known.lost.push((tip.view.as_str(), tip.block_id.as_str()));
            continue;
        };
        for (block, state) in log.iter().skip(at) {
            known.blocks.insert(block.as_str());
            known.ref_states.insert(state.as_str());
        }
    }
    known
}

impl Signer {
    /// The maintainer keys earlier rotations replaced, oldest first. Ids and dates; the record is authenticated
    /// to read them.
    pub fn superseded_keys(&self, wiki: &WikiId) -> Result<Vec<SupersededKey>, SignerError> {
        let meta = self.store.status(wiki).map_err(ks)?;
        Ok(meta
            .superseded
            .into_iter()
            .map(|s| SupersededKey {
                generation: s.generation,
                maintainer_key_id: s.maintainer_key_id,
                superseded_at: s.superseded_at,
                retired: s.retired,
                views_recorded: s.views.len(),
                findings_recorded: s.excused.len(),
            })
            .collect())
    }

    /// Every block a superseded maintainer key sealed after the rotation that replaced it (see the module
    /// documentation). Read-only: it reads the key record's plain facts, the repository's views and history, and
    /// `prikk verify`'s list of who sealed what; it signs nothing and decrypts no seed. Empty for a wiki that
    /// was never rotated.
    pub fn seals_by_superseded_keys(&self, repo: &Path) -> Result<Vec<LateSeal>, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let meta = self.store.status(&wiki).map_err(ks)?;
        if meta.superseded.is_empty() {
            return Ok(Vec::new());
        }
        // Who sealed what, now.
        let signers = self.writes.sealed_block_signers(repo)?;
        // The history of every view now, newest first.
        let histories = self.view_histories(repo)?;
        let mut late = Vec::new();
        for key in &meta.superseded {
            // The blocks that existed when this key was replaced: from each recorded tip back.
            let known = known_when_replaced(&key.views, &histories);
            for (block, signed_by) in &signers {
                if *signed_by == key.maintainer_key_id && !known.blocks.contains(block.as_str()) {
                    late.push(LateSeal {
                        generation: key.generation,
                        maintainer_key_id: key.maintainer_key_id.clone(),
                        block_id: block.clone(),
                    });
                }
            }
        }
        Ok(late)
    }

    /// The log of every view now, newest first, as `(view, entries)`.
    pub(super) fn view_histories(&self, repo: &Path) -> Result<Vec<ViewHistory>, SignerError> {
        let mut histories = Vec::new();
        for view in self.writes.all_views(repo)? {
            let blocks = self.view_history(repo, &view)?;
            histories.push((view, blocks));
        }
        Ok(histories)
    }

    /// A view's whole log, newest first: one `log --limit N` at a time, four times larger while it comes
    /// back full (the same reading the reconciliation does, ledger PK-36), and a refusal at a stated bound.
    fn view_history(&self, repo: &Path, view: &str) -> Result<Vec<Logged>, SignerError> {
        let mut limit = FIRST_READ;
        loop {
            let blocks = self.writes.view_blocks(repo, view, limit)?;
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
