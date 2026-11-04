//! A wiki's keys over time (RFC 002 D-8): **create** them for a repository that has none, **rotate** them,
//! and (in `Signer::key_status`) read their ids and dates. These are administrators' acts; the caller has
//! authorized and audited them. No operation here returns or writes a seed anywhere but the sealed record.
//!
//! **Rotation, and what prikk lets it mean.** prikk binds a key id to one public key forever and has no key
//! rotation (security-setup: *"no key rotation"*; a rotation attempt under the same id is refused as
//! indistinguishable from impersonation), so a rotation is a **new pair of key ids**: new keys are generated,
//! the new maintainer key is adopted in the repository's trust store, and only then does the sealed record
//! change, so a failure before the last step leaves the wiki exactly as it was (an adopted key nobody holds
//! the seed of is harmless). What happens to the **old** maintainer key is the operator's choice, and prikk
//! 0.54.1 makes it a real one (ledger PK-34):
//!
//! - [`OldKey::Keep`] leaves it trusted. Everything it sealed keeps verifying. The service no longer holds its
//!   seed, but a copy somewhere else could still seal changes that verify. Right for a routine rotation.
//! - [`OldKey::Retire`] is the rotation followed by the **one** retirement of the key it just replaced, the same
//!   (and the same code) as `inklkunn wiki keys retire` makes of any superseded key (handoff 004, X1). It removes the key from the trust store. It can seal nothing more — **and everything it sealed
//!   stops verifying**: `prikk verify` fails with publication-trust issues for each block, ref state and ref
//!   update it signed, and the wiki shows *Verification failed* until prikk can keep a retired key's history
//!   verifiable. Right for a compromise, where the alternative is forged seals that verify.

use std::path::Path;

use inklkunn_store::WikiId;

use super::retirement::{not_begun_text, recovery_first, unfinished_refusal};
use super::{KeyDir, KeyIds, Meta, Signer, SignerError, ks, read_seeds};
use crate::KeyAuthority;
use crate::key_store::{Excused, Superseded};

/// What happens to the old maintainer key when keys are rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldKey {
    /// It stays trusted in the repository: history it sealed keeps verifying.
    Keep,
    /// It is removed from the repository's trust store: it can seal nothing more, and everything it sealed
    /// stops verifying (prikk 0.54.1, ledger PK-34).
    Retire,
}

/// What became of the old maintainer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OldKeyOutcome {
    /// It was left trusted, as asked.
    Kept,
    /// It was removed from the repository's trust, as asked, and the key record holds the findings the removal
    /// caused (handoff 004, P2′): a later failure made of exactly those is excused. Only when every finding of the
    /// removal was traceable to the key's own history (Z1); otherwise [`OldKeyOutcome::RetiredUnrecorded`].
    Retired,
    /// The key was removed from the repository's trust, as asked, **but its findings are not recorded**: `verify`
    /// could not be run after the removal, the two looks disagreed, the count of what prikk checked changed, a finding
    /// could not be traced to what the key sealed for this service (a late block, a tag, a ref state nothing in its
    /// history explains, a block another key lost: Z1, said with [`UNTRACEABLE_SENTENCE`]), the record was too large or
    /// could not be written. The key record
    /// keeps the state the retirement began from (`retiring`), nothing is excused, and **nothing new is sealed**
    /// until `inklkunn wiki keys finish-retirement <wiki>` ([`Signer::finish_retirement`]) completes the record from
    /// that state, or `inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>`
    /// ([`Signer::abandon_retirement`]) gives the retirement up (handoff 004, Q1, W1, W3). Nothing is trusted that
    /// should not be. The text is why, and names the way out that can work.
    ///
    /// [`Signer::finish_retirement`]: crate::Signer::finish_retirement
    /// [`Signer::abandon_retirement`]: crate::Signer::abandon_retirement
    /// [`UNTRACEABLE_SENTENCE`]: crate::UNTRACEABLE_SENTENCE
    RetiredUnrecorded(String),
    /// Removal was asked for and **failed**: the keys *were* rotated and the old maintainer key is **still
    /// trusted**, and the retirement stays begun (nothing new is sealed). The text is prikk's, for operators;
    /// `inklkunn wiki keys finish-retirement <wiki>` does the removal itself and the rest (W2), and
    /// `inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>` leaves the key trusted.
    RetireFailed(String),
    /// Removal was asked for and **no retirement was begun**: the keys *were* rotated, the old maintainer key is
    /// **still trusted**, and nothing is paused (the key record could not take the retirement's state, or the
    /// repository's trust could not be read). The text is why (without a *nothing was changed*, which is not true: the
    /// rotation happened), says the old key is still trusted and names `inklkunn wiki keys retire <wiki> --key <id>
    /// --confirm <slug> --accept-unverifiable-history` (handoff 004, X1, Y4), which retires the key later.
    RetireNotBegun(String),
}

/// What a rotation did. A rotation that happened is always a `Rotation`, whatever became of the old key: the
/// state of the key store is the first thing a caller must record, and an `Err` would hide it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotation {
    /// The keys now in force (generation, ids and dates).
    pub status: super::KeyStatus,
    /// The maintainer key id that was in force before.
    pub previous_maintainer_key_id: String,
    /// What became of it.
    pub old_key: OldKeyOutcome,
}

/// The id of a rotated key: `inklkunn-<wiki id>-<role>-<generation>-<first 8 hex of its public key>`. The
/// public-key part makes a retry after a half-failed rotation get fresh ids (prikk never lets an id be
/// adopted again with another key), and the generation lets a person reading a log tell them apart.
fn rotated_id(wiki: &WikiId, role: &str, generation: u32, public_key_hex: &str) -> String {
    let short: String = public_key_hex.chars().take(8).collect();
    format!("inklkunn-{wiki}-{role}-{generation}-{short}")
}

impl Signer {
    /// Make a wiki's keys for a repository that already exists (is initialised) and has none: two keys
    /// generated by prikk, the maintainer key adopted, the sealed record stored last. Refused when the wiki
    /// already has keys. Wiki creation does this too, after it makes the repository.
    pub fn create_keys(
        &self,
        _authority: &KeyAuthority,
        repo: &Path,
    ) -> Result<KeyIds, SignerError> {
        self.provision(repo, false)
    }

    /// Rotate a wiki's keys: new author and maintainer keys, the new maintainer adopted, the record replaced
    /// (generation + 1), and — only when asked — the old maintainer key removed from trust. See the module
    /// documentation for what each choice means. Refused when the wiki has no keys.
    ///
    /// A change being accepted at the same moment is excluded by the wiki's write lock: whichever of the two
    /// comes second is told the wiki is busy ([`SignerError::Busy`]), naming the process that holds it.
    pub fn rotate_keys(
        &self,
        _authority: &KeyAuthority,
        repo: &Path,
        old_key: OldKey,
    ) -> Result<Rotation, SignerError> {
        let wiki = self.wiki_of(repo)?;
        // Excludes an acceptance on this wiki in this process and in every other (the per-wiki write lock).
        let _held = self.write_lock(&wiki)?;
        let before = self.store.status(&wiki).map_err(ks)?;
        if let Some(open) = &before.retiring {
            return Err(unfinished_refusal(&wiki, &open.maintainer_key_id));
        }
        // A rotation records the newest block of every view (C11). A publication a killed `seal` left pending is
        // finished by recovery afterwards, and the service's own block would then look like one made after the key was
        // replaced (Z5): recovery first, whichever way the old key is to go.
        if self
            .writes
            .doctor(repo)?
            .diagnostics
            .iter()
            .any(|d| d.is_cleanup_pending())
        {
            return Err(SignerError::NeedsRecovery(format!(
                "a publication that a killed seal left is pending, and a rotation records where the old key stopped; run `inklkunn wiki recover {wiki}` first, then rotate again"
            )));
        }
        let generation = before.generation.saturating_add(1);
        // A retirement starts from a repository that verifies (handoff 004, P2′): `verify` runs before anything is
        // changed, under the write lock just taken, so nothing happens between it and the removal from trust, and the
        // old key stays trusted if it does not pass. Its text names the signer of every sealed block.
        let verified_before = match old_key {
            OldKey::Keep => None,
            OldKey::Retire => Some(
                self.verify_before_retirement(repo, &wiki, &before.maintainer_key_id)
                    .map_err(|e| {
                        recovery_first(&wiki, e, "then rotate again with `--retire-old`")
                    })?,
            ),
        };
        let scratch = self.seed_base.scratch().map_err(ks)?;
        let result = (|| -> Result<Rotation, SignerError> {
            // 1. New keys, in a scratch directory on the tmpfs. The ids depend on the public keys, so the
            //    seeds are generated under provisional ids and prikk is told the final ones afterwards.
            let provisional = KeyDir::with_ids(
                scratch.path(),
                "provisional-author",
                "provisional-maintainer",
            )?;
            self.writes.generate_key(&provisional, "author")?;
            self.writes.generate_key(&provisional, "maintainer")?;
            let author_public = self.writes.public_key(&provisional, "author")?;
            let maintainer_public = self.writes.public_key(&provisional, "maintainer")?;
            let author_id = rotated_id(&wiki, "author", generation, &author_public);
            let maintainer_id = rotated_id(&wiki, "maintainer", generation, &maintainer_public);
            let keys = KeyDir::with_ids(scratch.path(), &author_id, &maintainer_id)?;

            // 2. Adopt the new maintainer key. Until the record changes nothing signs with it.
            self.writes
                .trust_maintainer(repo, &maintainer_id, &maintainer_public)?;

            // 3. The commit point: from here the service signs with the new keys. The record also keeps where the
            //    old maintainer key stopped: the tip block of every view now (corrections handoff C11), so that a
            //    later seal by that key can be told from the history it really sealed.
            let tips = self.current_tips(repo)?;
            let now = inklkunn_store::record::now_secs();
            let mut superseded = before.superseded.clone();
            superseded.push(Superseded {
                generation: before.generation,
                maintainer_key_id: before.maintainer_key_id.clone(),
                superseded_at: now,
                retired: false,
                views: tips,
                excused: Excused::default(),
            });
            let seeds = read_seeds(&keys)?;
            let meta = Meta {
                wiki: wiki.clone(),
                generation,
                author_key_id: author_id.clone(),
                maintainer_key_id: maintainer_id.clone(),
                created_at: before.created_at,
                rotated_at: Some(now),
                superseded,
                retiring: None,
            };
            self.store.replace(&meta, &seeds).map_err(ks)?;

            // 4. Only now, and only on request, the old maintainer key is retired: the same retirement, by the same code,
            //    as `inklkunn wiki keys retire` makes of any superseded key (X1). The rotation has happened whatever
            //    comes of it, and the result says so, never hidden and never an error that would make the caller think
            //    nothing changed. (The `verify` that must pass was taken above, before anything was changed.)
            let old_key_outcome = match verified_before {
                None => OldKeyOutcome::Kept,
                Some(look) => {
                    match self.retire_superseded(repo, &wiki, &before.maintainer_key_id, look) {
                        Ok(outcome) => outcome,
                        Err(e) => OldKeyOutcome::RetireNotBegun(not_begun_text(
                            &wiki,
                            &before.maintainer_key_id,
                            &e,
                        )),
                    }
                }
            };
            Ok(Rotation {
                status: super::KeyStatus {
                    wiki: wiki.clone(),
                    generation,
                    author_key_id: author_id,
                    maintainer_key_id: maintainer_id,
                    created_at: before.created_at,
                    rotated_at: meta.rotated_at,
                },
                previous_maintainer_key_id: before.maintainer_key_id.clone(),
                old_key: old_key_outcome,
            })
        })();
        match (result, scratch.finish()) {
            (Ok(r), Ok(())) => Ok(r),
            (Ok(_), Err(e)) => Err(ks(e)),
            (Err(e), _) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests;
