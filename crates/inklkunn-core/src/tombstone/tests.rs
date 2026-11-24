//! The reader and the acts over the memory stores. (The surfaces' own tests are the web crate's.)
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{
    AccountId, Actor, HiddenItem, HiddenItemKind, HiddenParts, ReasonCategory, Role, TombstoneAct,
    TombstoneActId, TombstoneEvent, TombstoneEventId, WikiId,
};

use super::{Hidden, Mark};

fn event(act: TombstoneAct, category: Option<ReasonCategory>) -> TombstoneEvent {
    TombstoneEvent {
        id: TombstoneEventId::new(),
        act_id: TombstoneActId::new(),
        wiki: WikiId::new(),
        at: 1_700_000_000,
        act,
        item: HiddenItem {
            kind: HiddenItemKind::PageRevision,
            id: "b".repeat(64),
            reference: None,
        },
        parts: HiddenParts {
            content: true,
            summary: false,
            author: false,
        },
        category,
        note: None,
        affected: None,
        by: Actor {
            account: AccountId::new(),
            rung: Role::Admin,
        },
        policy_block: None,
        of_event: None,
        appeal: None,
        declined_transition: None,
    }
}

/// **A hide that records no category is a hide** (correction L-8): the store refuses one, so a record that reads as one is damaged,
/// and what it says is hidden stays hidden, with the generic mark. `Mark::of` and `Hidden::of_events` answered *nothing hidden*.
#[test]
fn a_hide_that_records_no_category_is_hidden_with_the_generic_mark_and_never_nothing() {
    let hide = event(TombstoneAct::Hide, None);
    let mark = Mark::of(&hide).expect("a hide is a mark");
    assert_eq!(mark.category, ReasonCategory::Other);
    assert_eq!(mark.event, hide.id);
    assert!(mark.parts.content);
    let hidden = Hidden::of_events(std::slice::from_ref(&hide));
    assert!(hidden.is_hidden());
    assert!(hidden.content().is_some());
}

/// The mark of a hide that has its category is that category, and an unhide is no mark at all.
#[test]
fn a_hide_with_its_category_keeps_it_and_an_unhide_is_no_mark() {
    let hide = event(TombstoneAct::Hide, Some(ReasonCategory::Spam));
    assert_eq!(
        Mark::of(&hide).map(|m| m.category),
        Some(ReasonCategory::Spam)
    );
    assert!(Mark::of(&event(TombstoneAct::Unhide, None)).is_none());
}
