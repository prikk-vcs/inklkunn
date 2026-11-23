//! What acceptance does with the rules of a wiki (RFC 006 D-1, D-4; handoff G-T1 part C): **the rung and the approvals a
//! proposal needs, the hatch, the repair of a wiki whose governance cannot be read, and the validation of a change to
//! `governance/` before the signer is asked.**
//!
//! Two functions, called by [`ProposalAcceptor`] in the order the acceptance takes its steps:
//!
//! - [`ProposalAcceptor::judge`] *who may accept this*: before the lease, from what the revision writes, as an early refusal,
//!   and again under the lease, from what is about to be signed, which is the one that counts and whose answer (the rung, the
//!   block, the hatch) is written into the acceptance record.
//! - [`ProposalAcceptor::check_governance`] *is what is about to be written sound*: under the lease, after the merge, before
//!   the knowledge files are judged and long before the signer. Each refusal names the file, the rule and, where it can be
//!   said, the line, and never repeats what a file said.
//!
//! **A wiki with no governance files is decided exactly as 0.3.0 decided it**: no approvals are asked for, the rung is the
//! grant, and a maintainer accepts their own proposal only while the wiki has fewer than three maintainers (a test holds
//! this against a frozen copy of 0.3.0's rule).
//!
//! # The hatch (D-4, INV-14)
//!
//! An **admin of the wiki** (the rung, held with a credential that carries the admin scope) may accept a change that touches
//! `governance/` with **no approvals and no rung beyond their own, whatever the constitution and the policy say**, and the
//! acceptance record names it ([`AcceptanceRecord::hatch`]). It is here and not in `authorize()`. It also lifts the rule
//! against accepting one's own proposal (`acceptance.self`) **for a change to `governance/` only**: the sole admin of a wiki
//! whose policy says *an admin accepts* and *never one's own* would otherwise be unable to change the rules, which is the one
//! way such a wiki dies.
//!
//! **A change to the rules is its own proposal.** A proposal that touches `governance/` touches nothing else
//! ([`Invalid::GovernanceMixed`]): it is refused when it is proposed and at each revision, and again at acceptance, before
//! the lease and under it ([`refuse_mixed`]). The hatch checks it for itself too: it covers a change only if **every** path the
//! change touches is under `governance/`, so a page that reached acceptance beside a rules change is judged by the policy's
//! approvals like any page, never by the hatch.
//!
//! # A wiki whose governance cannot be read (D-1, C-3d)
//!
//! Every write refuses, naming the file and the line ([`AcceptFailure::RulesUnreadable`]), **except the repair**: an admin's
//! change that touches nothing but the two governance files, accepted by the hatch under the default policy. A change that
//! touches anything else is refused with the same sentence, so that the files are repaired first.
//!
//! **What the repair may remove** (handoff G-T1 correction D-1): a change writes only the two governance files and may remove
//! any path below `governance/`. In a sound wiki nothing else is there, so this opens nothing. In a wiki where something
//! stands at or below a governance path (a directory where `governance/policy.yaml` belongs, with files in it: the
//! [`Refusal::NotAFile`](crate::governance::Refusal::NotAFile) fault), the admin's repair is the proposal that removes
//! the files below the path and, if the rules are to be set, writes the file: one change, one patch, accepted by the hatch. A
//! removal below a file's place is judged as a change to that file ([`GovernanceChange::at_or_below`]); a removal of a path
//! that is below neither file is judged as no rule at all (it changes none: the ordinary acceptance, for a wiki whose rules
//! can be read; while they cannot, the repair is the two files and what lies below them, and this is refused with the rest),
//! and is still a path under `governance/`, so a page never travels with it (`governance-mixed`).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use inklkunn_signer::PreparedFile;
use inklkunn_store::{AccountId, DecisionKind, ProposalRecord, Role, WikiRecord};

use super::{AcceptFailure, ProposalAcceptor};
use crate::authorize::{Action, Principal};
use crate::governance::{
    Fault, GovernanceChange, GovernanceFile, Requirement, Rules, ViewPlace, is_governance_path,
    mixes_rules_with_other_paths, only_governance_files, refuse_outside_default_view,
    refuse_unknown_files, validate_constitution_bytes, validate_policy_bytes,
};
use crate::proposal::{Invalid, ProposalError};
use crate::protection::{Demand, Lack};

/// How an acceptance was authorized: what the acceptance record says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Judged {
    /// The rung the acceptor held, under the rules in force.
    pub(super) rung: Role,
    /// The block that last changed a governance file, or `None` for the defaults.
    pub(super) policy_block: Option<String>,
    /// An admin's hatch: a change to `governance/` accepted with no approvals.
    pub(super) hatch: bool,
}

/// The kinds of change the touched paths are to governance files (empty when none is). A path **below** one of the two files
/// (a removal of what stands where the file belongs, handoff G-T1 correction D-1) is a change to that file's place and counts
/// as one to the file; any other path below `governance/` is a change to no rule, and is no kind.
fn kinds_of(touched: &[String]) -> BTreeSet<GovernanceChange> {
    touched
        .iter()
        .filter_map(|p| GovernanceChange::at_or_below(p))
        .collect()
}

impl ProposalAcceptor {
    /// Who may accept a change that touches `touched` (repository paths: what it writes, removes or renames to or from)?
    ///
    /// In this order: a wiki whose governance cannot be read refuses everything but the repair; a change to `governance/`
    /// needs the constitution's rung (the hatch excepted); the approvals the policy and the constitution ask for are
    /// counted; and the proposer's own acceptance is judged by `acceptance.self`. Nothing is written.
    ///
    /// **`rules` are the rules the caller judges by**: before the lease, the rules as the service holds them (an advisory
    /// answer); under the lease, **the rules of the default view's state the caller has just read** ([`ProposalAcceptor::rules_now`]),
    /// never the held ones, so that the change is judged by the rules of the state it is signed on (C-2). The rung, the
    /// approvals' rung and the block the record names are all theirs.
    pub(super) fn judge(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        acceptor: &AccountId,
        touched: &[String],
        rules: &Arc<Rules>,
    ) -> Result<Judged, AcceptFailure> {
        let proposals = &self.parts.proposals;
        let standing = proposals.standing_under(principal, wiki, rules)?;
        let rung = standing.rung.ok_or(AcceptFailure::NotAllowed)?;
        let kinds = kinds_of(touched);
        // The hatch is an admin's: the rung, and a credential that carries it (a token of a narrower scope does not).
        let admin = rung >= Role::Admin
            && matches!(principal, Principal::Account(a) if a.scope() >= inklkunn_store::Scope::Admin);
        // A second guard beside `check_governance`'s (C-1): the hatch is for a change to the rules and for nothing else, so a
        // change that also touches anything outside `governance/` is not the hatch, whatever reached here.
        let rules_only = !touched.is_empty() && touched.iter().all(|p| is_governance_path(p));
        let hatch = admin && !kinds.is_empty() && rules_only;
        if let Some(fault) = rules.fault()
            && !(hatch && only_governance_files(touched.iter().map(String::as_str)))
        {
            return Err(AcceptFailure::RulesUnreadable(fault.clone()));
        }
        let policy = rules.policy();
        // **The acceptor must still hold the rung that accepts** (F-1): the early authorization asked it before the lease, and a
        // restriction or a grant may have changed since, so under the lease it is asked again, of the rung read now under the
        // rules of the state the change is signed on. It comes before the protections' check, so that each refusal names its own
        // cause: a restriction says so, with its reason and its end; any other lack is *not allowed*. The hatch is an admin's
        // and an admin holds every rung.
        if !hatch && rung < policy.minimum_role(Action::Accept) {
            return Err(proposals
                .accept_restriction_under(principal, wiki, &proposal.id, rules)
                .map_or(AcceptFailure::NotAllowed, AcceptFailure::Restricted));
        }
        let required: Option<Requirement> = kinds
            .iter()
            .map(|k| k.required(rules.constitution()))
            .reduce(|a, b| Requirement {
                rung: a.rung.max(b.rung),
                approvals: a.approvals.max(b.approvals),
            });
        if let Some(required) = required
            && !hatch
            && rung < required.rung
        {
            return Err(AcceptFailure::GovernanceRung(required.rung));
        }
        let needed = if hatch {
            0
        } else {
            policy
                .acceptance()
                .approvals()
                .max(required.map_or(0, |r| r.approvals))
        };
        // The protections of the pages the change touches (RFC 006 D-7), standing and temporary, read now with the rules of the
        // state it is signed on and the clock of this act, never cached: the stricter of them decides. A change to the rules
        // touches no page, so the hatch meets none.
        let notices = proposals.protections_for_change(wiki, touched, rules, (self.clock)())?;
        let demand = Demand::of(&notices);
        if !demand.rung_met(Some(rung)) {
            let asked = demand.rung.unwrap_or(rung);
            return Err(AcceptFailure::Protected {
                notices,
                lack: Lack::Rung(asked),
            });
        }
        // One approval is the least a review asks, **whatever `acceptance.approvals` says**: the greater of the two applies,
        // so the policy's own count (below) stands when it asks for one or more.
        if demand.review && needed == 0 && self.approvals(principal, wiki, proposal, rules)? == 0 {
            return Err(AcceptFailure::Protected {
                notices,
                lack: Lack::Review,
            });
        }
        if needed > 0 {
            let have = self.approvals(principal, wiki, proposal, rules)?;
            if have < usize::from(needed) {
                return Err(AcceptFailure::ApprovalsMissing { needed, have });
            }
        }
        if proposal.author == *acceptor && !hatch {
            let maintainers = self
                .parts
                .roles
                .grants_in(&wiki.id)?
                .iter()
                .filter(|g| g.role >= Role::Maintainer)
                .count();
            if !policy.acceptance().may_self_accept(maintainers) {
                return Err(AcceptFailure::NotAllowed);
            }
        }
        Ok(Judged {
            rung,
            policy_block: rules.block().map(str::to_owned),
            hatch,
        })
    }

    /// How many people other than the proposer approved the proposal's latest revision, **have an account that is not
    /// disabled** and still hold the review rung under the rules in force now (RFC 004 D-3: a decision is bound to the
    /// revision it was made on, and a later revision starts the count again).
    fn approvals(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        rules: &Arc<Rules>,
    ) -> Result<usize, AcceptFailure> {
        let proposals = &self.parts.proposals;
        let latest = proposals
            .revisions(principal, wiki, &proposal.id)?
            .last()
            .map(|r| r.seq)
            .ok_or(AcceptFailure::NotFound)?;
        let reviewers: BTreeSet<AccountId> = proposals
            .decisions(principal, wiki, &proposal.id)?
            .into_iter()
            .filter(|d| {
                d.kind == DecisionKind::Approve
                    && d.revision == latest
                    && d.reviewer != proposal.author
            })
            .map(|d| d.reviewer)
            .collect();
        let mut counted = 0;
        for reviewer in &reviewers {
            // A disabled account's approval stays recorded and stops counting (C-4): an operator who disables a reviewer they no
            // longer trust expects what that reviewer approved to stop being enough. An account the store does not know counts
            // for nothing either.
            let enabled = self
                .parts
                .accounts
                .account(reviewer)?
                .is_some_and(|account| !account.disabled);
            if enabled && proposals.holds_review(reviewer, wiki, &proposal.id, rules) {
                counted += 1;
            }
        }
        Ok(counted)
    }

    /// Judge the governance files the change is about to write (RFC 006 D-4), before the signer is asked: a proposal in
    /// another view that touches `governance/`, a path under it that is neither of the two files and is written (a removal of any path below it is allowed, D-1), and each file's text (the
    /// schema, the ordering rule and the platform limits). Nothing is written, the repository is untouched and the proposal
    /// stays as it is. A change that touches no governance file is not looked at.
    pub(super) fn check_governance(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalRecord,
        files: &[PreparedFile],
        renames: &[(PathBuf, PathBuf)],
    ) -> Result<(), AcceptFailure> {
        let paths = touched_paths(files, renames);
        if !paths.iter().any(|p| is_governance_path(p)) {
            return Ok(());
        }
        refuse_mixed(&paths)?;
        let place = if proposal.view == wiki.default_view {
            ViewPlace::Default
        } else {
            ViewPlace::Alternative
        };
        let mut faults: Vec<Fault> = Vec::new();
        let touched = || paths.iter().map(String::as_str);
        faults.extend(refuse_outside_default_view(touched(), place).err());
        // The writes only (D-1): a removal of any path below `governance/` is allowed, and the rest of this function looks only
        // at the two files' text.
        let writes = files
            .iter()
            .filter(|f| f.content.is_some())
            .map(|f| f.path.to_string_lossy().into_owned());
        let renamed = renames
            .iter()
            .flat_map(|(a, b)| [a, b])
            .map(|p| p.to_string_lossy().into_owned());
        let written: Vec<String> = writes.chain(renamed).collect();
        faults.extend(refuse_unknown_files(written.iter().map(String::as_str)).err());
        let limits = self.parts.reads.platform_limits();
        for file in files {
            let Some(path) = file.path.to_str() else {
                continue;
            };
            let (Some(governance), Some(bytes)) = (GovernanceFile::of_path(path), &file.content)
            else {
                continue;
            };
            let verdict = match governance {
                GovernanceFile::Policy => validate_policy_bytes(bytes, limits).map(drop),
                GovernanceFile::Constitution => validate_constitution_bytes(bytes).map(drop),
            };
            faults.extend(verdict.err());
        }
        if faults.is_empty() {
            Ok(())
        } else {
            Err(AcceptFailure::GovernanceInvalid(faults))
        }
    }
}

/// A change to the rules touches nothing else (C-1): the refusal made when it was proposed, made again at acceptance for a
/// proposal made before the rule, or one that reached the store another way. Asked before the lease, from what the revision
/// writes, and again under it, from what is about to be signed.
pub(super) fn refuse_mixed(touched: &[String]) -> Result<(), AcceptFailure> {
    if mixes_rules_with_other_paths(touched.iter().map(String::as_str)) {
        return Err(AcceptFailure::Proposal(ProposalError::Invalid(
            Invalid::GovernanceMixed,
        )));
    }
    Ok(())
}

/// Every repository path a change writes, removes or renames to or from.
pub(super) fn touched_paths(files: &[PreparedFile], renames: &[(PathBuf, PathBuf)]) -> Vec<String> {
    files
        .iter()
        .map(|f| &f.path)
        .chain(renames.iter().flat_map(|(a, b)| [a, b]))
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}
