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

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use inklkunn_signer::PreparedFile;
use inklkunn_store::{AccountId, DecisionKind, ProposalRecord, Role, WikiRecord};

use super::{AcceptFailure, ProposalAcceptor};
use crate::authorize::Principal;
use crate::governance::{
    Fault, GovernanceChange, GovernanceFile, Requirement, Rules, ViewPlace, is_governance_path,
    mixes_rules_with_other_paths, refuse_outside_default_view, refuse_unknown_files,
    validate_constitution_bytes, validate_policy_bytes,
};
use crate::proposal::{Invalid, ProposalError};

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

/// The kinds of change the touched paths are to governance files (empty when none is).
fn kinds_of(touched: &[String]) -> BTreeSet<GovernanceChange> {
    touched
        .iter()
        .filter_map(|p| GovernanceChange::of_path(p))
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
        if let Some(fault) = rules.fault() {
            let only_governance =
                !touched.is_empty() && touched.iter().all(|p| GovernanceFile::of_path(p).is_some());
            if !(hatch && only_governance) {
                return Err(AcceptFailure::RulesUnreadable(fault.clone()));
            }
        }
        let policy = rules.policy();
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

    /// How many people other than the proposer approved the proposal's latest revision and still hold the review rung under
    /// the rules in force now (RFC 004 D-3: a decision is bound to the revision it was made on, and a later revision
    /// starts the count again).
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
        Ok(reviewers
            .iter()
            .filter(|reviewer| proposals.holds_review(reviewer, wiki, &proposal.id, rules))
            .count())
    }

    /// Judge the governance files the change is about to write (RFC 006 D-4), before the signer is asked: a proposal in
    /// another view that touches `governance/`, a path under it that is neither of the two files, and each file's text (the
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
        faults.extend(refuse_unknown_files(touched()).err());
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
