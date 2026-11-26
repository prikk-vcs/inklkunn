//! What the policy page and the read API read of the rules of a wiki, once: the rules in force, the platform's limits, the
//! protections in force, the approvals warning and, for the page, the changes of the rules.
//!
//! **Nothing is decided here and nothing is a file.** Every value comes from a reader of the core, which reads the repository
//! as the loader does and asks the stores as `authorize()` does. A part that could not be read now is `None`, and says so on
//! the surface that draws it: the rest of the page is whole.

use std::sync::Arc;

use inklkunn_core::Principal;
use inklkunn_core::governance::{ApprovalsWarning, Interwiki, PlatformLimits, Rules};
use inklkunn_core::protection::Notice;
use inklkunn_core::read::RuleHistory;
use inklkunn_store::WikiRecord;

use crate::services::Services;

/// The rules of one wiki as a reader sees them.
pub(crate) struct RulesData {
    /// The rules in force at the default view's tip: the policy (as read under the limits), the constitution, the block that
    /// set them, what a limit lowered and the fault that makes the wiki refuse writes.
    pub(crate) rules: Arc<Rules>,
    /// What the operator allows any wiki.
    pub(crate) limits: PlatformLimits,
    /// The operator's review interval, which a policy with none of its own has.
    pub(crate) server_days: u32,
    /// Every protection in force, the policy's and the temporary ones; `None` when they could not be read now.
    pub(crate) notices: Option<Vec<Notice>>,
    /// The approvals warning (RFC 006 D-4), when the rules ask for more than the people who could give them.
    pub(crate) warning: Option<ApprovalsWarning>,
    /// The changes of the rules; `None` when they could not be read now, and not asked for by the API.
    pub(crate) history: Option<RuleHistory>,
    /// The interwiki map of the default view's tip (RFC 007 D-8); `None` when it could not be read now.
    pub(crate) interwiki: Option<Interwiki>,
}

/// Read the rules of `wiki` for `principal`, who may read it (the caller has opened the wiki). `with_history` asks for the
/// changes of the rules too, which costs a walk of the view's newest changes. `Err` is *the rules could not be read now*, with the cause for the operator's log.
pub(crate) fn gather(
    services: &Services,
    wiki: &WikiRecord,
    principal: &Principal,
    with_history: bool,
) -> Result<RulesData, String> {
    // The rules are read where `authorize()` reads them: one source, so that what the page says is what decides.
    let rules = services
        .authorizer
        .rules(&wiki.id)
        .map_err(|e| e.to_string())?;
    let notices = services
        .authorizer
        .protections_in_force(principal, &wiki.id);
    let warning = services
        .proposals
        .rules_approvals_warning(wiki)
        .ok()
        .flatten();
    let history = if with_history {
        match services.read.rule_history(wiki) {
            Ok(history) => Some(history),
            Err(error) => {
                (services.log)(&format!(
                    "the changes of the rules of wiki `{}` could not be read now: {error}",
                    wiki.slug
                ));
                None
            }
        }
    } else {
        None
    };
    let interwiki = match services.read.interwiki_of(wiki) {
        Ok(map) => Some(map),
        Err(error) => {
            (services.log)(&format!(
                "the interwiki map of wiki `{}` could not be read now: {error}",
                wiki.slug
            ));
            None
        }
    };
    Ok(RulesData {
        rules,
        limits: *services.read.platform_limits(),
        server_days: services.read.review_after_days(),
        notices,
        warning,
        history,
        interwiki,
    })
}
