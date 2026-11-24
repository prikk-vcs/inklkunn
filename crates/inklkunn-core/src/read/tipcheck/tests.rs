//! The check of a change's content against the views' tips (RFC 006 D-8; handoff G-T4; corrections L-2 and L-10): a change that cannot
//! be compared is refused, not allowed, and the tips it is compared with are the repository's **now**, never the held ones. The
//! repository is the read tests' double.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::Operation;
use inklkunn_prikk::storage::PathRef;

use super::super::testing::block_id;
use super::super::writes::tests::{foo, op, rig, shows};
use super::*;

const FOO: &str = "pages/Foo.md";

/// An operation whose path prikk could not resolve: it may have written any file.
fn unresolved() -> Operation {
    Operation {
        kind: "edit-text".to_owned(),
        paths: vec![PathRef {
            path: None,
            unresolved_node_id: Some("n".repeat(8)),
        }],
    }
}

#[test]
fn a_change_with_an_operation_on_no_resolved_path_cannot_be_shown_not_to_be_a_tip() {
    // L-2: the file such an operation wrote is in no list, so no tip's file was compared with it; the change is refused by name,
    // **also when the files that were resolved are not the tip's**.
    let r = rig(&[(FOO, &foo("The page."))]);
    r.w.fake().set(FOO, &foo("Written by the change."));
    r.w.fake()
        .show(2, shows(vec![op("edit-text", &[FOO]), unresolved()]));
    r.w.fake().set(FOO, &foo("Rewritten since.")); // 3: the tip's file is no longer the change's
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Unresolved
    );
    // A change with nothing else but an unresolved operation is no different (it has no path to compare at all).
    r.w.fake().set(FOO, &foo("Another."));
    r.w.fake().show(4, shows(vec![unresolved()]));
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(4)).unwrap(),
        TipCheck::Unresolved
    );
    // The change that is all resolved and is not a tip's is clear, as it was.
    r.w.fake().show(3, shows(vec![op("edit-text", &[FOO])]));
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(3)).unwrap(),
        TipCheck::Clear
    );
    // What the change wrote at the paths that could be read is still told (the reveal and the record of a hide).
    let paths = r.service.written_paths_of(&r.w.wiki, &block_id(2)).unwrap();
    assert_eq!(paths, Some(vec![FOO.to_owned()]));
}

#[test]
fn the_check_compares_with_the_tips_as_the_repository_has_them_now_not_as_they_were_held() {
    // L-10: a seal by another process moves a view after this one read it and holds the answer (here for an hour). A change that has
    // just become the tip must still be refused, whatever the held tip says.
    let r = rig(&[(FOO, &foo("The page."))]);
    let held = r.service.tip(&r.w.wiki, None).expect("a tip");
    assert_eq!(held.block_id(), block_id(1));
    // Another process seals the change whose text this check is about; this service has not been told.
    r.w.fake().set(FOO, &foo("The text to be hidden."));
    r.w.fake().show(2, shows(vec![op("edit-text", &[FOO])]));
    assert_eq!(
        r.service.tip(&r.w.wiki, None).expect("a tip").block_id(),
        block_id(1),
        "the service still holds the old tip (the hold is what this test is about)"
    );
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Holds {
            view: "main".to_owned()
        },
        "the change is the tip now: it may not be hidden"
    );
    // And the check has left the service with the current tip, not the old one.
    assert_eq!(
        r.service.tip(&r.w.wiki, None).expect("a tip").block_id(),
        block_id(2)
    );
}
