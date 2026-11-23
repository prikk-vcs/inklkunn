//! What a proposal's page says of a **change to the rules** (RFC 006 D-4; handoff G-T7): *what it changes*, as before and after
//! sentences, per rule and never as YAML (*who may accept a proposal: maintainer or above → reviewer or above*); *back to the
//! default rules* for the removal of a file; what accepting it takes (the role and the approvals the rules of the wiki ask of a
//! change to the rules); and, to an admin of the wiki, that the hatch applies to them.
//!
//! The text before the change is read at the base the proposal was written on, with the one reader of a file at a block
//! ([`ReadService::source_at`](inklkunn_core::ReadService::source_at)); the text after it is the revision's own. The approvals
//! warning is the page's, as for any proposal.

use inklkunn_core::governance::{GovernanceChange, GovernanceFile};
use inklkunn_store::{RevisionContent, Role};

use super::Ctx;
use super::governance::fault_text;
use crate::catalog::{Key, T};
use crate::policy::rules_change::{Compared, file_lines};
use crate::policy::rules_phrases::{change_rules_value, change_who_value};

/// The section of a proposal's page that says a change to the rules.
pub(crate) struct GovernanceView {
    pub(crate) heading: String,
    /// One sentence for each rule the change says differently.
    pub(crate) lines: Vec<String>,
    /// What else the page says: that the file cannot be read as rules, what accepting it takes, the hatch.
    pub(crate) notes: Vec<String>,
}

/// The section for the revision `content`, or `None` when it touches no governance file.
pub(crate) fn view(ctx: &Ctx, t: T, content: &RevisionContent) -> Option<GovernanceView> {
    let touched: Vec<(GovernanceFile, Option<&str>)> = content
        .files
        .iter()
        .filter_map(|f| Some((GovernanceFile::of_path(&f.path)?, f.text.as_deref())))
        .collect();
    let below = content.files.iter().any(|f| {
        GovernanceFile::at_or_below(&f.path).is_some() && GovernanceFile::of_path(&f.path).is_none()
    });
    if touched.is_empty() && !below {
        return None;
    }
    let server_days = ctx.services.read.review_after_days();
    let mut lines = Vec::new();
    let mut notes = Vec::new();
    for (file, after) in &touched {
        let before =
            match ctx
                .services
                .read
                .source_at(&ctx.wiki, &content.base.block_id, file.path())
            {
                Ok(before) => before,
                Err(error) => {
                    ctx.log(&format!(
                        "a governance file at the base of a proposal could not be read now: {error}"
                    ));
                    notes.push(t.get(Key::UnavailableBody));
                    continue;
                }
            };
        let found = match file_lines(t, *file, before.as_deref(), *after, server_days) {
            Compared::Lines(found) => found,
            Compared::Unreadable(fault) => {
                notes.push(
                    t.msg(Key::GovernanceChangeUnreadable)
                        .with("rule", fault_text(t, &fault))
                        .text(),
                );
                continue;
            }
        };
        if after.is_none() {
            lines.push(t.get(Key::GovernanceChangeDefaults));
        }
        if found.is_empty() && after.is_some() {
            lines.push(t.get(Key::GovernanceChangeNone));
        }
        lines.extend(found);
    }
    // What accepting it takes: the rules in force ask a role and approvals of each kind of change.
    if let Ok(rules) = ctx.services.authorizer.rules(&ctx.wiki.id) {
        let kinds: Vec<GovernanceChange> = touched
            .iter()
            .filter_map(|(file, _)| GovernanceChange::of_path(file.path()))
            .collect();
        let floor = rules.policy().acceptance().approvals();
        for kind in [GovernanceChange::Policy, GovernanceChange::Constitution] {
            if kinds.contains(&kind) {
                let rule = match kind {
                    GovernanceChange::Policy => change_rules_value(t, rules.constitution(), floor),
                    GovernanceChange::Constitution => {
                        change_who_value(t, rules.constitution(), floor)
                    }
                };
                notes.push(t.msg(Key::GovernanceNeeds).with("rule", rule).text());
            }
        }
    }
    // The hatch is an admin's, and the record of the acceptance will say it was used.
    let admin = ctx
        .services
        .authorizer
        .holding(&ctx.principal, &ctx.wiki.id)
        .is_ok_and(|h| h.rung == Some(Role::Admin));
    if admin {
        notes.push(t.get(Key::GovernanceHatchYou));
    }
    Some(GovernanceView {
        heading: t.get(Key::GovernanceChangeHeading),
        lines,
        notes,
    })
}
