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
//!    when step 1 listed the retired key as its signer, and only if prikk counted the same number of publication
//!    trust records before and after (nothing was published in between; Q4).
//!
//! **A retirement never strands a wiki** (handoff 004, Q1). Before step 2 the key record stores a `retiring` state
//! (the blocks step 1 listed as the key's, the count, the tip of every view); step 3 clears it when the findings are
//! recorded. If anything between the removal and the record fails (`verify` cannot run, the two looks disagree, the
//! count line is missing, the record is too large, the key store cannot be written) the key *is* out of the trust
//! store, the state stays, [`OldKeyOutcome::RetiredUnrecorded`] says so, `inklkunn wiki keys rotate` exits non-zero
//! naming the command, and [`Signer::finish_retirement`] (`inklkunn wiki keys finish-retirement <wiki>`) completes
//! step 3 **from the state**, under the write lock and only while the views' tips are those the state kept: a
//! block written since could not be told from what the removal made fail, so the record is not made from a changed
//! repository. Until it is finished nothing is excused: acceptances are *sealed, not confirmed* and recovery stops,
//! each naming the command.
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

use super::lifecycle::OldKeyOutcome;
use super::{Signer, SignerError, ks};
use crate::KeyAuthority;
use crate::key_store::{Excused, MAX_EXCUSED_SUBJECTS, MAX_RECORDED_VIEWS, RecordedTip, Retiring};
use crate::writes::{
    PublicationTrust, VerifyRun, checked_publication_trust_records, parse_publication_trust,
    parse_sealed_blocks_once,
};

#[cfg(test)]
thread_local! {
    /// Tests lower the number of findings a retirement may record (the limit is 150,000: not a number to reach).
    pub(crate) static RECORD_LIMIT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// The most findings the record of a retirement keeps.
fn record_limit() -> usize {
    #[cfg(test)]
    if let Some(n) = RECORD_LIMIT.with(std::cell::Cell::get) {
        return n;
    }
    MAX_EXCUSED_SUBJECTS
}

/// The command that finishes an unfinished retirement of `wiki`.
pub fn finish_retirement_command(wiki: &WikiId) -> String {
    format!("inklkunn wiki keys finish-retirement {wiki}")
}

/// What [`Signer::finish_retirement`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetirementFinished {
    /// The retired maintainer key.
    pub maintainer_key_id: String,
    /// How many blocks, ref states and ref updates the record now holds for it.
    pub findings_recorded: usize,
    /// Blocks that lack a trusted signature and were not the retired key's (so are not excused), when there are some.
    pub note: Option<String>,
}

/// What the `verify` just before a retirement said, and kept for the record.
pub(super) struct Before {
    /// The blocks it listed as sealed by the key being retired.
    pub(super) signed_by_key: Vec<String>,
    /// How many publication trust records it checked.
    pub(super) checked_records: u64,
}

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

/// What a retirement records from the `verify` just after the key left the trust store, given the blocks the verify
/// before listed as sealed by the retired key (`signed_by_key`), and what earlier retirements recorded already (those
/// are not recorded again).
///
/// Returns the new findings and how many named blocks were **left out** because the verify before did not list the
/// retired key as their signer.
pub(crate) fn record_of(
    trust: &PublicationTrust,
    signed_by_key: &[String],
    earlier: &Recorded,
) -> (Excused, usize) {
    let earlier_has = |pick: fn(&Excused) -> &Vec<String>, id: &String| {
        earlier.iter().any(|(_, e)| pick(e).contains(id))
    };
    let signed_by_retired: HashSet<&str> = signed_by_key.iter().map(String::as_str).collect();
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
    /// back what the look named for the record: the blocks sealed by `retired_key` and the number of publication trust
    /// records it checked. A look that did not run to the end, that lists a block twice, or that prints no count is
    /// not a repository known to verify (Q4, Q7).
    pub(super) fn verify_before_retirement(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
        retired_key: &str,
    ) -> Result<Before, SignerError> {
        let run = self.writes.verify_run(repo).map_err(|e| match e {
            SignerError::Unavailable(_) | SignerError::Damaged(_) => SignerError::NeedsRecovery(
                format!("`prikk verify` could not be run to the end ({e}); a key is retired only from a repository known to verify"),
            ),
            other => other,
        })?;
        if run.report.is_partial() {
            return Err(SignerError::NeedsRecovery(
                "`prikk verify` did not run to the end, so the key was not retired and is still trusted; run `inklkunn recover` for the wiki and retire the key afterwards".to_owned(),
            ));
        }
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
        let checked = checked_publication_trust_records(&run.text).ok_or_else(|| {
            SignerError::NeedsRecovery(
                "`prikk verify` did not print how many publication trust records it checked, so a retirement could not later prove that nothing was published in between; the key was not retired and is still trusted".to_owned(),
            )
        })?;
        if let Some(n) = too_many_to_record(&run) {
            return Err(SignerError::Refused(format!(
                "the wiki has {n} signed records and a retirement records at most {MAX_EXCUSED_SUBJECTS}; the key was not retired and is still trusted"
            )));
        }
        let signers = parse_sealed_blocks_once(&run.text).map_err(|why| {
            SignerError::NeedsRecovery(format!(
                "{why}; the key was not retired and is still trusted"
            ))
        })?;
        Ok(Before {
            signed_by_key: signers
                .into_iter()
                .filter(|(_, k)| k == retired_key)
                .map(|(b, _)| b)
                .collect(),
            checked_records: checked,
        })
    }

    /// The tip of every view now, as a retirement keeps them (refused past [`MAX_RECORDED_VIEWS`]).
    pub(super) fn current_tips(
        &self,
        repo: &std::path::Path,
    ) -> Result<Vec<RecordedTip>, SignerError> {
        let views = self.writes.all_views(repo)?;
        if views.len() > MAX_RECORDED_VIEWS {
            return Err(SignerError::Refused(format!(
                "the wiki has {} views and a rotation records the newest block of each, at most {MAX_RECORDED_VIEWS}",
                views.len()
            )));
        }
        let mut tips = Vec::with_capacity(views.len());
        for view in &views {
            let block_id = self
                .writes
                .tip(repo, view)?
                .map(|b| b.block_id)
                .unwrap_or_default();
            tips.push(RecordedTip {
                view: view.clone(),
                block_id,
            });
        }
        Ok(tips)
    }

    /// Step 3 of a retirement: `verify` just after the key left the trust store, and the findings to record from it
    /// (see [`record_of`]), from the state step 1 kept. `Err` is why nothing can be recorded: the key *is* removed
    /// from trust, nothing is excused, and the state stays for [`Signer::finish_retirement`].
    ///
    /// On success the second value is the note about blocks that lack a trusted signature without being the retired
    /// key's (they are not recorded and will stop the wiki), when there are some.
    pub(super) fn findings_after_retirement(
        &self,
        repo: &std::path::Path,
        retiring: &Retiring,
        earlier: &[(String, Excused)],
    ) -> Result<(Excused, Option<String>), String> {
        let run = self.writes.verify_run(repo).map_err(|e| {
            format!("`prikk verify` could not be run after the key was removed from trust ({e})")
        })?;
        if run.report.verdict.ok {
            return Ok((Excused::default(), None));
        }
        let trust = parse_publication_trust(&run.text);
        only_publication_trust(&run.report, &trust).map_err(|why| {
            format!(
                "`prikk verify` after the removal is not a clean publication-trust failure ({why})"
            )
        })?;
        // Nothing was published between the look before and the look after (Q4): prikk counts what it checked.
        let after = checked_publication_trust_records(&run.text).ok_or_else(|| {
            "`prikk verify` after the removal did not print how many publication trust records it checked".to_owned()
        })?;
        if after != retiring.checked_records {
            return Err(format!(
                "prikk checked {} publication trust records before the removal and {after} after it: something was published in between, and the findings cannot be told from the removal's",
                retiring.checked_records
            ));
        }
        let earlier: Vec<(&str, &Excused)> = earlier.iter().map(|(k, e)| (k.as_str(), e)).collect();
        let (record, left_out) = record_of(&trust, &retiring.signed_by_key, &earlier);
        if record.len() > record_limit() {
            return Err(format!(
                "the retirement names {} findings and a record keeps at most {}",
                record.len(),
                record_limit()
            ));
        }
        let note = (left_out > 0).then(|| {
            format!(
                "{left_out} block(s) that now lack a trusted signature were not sealed by the retired key (the verify before the removal did not list it as their signer) and were left out of the record; they will stop the wiki"
            )
        });
        Ok((record, note))
    }

    /// The key a retirement was begun for and not finished, if there is one (the key record's `retiring` state).
    pub fn unfinished_retirement(&self, wiki: &WikiId) -> Result<Option<String>, SignerError> {
        let meta = self.store.status(wiki).map_err(ks)?;
        Ok(meta.retiring.map(|r| r.maintainer_key_id))
    }

    /// Finish the record of a retirement that was begun and not recorded (handoff 004, Q1): the key record's
    /// `retiring` state is read, and — under the wiki's write lock, **only while every view's tip is the block the
    /// state kept** and the key is out of the trust store — the `verify` after the removal is taken and its findings
    /// recorded exactly as the retirement itself would have, the state cleared. Refused, with nothing changed, when
    /// nothing is unfinished, when something was written since (the record is never made from a changed repository),
    /// when the key is still trusted, or when the `verify` is not a clean publication-trust failure with the count
    /// the retirement kept. An administrator's act: the caller has authorized and audited it.
    pub fn finish_retirement(
        &self,
        _authority: &KeyAuthority,
        repo: &std::path::Path,
    ) -> Result<RetirementFinished, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        let meta = self.store.status(&wiki).map_err(ks)?;
        let Some(retiring) = meta.retiring.clone() else {
            return Err(SignerError::Refused(
                "no key retirement of this wiki is waiting to be finished; nothing was changed"
                    .to_owned(),
            ));
        };
        let key = &retiring.maintainer_key_id;
        let mut now = self.current_tips(repo)?;
        let mut kept = retiring.views.clone();
        now.sort_by(|a, b| a.view.cmp(&b.view));
        kept.sort_by(|a, b| a.view.cmp(&b.view));
        if now != kept {
            return Err(SignerError::Refused(format!(
                "the wiki was written, or its views changed, since the retirement of key {key} was begun, so its record is not made from a repository that has changed (a block written since could not be told from what the removal made fail); nothing was changed"
            )));
        }
        if self.writes.maintainer_trusted(repo, key)? {
            return Err(SignerError::Refused(format!(
                "the key {key} is still trusted in the repository, so its retirement did not get as far as the removal; remove it with `prikk trust maintainer remove --key-id {key}` in the wiki's repository and run this command again; nothing was changed"
            )));
        }
        let earlier: Vec<(String, Excused)> = meta
            .superseded
            .iter()
            .filter(|s| s.retired)
            .map(|s| (s.maintainer_key_id.clone(), s.excused.clone()))
            .collect();
        let (found, note) = self
            .findings_after_retirement(repo, &retiring, &earlier)
            .map_err(|why| {
                SignerError::Refused(format!(
                    "the retirement of key {key} cannot be finished: {why}; nothing was changed"
                ))
            })?;
        let findings_recorded = found.len();
        self.store
            .update_meta(&wiki, |m| {
                m.retiring = None;
                if let Some(last) = m.superseded.last_mut() {
                    last.retired = true;
                    last.excused = found;
                }
            })
            .map_err(ks)?;
        Ok(RetirementFinished {
            maintainer_key_id: key.clone(),
            findings_recorded,
            note,
        })
    }

    /// What the signer says of a retirement that ended without its record, for the text of a refusal or a stop:
    /// the key and the command that finishes it; `None` when nothing is unfinished or the record cannot be read.
    pub(crate) fn unfinished_retirement_note(&self, wiki: &WikiId) -> Option<String> {
        let key = self.unfinished_retirement(wiki).ok().flatten()?;
        Some(format!(
            "the retirement of key {key} was begun and its record not finished, so nothing is excused: run `{}`",
            finish_retirement_command(wiki)
        ))
    }
}

/// The outcome a retirement reports when its record could not be completed.
pub(super) fn unrecorded(why: impl Into<String>, wiki: &WikiId) -> OldKeyOutcome {
    OldKeyOutcome::RetiredUnrecorded(format!(
        "{}; the key is out of the trust store and nothing is excused until the record is finished: run `{}`",
        why.into(),
        finish_retirement_command(wiki)
    ))
}

#[cfg(test)]
mod tests;
