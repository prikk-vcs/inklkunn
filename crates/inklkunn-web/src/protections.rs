//! What the pages say of the protections of a page (RFC 006 D-7; handoff G-T3; threat model C-3g): **one display for both kinds**.
//!
//! The page, the proposal form, the proposal's page and every refusal a protection causes show each protection **once**, with
//! its kind (*by this wiki's rules*, or *temporarily, until `<date>`*), what it is on, what it asks, and its reason. Where a
//! standing and a temporary one apply, both are listed; the stricter decides, in `authorize()` and the acceptance, and never
//! here. **This module decides nothing**: it draws the [`Notice`]s the core hands it, and no template evaluates a protection.
//!
//! - [`views`]: the notices as the list the pages print ([`NoticeView`]), in the reader's language.
//! - [`paragraphs`]: the same as sentences, for a page that is only paragraphs (a refusal).
//! - [`lack_note`]: the sentence for what a refused acceptance lacked.
//!
//! The reason is the words of whoever set the protection, printed as text (escaped like any other); it is shown to everyone
//! who may read the page it is on, as the RFC says.

mod fields;
pub(crate) mod form;
pub(crate) mod paths;

use inklkunn_core::governance::{Require, Target};
use inklkunn_core::protection::{Kind, Lack, Notice};
use inklkunn_store::Role;

use crate::catalog::{Key, Rich, T};
use crate::time::utc_minutes;

/// One protection, ready to print: four short sentences, each a whole message.
pub(crate) struct NoticeView {
    /// *By this wiki's rules*, or *temporarily, until `<date>`*.
    pub(crate) kind: String,
    /// What it is on: the page, or every page below a title.
    pub(crate) on: String,
    /// What it asks of whoever accepts a change.
    pub(crate) requires: String,
    /// Why, in the words of whoever set it.
    pub(crate) reason: String,
    /// For a temporary protection: the role of whoever put it in place and the rules they did it under (RFC 006 D-5; no name).
    pub(crate) set_by: Option<Rich>,
}

/// The name of a role, in the reader's language.
pub(crate) fn rung_word(t: T, role: Role) -> String {
    t.get(match role {
        Role::Observer => Key::RoleObserver,
        Role::Questioner => Key::RoleQuestioner,
        Role::Contributor => Key::RoleContributor,
        Role::Reviewer => Key::RoleReviewer,
        Role::Maintainer => Key::RoleMaintainer,
        Role::Admin => Key::RoleAdmin,
    })
}

fn view(t: T, slug: &str, notice: &Notice) -> NoticeView {
    let kind = match notice.kind() {
        Kind::Standing => t.get(Key::ProtectionKindStanding),
        Kind::Temporary { until } => t
            .msg(Key::ProtectionKindTemporary)
            .with("until", utc_minutes(until))
            .text(),
    };
    let on = match notice.target() {
        Target::Path(title) => t.msg(Key::ProtectionOnPage).with("title", title).text(),
        Target::Namespace(title) => t
            .msg(Key::ProtectionOnNamespace)
            .with("title", title)
            .text(),
    };
    let requires = match notice.require() {
        Require::Review => t.get(Key::ProtectionRequiresReview),
        Require::Rung(role) => t
            .msg(Key::ProtectionRequiresRung)
            .with("role", rung_word(t, role))
            .text(),
    };
    NoticeView {
        kind,
        on,
        requires,
        reason: t
            .msg(Key::ProtectionReason)
            .with("reason", notice.reason())
            .text(),
        set_by: notice.set_by().map(|by| {
            t.msg(Key::AuthoritySetBy)
                .with("role", rung_word(t, by.rung))
                .rich(
                    "authority",
                    crate::authority::under(t, slug, by.block.as_deref()),
                )
                .html()
        }),
    }
}

/// The notices as the list a page prints, in the order the core gave them (the policy's first, then the temporary ones).
pub(crate) fn views(t: T, slug: &str, notices: &[Notice]) -> Vec<NoticeView> {
    notices.iter().map(|n| view(t, slug, n)).collect()
}

/// The notices as paragraphs: each protection's four sentences, one paragraph a sentence, in the same order.
pub(crate) fn paragraphs(t: T, notices: &[Notice]) -> Vec<String> {
    views(t, "", notices)
        .into_iter()
        .flat_map(|v| [v.kind, v.on, v.requires, v.reason])
        .collect()
}

/// The sentence for what a refused acceptance lacked: the role, or the approval first.
pub(crate) fn lack_note(t: T, lack: Lack) -> String {
    match lack {
        Lack::Rung(role) => t
            .msg(Key::OutcomeNoteProtectedRung)
            .with("role", rung_word(t, role))
            .text(),
        Lack::Review => t.get(Key::OutcomeNoteProtectedReview),
    }
}
