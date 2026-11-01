//! A retired key never freezes the wiki, and never hides a tampering (handoff 004, P2 and P2′; DR-10; ledger PK-34).
//!
//! **What the owner ruled** (DR-10): retiring a maintainer key costs the wiki its *Verified* label, because prikk
//! judges the whole history against today's trust set and the blocks the retired key sealed no longer verify. The
//! label is the known cost. A frozen wiki is not: before this check every acceptance after a retirement answered
//! *sealed, but not confirmed* and recovery's `verify` step always failed.
//!
//! **What is excused is the retirement's own record, and nothing wider.** A retirement ([`Signer::rotate_keys`] with
//! `OldKey::Retire`) works in this order, under the wiki's write lock:
//!
//! 1. `verify` **before** anything changes. It must pass (or fail only by what an earlier retirement recorded); if
//!    it does not, the retirement is refused and the old key stays trusted ([`SignerError::NeedsRecovery`]). The
//!    text of that run lists the signer of every sealed block (`sealed-block <id>: <key id>`, prikk 0.55.0).
//! 2. The key is removed from the trust store.
//! 3. `verify` **after**. Its report names every block, ref state and ref update that now has no trusted
//!    signature. Those ids are recorded in the authenticated key record ([`Excused`]); a block is recorded only
//!    when step 1 listed the retired key as its signer.
//!
//! A later failing `verify` is excused if and only if its report has **no failed condition but
//! `publication-trust`** and **every block, ref state and ref update it names is in a retirement's recorded set**
//! ([`judge`]). Nothing is excused by count or by position: a rollback of `heads/main` or a branch published from an
//! old block after the retirement names a ref state or ref update the record does not hold, so it fails; so does a
//! block sealed by any key the retirement did not remove.
//!
//! **What the record can prove, and what it cannot.** It proves that the named ids were there, signed by a trusted
//! key, when `verify` passed immediately before the key left the trust store, and that they lost their trusted
//! signature at that moment. It proves the signer of a **block** (prikk lists it) and nothing about the signer of a
//! **ref state** or **ref update**: prikk 0.55.0 names no signer for those, so they are in the set because the
//! retirement made them fail. It cannot show that the retired key's blocks were honest before the retirement (that
//! is what a compromise puts in doubt), only that they are the ones the retirement removed. A retirement recorded in
//! the key-record format before this rule (no set) excuses nothing.
//!
//! The badge still says *Verification failed*, as DR-10 ruled.

use std::collections::HashSet;

use inklkunn_prikk::storage::Verification;
use inklkunn_store::WikiId;

use super::{Signer, SignerError, ks};
use crate::key_store::{Excused, MAX_EXCUSED_SUBJECTS};
use crate::writes::{PublicationTrust, VerifyRun, parse_publication_trust};

/// The condition id prikk gives a failed publication-trust check.
const CONDITION: &str = "publication-trust";

/// A failed `verify` that retirements account for entirely: what it names, and whose retirements those are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetirementExcuse {
    /// For the recovery report and the verification page: how many findings, and for which key(s).
    pub note: String,
    /// The retired maintainer key ids whose recorded findings the failure is made of.
    pub keys: Vec<String>,
    /// The blocks the failure names (all in a retirement's recorded set).
    pub blocks: Vec<String>,
    /// The ref states it names.
    pub ref_states: Vec<String>,
    /// The ref updates it names.
    pub ref_updates: Vec<String>,
}

impl RetirementExcuse {
    /// Does the failure name this block, ref state or ref update? (`kind` is prikk's word: `block`, `ref-state`,
    /// `ref-update`.)
    pub fn names(&self, kind: &str, id: &str) -> bool {
        let list = match kind {
            "block" => &self.blocks,
            "ref-state" => &self.ref_states,
            "ref-update" => &self.ref_updates,
            _ => return false,
        };
        list.iter().any(|i| i == id)
    }
}

/// Why a failed verification is **not** explained by the recorded retirements, or what it is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Judgement {
    /// Exactly and only recorded findings: these (the ids of what the failure names).
    Explained {
        /// What it names.
        named: Excused,
        /// The retired keys whose records hold some of it.
        keys: Vec<String>,
    },
    /// Something else failed, or something no retirement recorded (the reason is for operators).
    Unexplained(&'static str),
}

/// The retired keys' recorded sets, one `(key id, set)` for each retired key.
pub(crate) type Recorded<'a> = [(&'a str, &'a Excused)];

/// The shape of the failure alone, before any history is read: the report is complete, its one failed condition
/// is publication trust, and the lines prikk printed are the known shape, exactly one count line, and as many
/// findings as it counts. Returns how many findings there are.
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
    if trust.issues_lines != 1 {
        return Err("the text does not have exactly one publication-trust count line");
    }
    if trust.issues != Some(u64::try_from(named).unwrap_or(u64::MAX)) {
        return Err("the findings named are not the number prikk counts");
    }
    Ok(named)
}

/// Judge one look at `verify` against what the retirements recorded (see the module documentation).
pub(crate) fn judge(
    report: &Verification,
    trust: &PublicationTrust,
    recorded: &Recorded,
) -> Judgement {
    if let Err(why) = only_publication_trust(report, trust) {
        return Judgement::Unexplained(why);
    }
    let set = |pick: fn(&Excused) -> &Vec<String>| -> HashSet<&str> {
        recorded
            .iter()
            .flat_map(|(_, e)| pick(e).iter().map(String::as_str))
            .collect()
    };
    let (blocks, states, updates) = (
        set(|e| &e.blocks),
        set(|e| &e.ref_states),
        set(|e| &e.ref_updates),
    );
    if trust.blocks.iter().any(|b| !blocks.contains(b.as_str())) {
        return Judgement::Unexplained(
            "a block has no trusted signature and no retirement recorded it",
        );
    }
    if trust
        .ref_states
        .iter()
        .any(|s| !states.contains(s.as_str()))
    {
        return Judgement::Unexplained(
            "a ref state has no trusted signature and no retirement recorded it",
        );
    }
    if trust
        .ref_updates
        .iter()
        .any(|u| !updates.contains(u.as_str()))
    {
        return Judgement::Unexplained(
            "a ref update has no trusted signature and no retirement recorded it",
        );
    }
    let holds = |e: &Excused| {
        trust.blocks.iter().any(|b| e.blocks.contains(b))
            || trust.ref_states.iter().any(|s| e.ref_states.contains(s))
            || trust.ref_updates.iter().any(|u| e.ref_updates.contains(u))
    };
    Judgement::Explained {
        named: Excused {
            blocks: trust.blocks.clone(),
            ref_states: trust.ref_states.clone(),
            ref_updates: trust.ref_updates.clone(),
        },
        keys: recorded
            .iter()
            .filter(|(_, e)| holds(e))
            .map(|(k, _)| (*k).to_owned())
            .collect(),
    }
}

/// What a retirement records from the `verify` just after the key left the trust store, given the signer of every
/// block before (`signers`: `(block id, key id)` from the text of the verify before), the retired key's id, and
/// what earlier retirements recorded already (those are not recorded again).
///
/// Returns the new findings and how many named blocks were **left out** because the verify before did not list the
/// retired key as their signer.
pub(crate) fn record_of(
    trust: &PublicationTrust,
    signers: &[(String, String)],
    retired_key: &str,
    earlier: &Recorded,
) -> (Excused, usize) {
    let earlier_has = |pick: fn(&Excused) -> &Vec<String>, id: &String| {
        earlier.iter().any(|(_, e)| pick(e).contains(id))
    };
    let signed_by_retired: HashSet<&str> = signers
        .iter()
        .filter(|(_, k)| k == retired_key)
        .map(|(b, _)| b.as_str())
        .collect();
    let mut left_out = 0;
    let mut out = Excused::default();
    for block in &trust.blocks {
        if earlier_has(|e| &e.blocks, block) {
            continue;
        }
        if signed_by_retired.contains(block.as_str()) {
            out.blocks.push(block.clone());
        } else {
            left_out += 1;
        }
    }
    out.ref_states = trust
        .ref_states
        .iter()
        .filter(|s| !earlier_has(|e| &e.ref_states, s))
        .cloned()
        .collect();
    out.ref_updates = trust
        .ref_updates
        .iter()
        .filter(|u| !earlier_has(|e| &e.ref_updates, u))
        .cloned()
        .collect();
    (out, left_out)
}

/// The most findings a retirement may record, read before the key leaves the trust store from the count prikk
/// prints (`checked publication trust records: N`): a wiki past it is refused and nothing changes.
pub(crate) fn too_many_to_record(before: &VerifyRun) -> Option<u64> {
    let n = crate::writes::checked_publication_trust_records(&before.text)?;
    (n > MAX_EXCUSED_SUBJECTS as u64).then_some(n)
}

impl Signer {
    /// The retired keys' recorded sets, from the authenticated key record.
    fn retired_records(&self, wiki: &WikiId) -> Result<Vec<(String, Excused)>, SignerError> {
        let meta = self.store.status(wiki).map_err(ks)?;
        Ok(meta
            .superseded
            .into_iter()
            .filter(|s| s.retired)
            .map(|s| (s.maintainer_key_id, s.excused))
            .collect())
    }

    /// Is a failed `verify` of this wiki's repository made, entirely, of what retirements recorded (see the module
    /// documentation)? One look at `verify` (verdict and text together, P14) is taken for the answer; `Ok(None)` is
    /// anything else, including a repository that verifies and a wiki with no retirement that recorded anything.
    /// Read-only, no keys.
    pub(crate) fn retirement_explains(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
    ) -> Result<Option<RetirementExcuse>, SignerError> {
        let retired: Vec<_> = self
            .retired_records(wiki)?
            .into_iter()
            .filter(|(_, e)| !e.is_empty())
            .collect();
        if retired.is_empty() {
            return Ok(None);
        }
        let run = self.writes.verify_run(repo)?;
        if run.report.verdict.ok {
            return Ok(None);
        }
        let trust = parse_publication_trust(&run.text);
        let recorded: Vec<(&str, &Excused)> =
            retired.iter().map(|(k, e)| (k.as_str(), e)).collect();
        Ok(match judge(&run.report, &trust, &recorded) {
            Judgement::Explained { named, keys } => {
                let n = named.len();
                Some(RetirementExcuse {
                    note: format!(
                        "only publication trust fails: {n} finding(s), all of them recorded by the retirement of key {}; the badge still says Verification failed (DR-10)",
                        keys.join(", ")
                    ),
                    keys,
                    blocks: named.blocks,
                    ref_states: named.ref_states,
                    ref_updates: named.ref_updates,
                })
            }
            Judgement::Unexplained(_) => None,
        })
    }

    /// Step 1 of a retirement: `verify` must pass (or fail only by what earlier retirements recorded) **before** the
    /// key leaves the trust store; if it does not, nothing is done and the wiki is told it needs recovery first. Gives
    /// back the signer of every sealed block, `(block id, key id)`, from the same look.
    pub(super) fn verify_before_retirement(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
    ) -> Result<Vec<(String, String)>, SignerError> {
        let run = self.writes.verify_run(repo).map_err(|e| match e {
            SignerError::Unavailable(_) | SignerError::Damaged(_) => SignerError::NeedsRecovery(
                format!("`prikk verify` could not be run to the end ({e}); a key is retired only from a repository known to verify"),
            ),
            other => other,
        })?;
        if !run.report.verdict.ok {
            let trust = parse_publication_trust(&run.text);
            let retired = self.retired_records(wiki)?;
            let recorded: Vec<(&str, &Excused)> =
                retired.iter().map(|(k, e)| (k.as_str(), e)).collect();
            if let Judgement::Unexplained(why) = judge(&run.report, &trust, &recorded) {
                return Err(SignerError::NeedsRecovery(format!(
                    "`prikk verify` fails ({why}), so the key was not retired and is still trusted; run `inklkunn recover` for the wiki and retire the key afterwards"
                )));
            }
        }
        if let Some(n) = too_many_to_record(&run) {
            return Err(SignerError::Refused(format!(
                "the wiki has {n} signed records and a retirement records at most {MAX_EXCUSED_SUBJECTS}; the key was not retired and is still trusted"
            )));
        }
        Ok(crate::writes::parse_sealed_blocks(&run.text))
    }

    /// Step 3 of a retirement: `verify` just after the key left the trust store, and the findings to record from it
    /// (see [`record_of`]). `Err` carries the text for [`OldKeyOutcome::RetiredUnrecorded`]: the key *is* removed
    /// from trust, and whatever could not be recorded is not excused, which the text says.
    ///
    /// [`OldKeyOutcome::RetiredUnrecorded`]: super::lifecycle::OldKeyOutcome::RetiredUnrecorded
    pub(super) fn record_after_retirement(
        &self,
        repo: &std::path::Path,
        retired_key: &str,
        signers: &[(String, String)],
        earlier: &[(String, Excused)],
    ) -> (Excused, Option<String>) {
        let nothing = |why: String| {
            (
                Excused::default(),
                Some(format!(
                    "{why}; the key record holds no findings for this retirement, so nothing is excused: every later `verify` failure stops the wiki (a change is sealed but not confirmed, and recovery's `verify` step keeps the flag)"
                )),
            )
        };
        let run = match self.writes.verify_run(repo) {
            Ok(run) => run,
            Err(e) => {
                return nothing(format!(
                    "`prikk verify` could not be run after the key was removed from trust ({e})"
                ));
            }
        };
        if run.report.verdict.ok {
            return (Excused::default(), None);
        }
        let trust = parse_publication_trust(&run.text);
        if let Err(why) = only_publication_trust(&run.report, &trust) {
            return nothing(format!(
                "`prikk verify` after the removal is not a clean publication-trust failure ({why})"
            ));
        }
        let earlier: Vec<(&str, &Excused)> = earlier.iter().map(|(k, e)| (k.as_str(), e)).collect();
        let (record, left_out) = record_of(&trust, signers, retired_key, &earlier);
        if record.len() > MAX_EXCUSED_SUBJECTS {
            return nothing(format!(
                "the retirement names {} findings and a record keeps at most {MAX_EXCUSED_SUBJECTS}",
                record.len()
            ));
        }
        let note = (left_out > 0).then(|| {
            format!(
                "{left_out} block(s) that now lack a trusted signature were not sealed by the retired key (the verify before the removal did not list it as their signer) and were left out of the record; they will stop the wiki"
            )
        });
        (record, note)
    }
}

#[cfg(test)]
mod tests;
