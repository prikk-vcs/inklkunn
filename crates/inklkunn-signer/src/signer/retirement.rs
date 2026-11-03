//! A retired key never freezes the wiki, and never hides a tampering (handoff 004, P2 and P2′; DR-10; ledger PK-34).
//!
//! **What the owner ruled** (DR-10): retiring a maintainer key costs the wiki its *Verified* label, because prikk
//! judges the whole history against today's trust set and the blocks the retired key sealed no longer verify. The
//! label is the known cost. A frozen wiki is not: before this check every acceptance after a retirement answered
//! *sealed, but not confirmed* and recovery's `verify` step always failed.
//!
//! **What is excused is the retirement's own record, and nothing wider.** A retirement works in this order, under the
//! wiki's write lock. It is one code path whichever way it is asked for (X1): [`Signer::retire_key`]
//! (`inklkunn wiki keys retire <wiki> --key <id>`) retires any **superseded** maintainer key still trusted in the
//! repository, found by its id, and [`Signer::rotate_keys`] with `OldKey::Retire` is the rotation followed by that same
//! retirement of the key it just replaced:
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
//! **A retirement never strands a wiki** (handoff 004, Q1, W1 – W7). Before step 2 the key record stores a `retiring`
//! state: the blocks step 1 listed as the key's, the count, the tip of every view, the retired key's public key and
//! the ids of the maintainer keys the repository trusts. Step 3 clears it when the findings are recorded. If the
//! retirement stops anywhere between the state and the record (killed, `verify` cannot run, the two looks disagree,
//! the count line is missing, the record is too large, the key store cannot be written, the removal fails) the state
//! stays, and **while it stays the signer seals nothing new** (W1): an acceptance, recovery's `seal_queued` and the
//! creation of a view are refused before anything changes, in one sentence that names the command, and the proposal
//! is kept. A block written meanwhile could not be told from what the removal made fail, and would close the only
//! way out. There are two ways out, both under the write lock and both audited by the caller:
//!
//! - [`Signer::finish_retirement`] (`inklkunn wiki keys finish-retirement <wiki>`) does **whatever step remains**
//!   (W2): with the key still trusted it takes a fresh `verify` (which must pass), rewrites the state from it and
//!   removes the key itself; with the key already removed it requires every view's tip to be the one the state kept
//!   and the trust list to be exactly the kept list without the key (W5). Then it takes the `verify` after and records
//!   its findings, **and records nothing** when a block that lost its signature was not the key's (W4).
//! - [`Signer::abandon_retirement`] (`inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>`) gives the
//!   retirement up (W3): the rotation stands, the old key is trusted again (adopted again from the public key the
//!   state kept, if it had been removed), nothing is retired.
//!
//! Every refusal of the first that no later run can cure names the second. Whichever ends a retirement, the caller
//! records a fresh verdict (W6): the stored explanation of an earlier verdict never outlives its truth.
//!
//! **What abandoning costs, and what it must never do.** After an abandon the old key is trusted and **nothing is
//! retired**; `inklkunn wiki keys retire <wiki> --key <id>` retires it later (X1), and every text that names abandon
//! says so. Abandon never adopts again a key whose retirement the audit trail records as finished (X2, [`trail`]): a
//! key record restored from a backup taken while the retirement was unfinished is older than the trail, and abandoning
//! from it would trust a key that was retired on purpose.
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
use inklkunn_store::{AuditLog, WikiId};

use super::lifecycle::OldKeyOutcome;
use super::{Signer, SignerError, ks};
use crate::KeyAuthority;
use crate::key_store::{Excused, MAX_EXCUSED_SUBJECTS, MAX_RECORDED_VIEWS, RecordedTip, Retiring};
use crate::writes::{
    PublicationTrust, VerifyRun, checked_publication_trust_records, parse_publication_trust,
    parse_sealed_blocks_once, pending_cleanup_findings,
};

pub mod trail;

pub use trail::{
    ACTION_FINISH_RETIREMENT, ACTION_RETIRE, ACTION_ROTATE, recorded_detail, recorded_key,
    retirement_finished_in,
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

/// The excuse a judged failure comes to, with the words that say what failed.
fn excuse_of(named: Excused, keys: Vec<String>, what: &str) -> RetirementExcuse {
    let n = named.len();
    RetirementExcuse {
        note: format!(
            "{what}: {n} publication-trust finding(s), all of them recorded by the retirement of key {}; the badge still says Verification failed (DR-10)",
            keys.join(", ")
        ),
        keys,
        blocks: named.blocks,
        ref_states: named.ref_states,
        ref_updates: named.ref_updates,
    }
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
    /// The command removed the key from the repository's trust itself (it was still trusted).
    pub removed_now: bool,
}

/// What [`Signer::abandon_retirement`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetirementAbandoned {
    /// The maintainer key that was to be retired and is not.
    pub maintainer_key_id: String,
    /// It had been removed from the repository's trust and was adopted again (otherwise it was trusted when
    /// abandoning began: it was never removed, or it was added again outside inklkunn).
    pub restored: bool,
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
    trust_lines(trust)
}

/// The publication-trust lines of the text are of the known shape and as many as prikk counts. Returns how many.
fn trust_lines(trust: &PublicationTrust) -> Result<usize, &'static str> {
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
    judge_named(trust, recorded)
}

/// The condition ids prikk gives a publication a killed `seal` left unfinished.
const CLEANUP_CONDITION: &str = "blocking-ref-publication";

/// As [`judge`], for a repository a killed `seal` left with a publication to finish (handoff 004, W7): `verify` fails
/// by **exactly** `blocking-ref-publication` and `publication-trust` (nothing else, and run to the end), the text
/// holds the pending-cleanup finding once and says one ref-publication issue, and every publication-trust finding is
/// in a recorded set. The rule for the findings is [`judge`]'s, unchanged; this only lets the cleanup stand beside
/// them, so that recovery can finish it. After the cleanup is finished the plain [`judge`] must hold again.
pub(crate) fn judge_beside_cleanup(
    report: &Verification,
    text: &str,
    trust: &PublicationTrust,
    recorded: &Recorded,
) -> Judgement {
    if report.verdict.ok {
        return Judgement::Unexplained("the repository verifies; there is nothing to explain");
    }
    if report.is_partial() {
        return Judgement::Unexplained("the verification did not run to the end");
    }
    let mut ids: Vec<&str> = report
        .verdict
        .failed_conditions
        .iter()
        .filter_map(|c| c.get("id").and_then(serde_json::Value::as_str))
        .collect();
    ids.sort_unstable();
    if ids != [CLEANUP_CONDITION, CONDITION] || report.verdict.failed_conditions.len() != 2 {
        return Judgement::Unexplained(
            "the failed conditions are not exactly a pending cleanup and publication trust",
        );
    }
    if pending_cleanup_findings(text) != (1, true) {
        return Judgement::Unexplained("the text does not name exactly one pending cleanup");
    }
    if let Err(why) = trust_lines(trust) {
        return Judgement::Unexplained(why);
    }
    judge_named(trust, recorded)
}

/// The findings of a failure already known to be publication trust (and a cleanup, perhaps), against the recorded sets.
fn judge_named(trust: &PublicationTrust, recorded: &Recorded) -> Judgement {
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
                Some(excuse_of(named, keys, "only publication trust fails"))
            }
            Judgement::Unexplained(_) => None,
        })
    }

    /// As [`Signer::retirement_explains`], for a repository a killed `seal` left with a publication to finish (handoff
    /// 004, W7): `verify` fails by exactly `blocking-ref-publication` and `publication-trust`, and every
    /// publication-trust finding it names is in a retirement's recorded set ([`judge_beside_cleanup`]). Recovery offers
    /// `complete-publication` on this and on nothing wider; afterwards the plain rule must hold. Read-only, no keys.
    pub(crate) fn retirement_explains_beside_cleanup(
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
        let run = self.writes.verify_run_beside_cleanup(repo)?;
        if run.report.verdict.ok {
            return Ok(None);
        }
        let trust = parse_publication_trust(&run.text);
        let recorded: Vec<(&str, &Excused)> =
            retired.iter().map(|(k, e)| (k.as_str(), e)).collect();
        Ok(
            match judge_beside_cleanup(&run.report, &run.text, &trust, &recorded) {
                Judgement::Explained { named, keys } => Some(excuse_of(
                    named,
                    keys,
                    "only publication trust and the cleanup of a killed seal fail",
                )),
                Judgement::Unexplained(_) => None,
            },
        )
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
                "`prikk verify` did not run to the end, so the key was not retired and is still trusted".to_owned(),
            ));
        }
        if !run.report.verdict.ok {
            let trust = parse_publication_trust(&run.text);
            let retired = self.retired_records(wiki)?;
            let recorded: Vec<(&str, &Excused)> =
                retired.iter().map(|(k, e)| (k.as_str(), e)).collect();
            if let Judgement::Unexplained(why) = judge(&run.report, &trust, &recorded) {
                return Err(SignerError::NeedsRecovery(format!(
                    "`prikk verify` fails ({why}), so the key was not retired and is still trusted"
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
    /// from trust, nothing is excused, and the state stays for [`Signer::finish_retirement`] (when a later run can
    /// cure it) or for [`Signer::abandon_retirement`] (when none can).
    ///
    /// **Nothing is recorded when a block that lost its signature was not the retired key's** (W4): such a block would
    /// stop the wiki, no command adds it to a record, and a record that looks complete while it is not is worse than
    /// none. The way out is [`Signer::abandon_retirement`].
    pub(super) fn findings_after_retirement(
        &self,
        repo: &std::path::Path,
        retiring: &Retiring,
        earlier: &[(String, Excused)],
    ) -> Result<Excused, Cannot> {
        let run = self.writes.verify_run(repo).map_err(|e| {
            Cannot::later(format!(
                "`prikk verify` could not be run after the key was removed from trust ({e})"
            ))
        })?;
        if run.report.verdict.ok {
            return Ok(Excused::default());
        }
        let trust = parse_publication_trust(&run.text);
        only_publication_trust(&run.report, &trust).map_err(|why| {
            Cannot::never(format!(
                "`prikk verify` after the removal is not a clean publication-trust failure ({why})"
            ))
        })?;
        // Nothing was published between the look before and the look after (Q4): prikk counts what it checked.
        let after = checked_publication_trust_records(&run.text).ok_or_else(|| {
            Cannot::later("`prikk verify` after the removal did not print how many publication trust records it checked".to_owned())
        })?;
        if after != retiring.checked_records {
            return Err(Cannot::never(format!(
                "prikk checked {} publication trust records before the removal and {after} after it: something was published in between, and the findings cannot be told from the removal's",
                retiring.checked_records
            )));
        }
        let earlier: Vec<(&str, &Excused)> = earlier.iter().map(|(k, e)| (k.as_str(), e)).collect();
        let (record, left_out) = record_of(&trust, &retiring.signed_by_key, &earlier);
        if left_out > 0 {
            return Err(Cannot::never(format!(
                "{left_out} block(s) that now lack a trusted signature were not sealed by the retired key (the verify before the removal did not list it as their signer), so no record of this retirement would be complete, and they would stop the wiki"
            )));
        }
        if record.len() > record_limit() {
            return Err(Cannot::never(format!(
                "the retirement names {} findings and a record keeps at most {}",
                record.len(),
                record_limit()
            )));
        }
        Ok(record)
    }

    /// The end of a retirement, once the key is out of the trust store (W2, W6): the `verify` after, the record, the
    /// `retiring` state cleared. Returns how many findings were recorded. Under the wiki's write lock, which the
    /// caller holds.
    fn conclude_retirement(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
        retiring: &Retiring,
    ) -> Result<usize, Cannot> {
        let meta = self
            .store
            .status(wiki)
            .map_err(|e| Cannot::later(format!("the key record could not be read ({})", ks(e))))?;
        // The record is made on **that key's** entry, found by its id (X1): the key being retired is not always the
        // newest superseded one.
        let key = retiring.maintainer_key_id.as_str();
        if !meta
            .superseded
            .iter()
            .any(|s| s.maintainer_key_id == key && !s.retired)
        {
            return Err(Cannot::never(format!(
                "the key record holds no superseded key {key} that is still to be retired, so there is no entry to record the findings on"
            )));
        }
        let earlier: Vec<(String, Excused)> = meta
            .superseded
            .iter()
            .filter(|s| s.retired)
            .map(|s| (s.maintainer_key_id.clone(), s.excused.clone()))
            .collect();
        let found = self.findings_after_retirement(repo, retiring, &earlier)?;
        let recorded = found.len();
        self.store
            .update_meta(wiki, |m| {
                m.retiring = None;
                if let Some(entry) = m
                    .superseded
                    .iter_mut()
                    .find(|s| s.maintainer_key_id == key && !s.retired)
                {
                    entry.retired = true;
                    entry.excused = found;
                }
            })
            .map_err(|e| {
                Cannot::later(format!("the key record could not be updated ({})", ks(e)))
            })?;
        Ok(recorded)
    }

    /// Run the end of a retirement after the key left the trust store: the outcome it reports.
    fn conclude_retirement_outcome(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
        retiring: &Retiring,
    ) -> OldKeyOutcome {
        match self.conclude_retirement(repo, wiki, retiring) {
            Ok(_) => OldKeyOutcome::Retired,
            Err(cannot) => unrecorded(&cannot, wiki, &retiring.maintainer_key_id),
        }
    }

    /// The maintainer keys the repository trusts now, read for a `retiring` state: the retired key's public key and
    /// the sorted ids (the retired key and the new one among them). `NeedsRecovery`-free: a repository that does not
    /// list the retired key as trusted is [`SignerError::Refused`].
    pub(super) fn retiring_trust_facts(
        &self,
        repo: &std::path::Path,
        retired_key: &str,
    ) -> Result<(String, Vec<String>), SignerError> {
        let listing = self.writes.trusted_maintainers(repo)?;
        let Some(retired) = listing.iter().find(|k| k.key_id == retired_key) else {
            return Err(SignerError::Refused(format!(
                "the key {retired_key} is not trusted in the repository, so there is no retirement to begin; nothing was changed"
            )));
        };
        let mut ids: Vec<String> = listing.iter().map(|k| k.key_id.clone()).collect();
        ids.sort();
        Ok((retired.public_key.clone(), ids))
    }

    /// The `retiring` state for `key` from the look `before` took and the repository as it is now: the blocks `before`
    /// listed as the key's, the count, the tip of every view, the key's public key and the sorted trusted ids. Read
    /// only; refused when the repository does not trust the key.
    fn begun_state(
        &self,
        repo: &std::path::Path,
        key: &str,
        before: Before,
    ) -> Result<Retiring, SignerError> {
        let (maintainer_public_key, trusted_maintainers) = self.retiring_trust_facts(repo, key)?;
        Ok(Retiring {
            maintainer_key_id: key.to_owned(),
            signed_by_key: before.signed_by_key,
            checked_records: before.checked_records,
            views: self.current_tips(repo)?,
            maintainer_public_key,
            trusted_maintainers,
        })
    }

    /// The one retirement (X1), whichever command asked for it, under the wiki's write lock the caller holds, after
    /// `before` (the `verify` that must pass) was taken: write the `retiring` state for the superseded key `key` (its
    /// id and public key, the blocks `before` listed as its own, the tip of every view, the trusted ids), remove the
    /// key from the repository's trust, and conclude (the `verify` after, the record made on the key's own entry, the
    /// state cleared). `Err` is a refusal before the state was written: **nothing was changed**. After that the
    /// outcome says what became of the key, as [`OldKeyOutcome`] documents.
    pub(super) fn retire_superseded(
        &self,
        repo: &std::path::Path,
        wiki: &WikiId,
        key: &str,
        before: Before,
    ) -> Result<OldKeyOutcome, SignerError> {
        let retiring = self.begun_state(repo, key, before)?;
        let begun = retiring.clone();
        self.store
            .update_meta(wiki, |m| m.retiring = Some(begun))
            .map_err(|e| {
                SignerError::Unavailable(format!(
                    "the retirement could not be begun: the key record could not be written ({}); nothing was changed",
                    ks(e)
                ))
            })?;
        Ok(match self.writes.untrust_maintainer(repo, key) {
            // `verify` just after names what lost its trusted signature; that set is what the record keeps, so that a
            // later failure is excused by it and by nothing else (P2′). Anything that keeps the record from being made
            // leaves the `retiring` state in the key record (Q1): `finish-retirement` makes it from there, or
            // `abandon-retirement` gives it up.
            Ok(()) => self.conclude_retirement_outcome(repo, wiki, &retiring),
            Err(e) => OldKeyOutcome::RetireFailed(e.to_string()),
        })
    }

    /// Retire a **superseded** maintainer key that is still trusted in the repository, by its id (handoff 004, X1;
    /// `inklkunn wiki keys retire <wiki> --key <id> --confirm <slug>`), under the wiki's write lock: the same
    /// before-`verify` (which must pass), `retiring` state, removal, after-`verify` and record as `rotate --retire-old`
    /// makes for the key it just replaced, with the record made on **that key's** entry. Refused, with nothing changed,
    /// for the wiki's current key (*rotate first*), for an id the key record does not know, for a key already retired,
    /// for a key the repository does not trust, while another retirement is unfinished, and when `verify` does not
    /// pass. After the state was written the answer is the outcome, as for a rotation. An administrator's act: the
    /// caller has authorized and audited it.
    pub fn retire_key(
        &self,
        _authority: &KeyAuthority,
        repo: &std::path::Path,
        key: &str,
    ) -> Result<OldKeyOutcome, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        let meta = self.store.status(&wiki).map_err(ks)?;
        if let Some(open) = &meta.retiring {
            return Err(unfinished_refusal(&wiki, &open.maintainer_key_id));
        }
        if key == meta.maintainer_key_id {
            return Err(SignerError::Refused(format!(
                "the key {key} is the wiki's current maintainer key, and a key is retired only after it was replaced: rotate first (`inklkunn wiki keys rotate {wiki} --confirm <the wiki's slug>`), then retire it; nothing was changed"
            )));
        }
        let Some(entry) = meta.superseded.iter().find(|s| s.maintainer_key_id == key) else {
            let known: Vec<&str> = meta
                .superseded
                .iter()
                .filter(|s| !s.retired)
                .map(|s| s.maintainer_key_id.as_str())
                .collect();
            return Err(SignerError::Refused(format!(
                "the key record of this wiki does not know a maintainer key {key}; the superseded keys still to be retired are: {}; nothing was changed",
                if known.is_empty() {
                    "none".to_owned()
                } else {
                    known.join(", ")
                }
            )));
        };
        if entry.retired {
            return Err(SignerError::Refused(format!(
                "the key {key} was retired already; nothing was changed"
            )));
        }
        // Not trusted any more (removed outside inklkunn, say): there is nothing to retire, and no look is needed.
        self.retiring_trust_facts(repo, key)?;
        let before = self
            .verify_before_retirement(repo, &wiki, key)
            .map_err(|e| {
                recovery_first(
                    &wiki,
                    e,
                    &format!("then retire the key with `{}`", retire_command(&wiki, key)),
                )
            })?;
        self.retire_superseded(repo, &wiki, key, before)
    }

    /// The key a retirement was begun for and not finished, if there is one (the key record's `retiring` state).
    pub fn unfinished_retirement(&self, wiki: &WikiId) -> Result<Option<String>, SignerError> {
        let meta = self.store.status(wiki).map_err(ks)?;
        Ok(meta.retiring.map(|r| r.maintainer_key_id))
    }

    /// Finish a retirement that was begun and not finished (handoff 004, Q1, W2, W4, W5), from whatever step remains,
    /// under the wiki's write lock:
    ///
    /// - **The key is still trusted** (the retirement stopped before the removal, or the removal failed): a fresh
    ///   `verify` is taken (it must pass, as `rotate` requires), the `retiring` state is rewritten from it, **this
    ///   command removes the key from the trust store itself**, and the rest follows.
    /// - **The key is out of the trust store**: only while every view's tip is the block the state kept and the trust
    ///   list is exactly the list the state kept without the retired key; a block written since, or a key added or
    ///   removed outside inklkunn, could not be told from what the removal made fail.
    ///
    /// Then the `verify` after the removal is taken and its findings recorded exactly as the retirement itself would
    /// have, the state cleared. Refused, with nothing recorded and the state kept, when nothing is unfinished, when
    /// the repository changed outside inklkunn, when the count of what prikk checked differs, when a finding is not
    /// the retirement's, when a block that lost its signature was not the retired key's (W4), or when `verify` cannot
    /// run. A refusal no later run can cure names `inklkunn wiki keys abandon-retirement`. An administrator's act: the
    /// caller has authorized and audited it.
    pub fn finish_retirement(
        &self,
        _authority: &KeyAuthority,
        repo: &std::path::Path,
    ) -> Result<RetirementFinished, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        let meta = self.store.status(&wiki).map_err(ks)?;
        let Some(mut retiring) = meta.retiring.clone() else {
            return Err(SignerError::Refused(
                "no key retirement of this wiki is waiting to be finished; nothing was changed"
                    .to_owned(),
            ));
        };
        let key = retiring.maintainer_key_id.clone();
        let listing = self.writes.trusted_maintainers(repo).map_err(|e| {
            SignerError::Unavailable(format!(
                "the repository's trusted maintainer keys could not be read ({e}); nothing was changed; run `{}` again",
                finish_retirement_command(&wiki)
            ))
        })?;
        let mut removed_now = false;
        if listing.iter().any(|k| k.key_id == key) {
            // W2: the removal never happened. Begin again from a repository that verifies, here and now.
            let before = self
                .verify_before_retirement(repo, &wiki, &key)
                .map_err(|e| match e {
                    SignerError::NeedsRecovery(why) => SignerError::NeedsRecovery(format!(
                        "{why}; the retirement stays begun: once the wiki verifies (`inklkunn wiki recover {wiki}`), `{}` takes the look again; or {}; nothing was changed",
                        finish_retirement_command(&wiki),
                        abandon_text(&wiki, &key)
                    )),
                    other => other,
                })?;
            retiring = self.begun_state(repo, &key, before)?;
            let begun = retiring.clone();
            self.store
                .update_meta(&wiki, |m| m.retiring = Some(begun))
                .map_err(ks)?;
            self.writes
                .untrust_maintainer(repo, &key)
                .map_err(|e| {
                    SignerError::Refused(format!(
                        "removing the key {key} from the repository's trust failed ({e}); the retirement stays begun: run `{}` again; or {}; nothing else was changed",
                        finish_retirement_command(&wiki),
                        abandon_text(&wiki, &key)
                    ))
                })?;
            removed_now = true;
        } else {
            let mut now = self.current_tips(repo)?;
            let mut kept = retiring.views.clone();
            now.sort_by(|a, b| a.view.cmp(&b.view));
            kept.sort_by(|a, b| a.view.cmp(&b.view));
            if now != kept {
                return Err(SignerError::Refused(format!(
                    "the wiki was written, or its views changed, since the retirement of key {key} was begun, so its record is not made from a repository that has changed (a block written since could not be told from what the removal made fail); no later run of `{}` can cure this: {}; nothing was changed",
                    finish_retirement_command(&wiki),
                    abandon_text(&wiki, &key)
                )));
            }
            // W5: the trust list is the list the retirement began from, without the retired key.
            let mut expected: Vec<&str> = retiring
                .trusted_maintainers
                .iter()
                .map(String::as_str)
                .filter(|id| *id != key)
                .collect();
            let mut actual: Vec<&str> = listing.iter().map(|k| k.key_id.as_str()).collect();
            expected.sort_unstable();
            actual.sort_unstable();
            if expected != actual {
                return Err(SignerError::Refused(format!(
                    "the repository's trusted maintainer keys are not those it trusted when the retirement of key {key} was begun, less that key (another key was added or removed outside inklkunn), so what the removal made fail cannot be told from what the other change made fail; no later run of `{}` can cure this: {}; nothing was changed",
                    finish_retirement_command(&wiki),
                    abandon_text(&wiki, &key)
                )));
            }
        }
        let findings_recorded = self.conclude_retirement(repo, &wiki, &retiring).map_err(
            |cannot| {
                SignerError::Refused(format!(
                    "the retirement of key {key} cannot be finished: {}; {}; nothing was recorded",
                    cannot.why,
                    way_out(&wiki, &key, cannot.curable)
                ))
            },
        )?;
        Ok(RetirementFinished {
            maintainer_key_id: key,
            findings_recorded,
            removed_now,
        })
    }

    /// Give up a retirement that was begun and cannot be finished (handoff 004, W3), under the wiki's write lock:
    /// with the old key **trusted** (id and public key) the state is cleared and the rotation stands, as a rotation that
    /// kept the old key; with the old key **removed** from the repository's trust it is adopted again, under its own
    /// id and with the public key the state kept, and then the state is cleared. Nothing is excused by it and nothing
    /// needs to be: the key is trusted, so what it sealed verifies. **The cost** (X1): the old key stays trusted and
    /// nothing is retired; `inklkunn wiki keys retire <wiki> --key <id>` retires it later.
    ///
    /// **A key whose retirement the audit trail records as finished is never adopted again** (X2): a key record
    /// restored from a backup taken while the retirement was unfinished is older than the trail, and the refusal says
    /// so. A trail that cannot be read is no answer, and the key is not adopted either. Refused, with nothing changed,
    /// also when nothing is unfinished and when the repository trusts another public key under the key's id. An
    /// administrator's act: the caller has authorized and audited it.
    pub fn abandon_retirement(
        &self,
        _authority: &KeyAuthority,
        repo: &std::path::Path,
        trail: &dyn AuditLog,
    ) -> Result<RetirementAbandoned, SignerError> {
        let wiki = self.wiki_of(repo)?;
        let _held = self.write_lock(&wiki)?;
        let meta = self.store.status(&wiki).map_err(ks)?;
        let Some(retiring) = meta.retiring else {
            return Err(SignerError::Refused(
                "no key retirement of this wiki is waiting to be abandoned; nothing was changed"
                    .to_owned(),
            ));
        };
        let key = retiring.maintainer_key_id;
        let again = format!("run `{}` again", abandon_retirement_command(&wiki));
        let listing = self.writes.trusted_maintainers(repo).map_err(|e| {
            SignerError::Unavailable(format!(
                "the repository's trusted maintainer keys could not be read ({e}); nothing was changed; {again}"
            ))
        })?;
        // Still trusted means the id **and** the public key the retirement began with (X4).
        let trusted = match listing.iter().find(|k| k.key_id == key) {
            Some(k) if k.public_key == retiring.maintainer_public_key => true,
            Some(_) => {
                return Err(SignerError::Refused(format!(
                    "the repository trusts a key under the id {key} whose public key is not the one the retirement began with, so it is not the old key and abandoning cannot tell what to put back; no inklkunn command changes this; nothing was changed"
                )));
            }
            None => false,
        };
        if !trusted {
            // X2: a key whose retirement finished is not adopted again from an older key record.
            match retirement_finished_in(trail, &wiki, &key) {
                Ok(false) => {}
                Ok(true) => {
                    return Err(SignerError::Refused(format!(
                        "the audit trail records the retirement of key {key} as finished, so the key record looks older than the audit trail; restore the key directory from a later backup; the key was not put back into the repository's trust and nothing was changed"
                    )));
                }
                Err(e) => {
                    return Err(SignerError::Unavailable(format!(
                        "the audit trail could not be read ({e}), so it cannot be told whether the retirement of key {key} was finished, and the key was not put back into the repository's trust; nothing was changed; {again}"
                    )));
                }
            }
            if let Err(e) =
                self.writes
                    .trust_maintainer(repo, &key, &retiring.maintainer_public_key)
            {
                return Err(SignerError::Unavailable(format!(
                    "putting the key {key} back into the repository's trust failed ({e}); the retirement stays begun, and the key may or may not have been added; {again}, which looks at the trust list first"
                )));
            }
            let listed = self.writes.trusted_maintainers(repo).map_err(|e| {
                SignerError::Unavailable(format!(
                    "the key {key} was added to the repository's trust, but the trust list could not be read afterwards ({e}); the retirement stays begun; {again}, which looks at the trust list first"
                ))
            })?;
            if !listed
                .iter()
                .any(|k| k.key_id == key && k.public_key == retiring.maintainer_public_key)
            {
                return Err(SignerError::Refused(format!(
                    "the key {key} was added to the repository's trust but the repository does not list it with its public key; the retirement stays begun; {again}, which looks at the trust list first"
                )));
            }
        }
        self.store
            .update_meta(&wiki, |m| m.retiring = None)
            .map_err(|e| {
                SignerError::Unavailable(format!(
                    "the old key {key} is trusted in the repository, but the key record could not be updated ({}), so the retirement is still begun; {again}",
                    ks(e)
                ))
            })?;
        Ok(RetirementAbandoned {
            maintainer_key_id: key,
            restored: !trusted,
        })
    }

    /// What the signer says of a retirement that was begun and not finished, for the text of a stop: the key and the
    /// ways out; `None` when nothing is unfinished or the record cannot be read.
    pub(crate) fn unfinished_retirement_note(&self, wiki: &WikiId) -> Option<String> {
        let key = self.unfinished_retirement(wiki).ok().flatten()?;
        Some(format!(
            "the retirement of key {key} was begun and its record not finished, so nothing is excused and nothing new is sealed: run `{}`, or {}",
            finish_retirement_command(wiki),
            abandon_text(wiki, &key)
        ))
    }

    /// The one sentence that refuses a new seal while a retirement is unfinished (W1); `Ok(None)` when none is. Read
    /// from the authenticated key record, under the write lock the caller holds, before anything is changed. **It fails
    /// closed** (X4): a key record that cannot be read refuses the seal, whose `Err` is the sentence for *unavailable*.
    pub(crate) fn seal_pause(&self, wiki: &WikiId) -> Result<Option<String>, String> {
        let key = match self.unfinished_retirement(wiki) {
            Ok(key) => key,
            Err(e) => {
                return Err(format!(
                    "the wiki's key record could not be read ({e}), so it cannot be told whether a change to its keys is unfinished; nothing was written"
                ));
            }
        };
        Ok(key.map(|key| {
            format!(
                "a change to this wiki's keys is not finished (the retirement of key {key} was begun), so nothing new is sealed until the person who runs this wiki runs `{}`; nothing was written",
                finish_retirement_command(wiki)
            )
        }))
    }
}

/// Why the end of a retirement cannot be recorded now, and whether a later run of `finish-retirement` can cure it.
#[derive(Debug)]
pub(super) struct Cannot {
    /// For the operator.
    pub(super) why: String,
    /// A later run can cure it (prikk could not run, the key record could not be written); otherwise only
    /// `abandon-retirement` is a way out.
    pub(super) curable: bool,
}

impl Cannot {
    fn later(why: String) -> Self {
        Cannot { why, curable: true }
    }

    fn never(why: String) -> Self {
        Cannot {
            why,
            curable: false,
        }
    }
}

/// The command that retires the superseded maintainer key `key` of `wiki` (X1).
pub fn retire_command(wiki: &WikiId, key: &str) -> String {
    format!("inklkunn wiki keys retire {wiki} --key {key} --confirm <the wiki's slug>")
}

/// The refusal of any new key change while the retirement of `key` is unfinished.
pub(super) fn unfinished_refusal(wiki: &WikiId, key: &str) -> SignerError {
    SignerError::Refused(format!(
        "the retirement of key {key} was begun and is not finished; finish it first with `{}`, or {}; nothing was changed",
        finish_retirement_command(wiki),
        abandon_text(wiki, key)
    ))
}

/// A look at `verify` that did not pass, said with what to do about it: recover the wiki, then `then`.
pub(super) fn recovery_first(wiki: &WikiId, e: SignerError, then: &str) -> SignerError {
    match e {
        SignerError::NeedsRecovery(why) => {
            SignerError::NeedsRecovery(format!("{why}; run `inklkunn wiki recover {wiki}`, {then}"))
        }
        other => other,
    }
}

/// The command that gives up an unfinished retirement of `wiki` (W3).
pub fn abandon_retirement_command(wiki: &WikiId) -> String {
    format!("inklkunn wiki keys abandon-retirement {wiki} --confirm <the wiki's slug>")
}

/// What abandoning does and what it costs, for the texts that name it (X1): the old key `key` is trusted again and
/// nothing is retired, until it is retired by its id.
fn abandon_text(wiki: &WikiId, key: &str) -> String {
    format!(
        "`{}` gives the retirement up: the old key {key} is trusted again (put back into the repository's trust if it was removed) and nothing is retired; to retire it later run `{}`",
        abandon_retirement_command(wiki),
        retire_command(wiki, key)
    )
}

/// What the operator can do about the retirement of `key` that is not finished.
fn way_out(wiki: &WikiId, key: &str, curable: bool) -> String {
    if curable {
        format!(
            "once the cause is gone run `{}` again; or {}",
            finish_retirement_command(wiki),
            abandon_text(wiki, key)
        )
    } else {
        format!(
            "no later run of `{}` can cure this: {}",
            finish_retirement_command(wiki),
            abandon_text(wiki, key)
        )
    }
}

/// The outcome a retirement reports when its record could not be completed.
fn unrecorded(cannot: &Cannot, wiki: &WikiId, key: &str) -> OldKeyOutcome {
    OldKeyOutcome::RetiredUnrecorded(format!(
        "{}; the key is out of the trust store and nothing is excused, and nothing new is sealed, until the retirement is finished or given up: {}",
        cannot.why,
        way_out(wiki, key, cannot.curable)
    ))
}

#[cfg(test)]
mod tests;
