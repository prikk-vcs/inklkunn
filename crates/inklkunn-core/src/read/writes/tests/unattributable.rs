//! *Not yet* and *never* (I-T1 correction X-1): what a caller that was answered *not checked yet* asks to tell the two apart. The reason
//! a text can never be attributed is read from the index alone ([`ReadService::unattributable_reason`]); how a build went is
//! [`ReadService::warm_writes_progress`]; and the moderation log's head is [`ReadService::moderation_head`].
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use inklkunn_store::{BlockWrites, MAX_MAYBE_WRITERS, PathWrite, WriteKind, WritesState};

use super::*;
use crate::read::{Unattributable, Warmed};

fn row(path: &str, kind: WriteKind) -> PathWrite {
    PathWrite {
        path: path.to_owned(),
        kind,
    }
}

fn block(n: u64, writes: Vec<PathWrite>) -> BlockWrites {
    BlockWrites {
        number: n,
        block: block_id(n),
        proposal: None,
        writes,
    }
}

fn state(head: u64, genesis: bool) -> WritesState {
    WritesState {
        head_number: head,
        head_block: block_id(head),
        floor_number: 1,
        genesis,
    }
}

fn reason(r: &Rig, n: u64, path: &str) -> Option<Unattributable> {
    r.service
        .unattributable_reason(&r.w.wiki, None, &block_id(n), path)
        .expect("an answer")
}

#[test]
fn a_block_whose_effect_is_not_listed_is_the_reason_for_the_text_that_passes_through_it_and_for_no_other()
 {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.append_by_hand(
        &[
            block(
                1,
                vec![
                    row("pages/Foo.md", WriteKind::Wrote),
                    row("pages/Bar.md", WriteKind::Wrote),
                ],
            ),
            block(2, vec![row("", WriteKind::Opaque)]),
            block(3, vec![row("pages/Bar.md", WriteKind::Wrote)]),
        ],
        &state(3, true),
    );
    // Nothing is hidden: every text is attributed, and nothing is asked of the index.
    assert_eq!(reason(&r, 2, "pages/Foo.md"), None);
    r.hide(3);
    assert_eq!(
        reason(&r, 2, "pages/Foo.md"),
        Some(Unattributable::OpaqueBlock)
    );
    assert_eq!(
        reason(&r, 3, "pages/Foo.md"),
        Some(Unattributable::OpaqueBlock)
    );
    // The text before the opaque block, and a text a later change wrote, have none.
    assert_eq!(reason(&r, 1, "pages/Foo.md"), None);
    assert_eq!(reason(&r, 3, "pages/Bar.md"), None);
    // The word a bundle says it with.
    assert_eq!(Unattributable::OpaqueBlock.as_str(), "opaque-block");
}

#[test]
fn more_possible_writers_than_the_index_lists_is_a_reason_of_its_own() {
    let r = rig(&[("pages/Foo.md", &foo("x"))]);
    let maybes = u64::try_from(MAX_MAYBE_WRITERS).unwrap() + 2;
    let mut blocks = vec![block(1, vec![row("pages/Foo.md", WriteKind::Wrote)])];
    for n in 2..2 + maybes {
        blocks.push(block(n, vec![row("", WriteKind::Maybe)]));
    }
    r.append_by_hand(&blocks, &state(1 + maybes, true));
    r.hide(1);
    assert_eq!(
        reason(&r, 1 + maybes, "pages/Foo.md"),
        Some(Unattributable::TooManyWriters)
    );
    // A few possible writers are counted with the one before them, and are no reason.
    assert_eq!(reason(&r, 3, "pages/Foo.md"), None);
    assert_eq!(Unattributable::TooManyWriters.as_str(), "too-many-writers");
}

#[test]
fn a_text_moved_more_often_than_the_renames_followed_is_a_reason_of_its_own() {
    let r = rig(&[("pages/Foo.md", &foo("x"))]);
    let hops = u64::try_from(crate::read::MAX_RENAME_HOPS).unwrap() + 2;
    let path = |n: u64| format!("pages/P{n}.md");
    let mut blocks = vec![block(1, vec![row(&path(1), WriteKind::Wrote)])];
    for n in 2..2 + hops {
        blocks.push(block(
            n,
            vec![row(&path(n), WriteKind::Carried { from: path(n - 1) })],
        ));
    }
    r.append_by_hand(&blocks, &state(1 + hops, true));
    r.hide(1);
    assert_eq!(
        reason(&r, 1 + hops, &path(1 + hops)),
        Some(Unattributable::TooManyRenames)
    );
    // A text moved a few times is followed to the change that wrote it, and that one is hidden, which is not a reason.
    assert_eq!(reason(&r, 3, &path(3)), None);
    assert_eq!(Unattributable::TooManyRenames.as_str(), "too-many-renames");
}

#[test]
fn a_path_nobody_is_recorded_as_having_written_is_a_reason_only_where_the_index_does_not_begin_at_the_first_block()
 {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.append_by_hand(
        &[block(2, vec![row("pages/Bar.md", WriteKind::Wrote)])],
        &state(2, false),
    );
    r.hide(2);
    // The index begins after the first block: Foo was written, if at all, before what it knows.
    assert_eq!(
        reason(&r, 2, "pages/Foo.md"),
        Some(Unattributable::NoWriterKnown)
    );
    assert_eq!(Unattributable::NoWriterKnown.as_str(), "no-writer-known");
    // A block the index does not hold is no answer: the index may not have reached it yet.
    assert_eq!(reason(&r, 9, "pages/Foo.md"), None);
}

#[test]
fn a_build_says_whether_it_reached_the_tip_and_whether_it_moved() {
    // Ten edits and a hidden secret; a request builds one block at a time when its budget is nothing and its clock ticks.
    let r = super::background::long(10, |s| {
        s.with_writes_budget(Duration::ZERO, ticking(Duration::from_secs(1)))
    });
    let mut moved = 0;
    for _ in 0..100 {
        match r.service.warm_writes_progress(&r.w.wiki).expect("a build") {
            Warmed::AtTip => break,
            Warmed::Short { advanced } => {
                assert!(advanced, "an index that is short moves a block at a time");
                moved += 1;
            }
        }
    }
    assert!(moved >= 2, "it took several calls: {moved}");
    assert_eq!(
        r.service.warm_writes_progress(&r.w.wiki).expect("a build"),
        Warmed::AtTip,
        "a built index is at the tip, and stays so"
    );
}

#[test]
fn the_head_of_the_moderation_log_moves_with_every_act_and_a_service_without_the_log_cannot_say() {
    let r = rig(&[("pages/Foo.md", &foo("x"))]);
    assert_eq!(r.service.moderation_head(&r.w.wiki).expect("a head"), None);
    let first = r.hide(1);
    let after_hide = r
        .service
        .moderation_head(&r.w.wiki)
        .expect("a head")
        .expect("an event");
    assert_eq!(after_hide, first.to_string());
    r.unhide(&first, 1);
    let after_unhide = r
        .service
        .moderation_head(&r.w.wiki)
        .expect("a head")
        .expect("an event");
    assert_ne!(after_hide, after_unhide);
    let bare = ReadService::new(
        r.w.counting.clone(),
        r.w.store.clone(),
        r.w.store.clone(),
        r.w.layout.clone(),
        config(),
        None,
    );
    assert!(matches!(
        bare.moderation_head(&r.w.wiki),
        Err(ReadError::CouldNotBeReadNow(Cause::ModerationNotConfigured))
    ));
    assert!(matches!(
        bare.unattributable_reason(&r.w.wiki, None, &block_id(1), "pages/Foo.md"),
        Err(ReadError::CouldNotBeReadNow(Cause::ModerationNotConfigured))
    ));
}
