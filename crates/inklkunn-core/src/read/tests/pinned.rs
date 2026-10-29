//! Reads are pinned to the tip's block (handoff 003, the notes before T4): `branch list` names a state, one
//! `log` names the block it belongs to, and the page and the index are read *at the block*, so a write
//! between two calls of one request cannot put newer content under an older key.

use super::super::testing::{World, page};
use super::*;

fn two_versions() -> World {
    World::pages(&[("Home", &[], "Old text.\n"), ("Guide", &[], "Guide.\n")])
}

#[test]
fn a_view_that_moves_after_its_tip_was_held_is_still_read_at_the_held_block() {
    let w = two_versions();
    let first = read_page(&w, "Guide");
    assert!(first.html.as_str().contains("Guide."));
    // The view moves while the tip is held. Home was not read yet: if it were read at the *view*, the new
    // text would be cached under the old tip's key.
    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "New text.\n"));
    let home = read_page(&w, "Home");
    assert!(home.html.as_str().contains("Old text."), "{:?}", home.html);
    assert!(!home.html.as_str().contains("New text."));
    assert_eq!(home.tip.block_id, first.tip.block_id);
    assert_eq!(home.tip.state_id, first.tip.state_id);
    // Once the hold ends the reader sees the new tip, whole.
    w.service.forget_wiki(&w.wiki);
    let now = read_page(&w, "Home");
    assert!(now.html.as_str().contains("New text."));
    assert_ne!(now.tip.state_id, first.tip.state_id);
    assert_eq!(now.tip.block_id, w.fake().block_id());
}

#[test]
fn a_view_that_moves_between_the_list_and_the_log_is_the_newer_tip_whole() {
    let w = two_versions();
    let before = w.fake().state_id();
    // The list answers with the old state, and the view moves right after it answered.
    w.fake()
        .move_after_next_views("pages/Home.md", &page(&["title: Home"], "New text.\n"));
    let home = read_page(&w, "Home");
    assert_ne!(home.tip.state_id, before, "the log named the newer state");
    assert_eq!(home.tip.state_id, w.fake().state_id());
    assert_eq!(home.tip.block_id, w.fake().block_id());
    assert!(home.html.as_str().contains("New text."), "{:?}", home.html);
    // And the cache of that tip is kept, not pruned as a past one: the next read is a hit, with no extract.
    let (again, calls) = w.counted(|| read_page(&w, "Home"));
    assert!(again.cached);
    assert_eq!(calls.materialize, 0);
    let (_, guide) = w.counted(|| read_page(&w, "Guide"));
    assert_eq!(guide.materialize, 0, "the index of the newer tip is there");
}

#[test]
fn the_title_index_is_extracted_at_the_block_not_at_the_view() {
    let w = two_versions();
    let tip = w.service.tip(&w.wiki, None).expect("tip");
    // The view moves after the tip was taken and before the index is built: the index must be the tip's.
    w.fake().set(
        "pages/Late.md",
        &page(&["title: Late"], "Written after the tip.\n"),
    );
    let index = w.service.title_index(&w.wiki, None).expect("index");
    assert_eq!(index.tip(), tip.state_id);
    assert!(
        matches!(index.lookup("Late"), Lookup::Missing),
        "a page written after the tip is not in the tip's index"
    );
    assert!(matches!(index.lookup("Home"), Lookup::Found(_)));
}
