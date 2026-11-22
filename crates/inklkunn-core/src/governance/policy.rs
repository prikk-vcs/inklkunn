//! The policy: who may do what in one wiki (RFC 006 D-2), as a value that cannot be wrong.
//!
//! A [`Policy`] is built in two ways only: [`Policy::default`] (0.3.0's behaviour) and by reading a file
//! ([`read_policy`](super::read_policy), [`validate_policy`](super::validate_policy)), which refuses a file whose
//! schema, ranges, ordering rule or limits are broken. So every `Policy` in the program obeys the ordering rule and
//! the ranges, and the code that decides with it never has to check them again.

use std::collections::BTreeMap;

use inklkunn_store::Role;

use super::chars::quoted;
use super::defaults;
use super::protect::{Protection, Require, Target};
use super::{Acceptance, Entry, Rungs};
use crate::authorize::Action;

/// The rules of one wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub(super) entry: Entry,
    pub(super) rungs: Rungs,
    pub(super) acceptance: Acceptance,
    pub(super) review_after: Option<u32>,
    pub(super) promotion: BTreeMap<Role, String>,
    pub(super) protect: Vec<Protection>,
}

impl Default for Policy {
    /// **0.3.0's behaviour**, key by key ([`defaults`]): no entry rung; comment from a questioner, propose from a
    /// contributor, review and create a view from a reviewer, accept and moderate from a maintainer, an appeal decided
    /// by an admin; a maintainer grants nothing; no approvals; a maintainer may accept their own proposal in a wiki
    /// with fewer than three maintainers; the operator's review interval; no promotion text; no standing protection.
    /// This is the policy of a wiki with no `governance/policy.yaml`.
    fn default() -> Self {
        Policy {
            entry: defaults::ENTRY,
            rungs: Rungs::default(),
            acceptance: Acceptance::default(),
            review_after: defaults::REVIEW_AFTER_DAYS,
            promotion: BTreeMap::new(),
            protect: Vec::new(),
        }
    }
}

impl Policy {
    /// `entry`: the rung every signed-in account holds without a grant.
    pub fn entry(&self) -> Entry {
        self.entry
    }

    /// `rungs.*`.
    pub fn rungs(&self) -> &Rungs {
        &self.rungs
    }

    /// `acceptance.*`.
    pub fn acceptance(&self) -> &Acceptance {
        &self.acceptance
    }

    /// The lowest rung that may do `action` in this wiki. **The admin actions** (admin, export, import) **are always
    /// `admin`, and reading is always `observer`**: no policy changes either (RFC 006 D-2). The other six are the
    /// policy's `rungs.*`, which for [`Policy::default`] are 0.3.0's table.
    pub fn minimum_role(&self, action: Action) -> Role {
        match action {
            Action::Read => Role::Observer,
            Action::Comment => self.rungs.comment,
            Action::Propose => self.rungs.propose,
            Action::Review => self.rungs.review,
            Action::CreateView => self.rungs.create_view,
            Action::Accept => self.rungs.accept,
            Action::Moderate => self.rungs.moderate,
            Action::Admin | Action::Export | Action::Import => Role::Admin,
        }
    }

    /// `knowledge.review_after` in days: the wiki's own, or `operator_default` (the operator's
    /// `INKLKUNN_KNOWLEDGE_REVIEW_AFTER_DAYS`) when the policy does not say (RFC 006 D-14).
    pub fn review_after_days(&self, operator_default: u32) -> u32 {
        self.review_after.unwrap_or(operator_default)
    }

    /// The wiki's own `knowledge.review_after`, when the policy sets one.
    pub fn review_after(&self) -> Option<u32> {
        self.review_after
    }

    /// The promotion text of a rung: what this wiki looks for before granting it. **Guidance for people**: nothing
    /// promotes by it, no count turns into a rung, and there is no score (RFC 006 D-2; C-12, GV-2).
    pub fn promotion(&self, rung: Role) -> Option<&str> {
        self.promotion.get(&rung).map(String::as_str)
    }

    /// Every promotion text, lowest rung first.
    pub fn promotions(&self) -> impl Iterator<Item = (Role, &str)> {
        self.promotion.iter().map(|(r, t)| (*r, t.as_str()))
    }

    /// The standing protections, in the order the file lists them.
    pub fn protections(&self) -> &[Protection] {
        &self.protect
    }

    /// The standing protections that cover the page with this title.
    pub fn protections_of_title<'a>(
        &'a self,
        title: &'a str,
    ) -> impl Iterator<Item = &'a Protection> {
        self.protect.iter().filter(move |p| p.covers_title(title))
    }

    /// The standing protections that cover the page at this repository path (`pages/Help/x.md`).
    pub fn protections_of_path<'a>(
        &'a self,
        path: &'a str,
    ) -> impl Iterator<Item = &'a Protection> {
        self.protect.iter().filter(move |p| p.covers_path(path))
    }

    /// The canonical text of the policy: **every key the schema has**, in the schema's order, each text
    /// double-quoted (a key left out of a file is written here with its default, except `knowledge.review_after`,
    /// `promotion` and `protect`, which are written only when set). Reading it back gives the policy again.
    pub fn to_yaml(&self) -> String {
        let r = &self.rungs;
        let mut out = String::new();
        out.push_str(&format!("entry: {}\n", self.entry.word()));
        out.push_str("rungs:\n");
        let rung = |out: &mut String, key: &str, role: Role| {
            out.push_str(&format!("  {key}: {}\n", role.as_str()));
        };
        rung(&mut out, "comment", r.comment);
        rung(&mut out, "propose", r.propose);
        rung(&mut out, "review", r.review);
        rung(&mut out, "create_view", r.create_view);
        rung(&mut out, "accept", r.accept);
        rung(&mut out, "moderate", r.moderate);
        out.push_str(&format!("  decide_appeal: {}\n", r.decide_appeal.word()));
        out.push_str(&format!("  grant_up_to: {}\n", r.grant_up_to.word()));
        out.push_str("acceptance:\n");
        out.push_str(&format!("  approvals: {}\n", self.acceptance.approvals));
        out.push_str(&format!(
            "  self: {}\n",
            self.acceptance.self_acceptance.word()
        ));
        if let Some(days) = self.review_after {
            out.push_str(&format!("knowledge:\n  review_after: {days}\n"));
        }
        if !self.promotion.is_empty() {
            out.push_str("promotion:\n");
            for (role, text) in &self.promotion {
                out.push_str(&format!("  {}: {}\n", role.as_str(), quoted(text)));
            }
        }
        if !self.protect.is_empty() {
            out.push_str("protect:\n");
            for p in &self.protect {
                let (key, title) = match &p.target {
                    Target::Path(t) => ("path", t),
                    Target::Namespace(t) => ("namespace", t),
                };
                out.push_str(&format!("  - {key}: {}\n", quoted(title)));
                match p.require {
                    Require::Review => out.push_str("    require: review\n"),
                    Require::Rung(role) => {
                        out.push_str(&format!("    require:\n      rung: {}\n", role.as_str()));
                    }
                }
                out.push_str(&format!("    reason: {}\n", quoted(&p.reason)));
            }
        }
        out
    }
}
