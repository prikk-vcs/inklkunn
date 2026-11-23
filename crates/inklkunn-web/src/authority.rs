//! **Every decision page shows the rung and the policy block** (RFC 006 D-5, acceptance criterion 12): *accepted by Name
//! (maintainer), under the rules as of `<change>`*, with the change's own link, or *under the default rules* for a wiki that
//! had no rules of its own.
//!
//! The decision records keep the rung the person held and the block of the rules in force when they decided ([`Mark`] of a
//! moderation act, a review's and an acceptance's record, a restriction's, a temporary protection's); a surface says them with
//! this one phrase, so that one word means one thing on every page. **A name is never added here**: a moderation record names
//! a role, never a person (RFC 006 D-8).
//!
//! [`Mark`]: inklkunn_core::tombstone::Mark

use inklkunn_core::governance::short_block;

use crate::catalog::{Key, Rich, T};

/// The same as plain text, for a page that is only paragraphs (a refusal, a restriction's notice): the change is named by its first
/// characters and not linked.
pub(crate) fn under_text(t: T, block: Option<&str>) -> String {
    match block {
        Some(block) => t
            .msg(Key::AuthorityUnderRules)
            .with("change", short_block(block))
            .text(),
        None => t.get(Key::AuthorityUnderDefaults),
    }
}

/// *Under the rules as of `<the change that set them>`*, linked to that change; *under the default rules* for no block.
pub(crate) fn under(t: T, slug: &str, block: Option<&str>) -> Rich {
    match block {
        Some(block) => t
            .msg(Key::AuthorityUnderRules)
            .link(
                "change",
                short_block(block),
                crate::url::change_path(slug, None, block),
            )
            .html(),
        None => t.msg(Key::AuthorityUnderDefaults).html(),
    }
}
