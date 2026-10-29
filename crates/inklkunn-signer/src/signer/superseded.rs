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
        let mut histories: Vec<(String, Vec<String>)> = Vec::new();
        for view in self.writes.all_views(repo)? {
            let blocks = self.view_history(repo, &view)?;
            histories.push((view, blocks));
        }
        let mut late = Vec::new();
        for key in &meta.superseded {
            // The blocks that existed when this key was replaced: from each recorded tip back.
            let mut known: HashSet<&str> = HashSet::new();
            for tip in &key.views {
                if tip.block_id.is_empty() {
                    continue;
                }
                let Some((_, blocks)) = histories.iter().find(|(v, _)| *v == tip.view) else {
                    continue;
                };
                if let Some(at) = blocks.iter().position(|b| *b == tip.block_id) {
                    known.extend(blocks.iter().skip(at).map(String::as_str));
                }
            }
            for (block, signed_by) in &signers {
                if *signed_by == key.maintainer_key_id && !known.contains(block.as_str()) {
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

    /// A view's whole history, newest first: one `log --limit N` at a time, four times larger while it comes
    /// back full (the same reading the reconciliation does, ledger PK-36), and a refusal at a stated bound.
    fn view_history(&self, repo: &Path, view: &str) -> Result<Vec<String>, SignerError> {
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
