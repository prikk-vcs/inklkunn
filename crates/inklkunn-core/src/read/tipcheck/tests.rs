//! The check of a change's content against the views' tips (RFC 006 D-8; handoff G-T4; corrections L-2 and L-10): a change that cannot
//! be compared is refused, not allowed, and the tips it is compared with are the repository's **now**, never the held ones. The
//! repository is the read tests' double.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{Operation, StorageError};

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

/// A refusal that says nothing of a file (a damaged store): not *not found*.
fn damaged() -> StorageError {
    StorageError::Refused {
        verb: "cat",
        message: "the store is damaged".to_owned(),
        kind: None,
        reason: None,
    }
}

#[test]
fn a_refusal_that_is_not_not_found_leaves_the_tip_check_unchecked_never_clear() {
    // P-1 (a): the tip's file is not at its path, and the `stat` that tells why refuses with something that is not *not found*. The
    // file may be there, so no tip can be shown not to hold the change's bytes: the hide is refused, not allowed.
    const NEW: &str = "pages/New.md";
    let r = rig(&[(FOO, &foo("The page."))]);
    r.w.fake().set(NEW, &foo("Written by the change."));
    r.w.fake().show(2, shows(vec![op("add-file", &[NEW])]));
    r.w.fake().change_files(&[], &[NEW]); // 3: the tip has no such file
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Clear,
        "the file is not at the tip, and prikk says so: nothing holds the change"
    );
    r.w.fake().fail_stat(Some(damaged()));
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Unchecked
    );
}

#[test]
fn a_refusal_that_is_not_not_found_leaves_the_parents_file_unchecked_too() {
    // P-1 (a): the same where the file the lines are counted against (the change's parent's) cannot be told from a file that is
    // not there. The tip holds the path with other bytes, so only the count asks.
    const NEW: &str = "pages/New.md";
    let r = rig(&[(FOO, &foo("The page."))]);
    r.w.fake().set(NEW, &foo("Written by the change."));
    r.w.fake().show(2, shows(vec![op("add-file", &[NEW])]));
    r.w.fake().set(NEW, &foo("Rewritten since.")); // 3: the tip holds the path, with other bytes
    assert!(matches!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Survives { .. } | TipCheck::Clear
    ));
    r.w.fake().fail_stat(Some(damaged()));
    assert_eq!(
        r.service.tip_check(&r.w.wiki, &block_id(2)).unwrap(),
        TipCheck::Unchecked
    );
}
