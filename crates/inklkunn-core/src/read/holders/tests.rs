//! The later revisions that hold the text of a change (RFC 006 D-8; handoff G-T4, correction J-4): what *holds* means (strictly
//! more than half of the lines the change added), what is skipped, what the bounds leave unchecked and say, and that the cost does not
//! grow with the history. The repository is the read tests' double; the moderation records are the memory store's.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::MAX_FOLLOWED_MOVES;
use super::super::testing::block_id;
use super::super::writes::tests::{Rig, foo, op, rig, shows};
use super::*;

const FOO: &str = "pages/Foo.md";
const BAR: &str = "pages/Bar.md";
const BAZ: &str = "pages/Baz.md";

/// A page whose paragraphs are the lines `Line 0.` … of `range`.
fn body(range: std::ops::Range<usize>) -> String {
    foo(&range
        .map(|n| format!("Line {n}."))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// The page with the lines of `range` and one line of its own.
fn body_and(range: std::ops::Range<usize>, own: usize) -> String {
    foo(&range
        .map(|n| format!("Line {n}."))
        .chain([format!("Own line {own}.")])
        .collect::<Vec<_>>()
        .join("\n\n"))
}

fn world() -> Rig {
    rig(&[(FOO, &foo("The page."))])
}

fn holders(r: &Rig, n: u64) -> LaterHolders {
    r.service
        .later_holders(&r.w.wiki, None, &block_id(n))
        .expect("an answer")
}

fn blocks(h: &LaterHolders) -> Vec<String> {
    h.listed.iter().map(|l| l.block.clone()).collect()
}

#[test]
fn a_later_revision_that_keeps_most_of_the_lines_is_listed_and_one_that_keeps_half_or_less_is_not()
{
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2: the change adds six lines
    r.w.fake().set(FOO, &body_and(0..4, 3)); // 3: keeps four of six
    r.w.fake().set(FOO, &body_and(0..3, 4)); // 4: keeps three of six: exactly half is not most
    r.w.fake().set(FOO, &body_and(0..2, 5)); // 5: keeps two
    r.w.fake().set(FOO, &foo("Something unrelated entirely.")); // 6
    r.warm();
    let got = holders(&r, 2);
    assert!(!got.unsettled);
    assert_eq!(blocks(&got), vec![block_id(3)]);
    assert_eq!(got.listed.first().map(|l| l.path.as_str()), Some(FOO));
    assert_eq!(got.more, 0);
    assert_eq!((got.hidden, got.unchecked, got.paths_unchecked), (0, 0, 0));
    assert!(
        got.listed.first().is_some_and(|l| l.first_seen.is_some()),
        "the moment this server first saw it"
    );
    assert!(!got.is_silent());
}

#[test]
fn a_revision_of_another_page_that_has_the_same_lines_is_not_a_later_revision_of_this_one() {
    let r = rig(&[(FOO, &foo("The page.")), (BAR, &foo("Bar."))]);
    r.w.fake().set(FOO, &body(0..6)); // 2
    r.w.fake().set(BAR, &body(0..6)); // 3: a copy on another page
    r.w.fake().set(FOO, &foo("Rewritten.")); // 4
    r.warm();
    let got = holders(&r, 2);
    assert!(got.listed.is_empty(), "{got:?}");
    assert!(got.is_silent(), "nothing to say: {got:?}");
}

#[test]
fn a_change_that_added_no_line_names_nothing_and_a_line_is_trimmed_and_non_blank() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    r.w.fake().set(FOO, &body(0..3)); // 3: removes three lines and adds none
    r.w.fake().set(FOO, &body_and(0..3, 1)); // 4
    r.warm();
    assert!(holders(&r, 3).is_silent());
    // Blank lines and the indentation of a line are not lines the change added: one line is, and a later revision has it whole.
    let r = world();
    r.w.fake().set(FOO, &foo("The page.\n\n   \n\n  Line 0.  "));
    r.w.fake()
        .set(FOO, &foo("The page.\n\n   \n\nLine 0.\n\nLine 1."));
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(blocks(&got), vec![block_id(3)], "{got:?}");
}

#[test]
fn a_page_that_was_only_moved_added_nothing_at_its_new_path() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    // 3: the page moves with its bytes and nothing else; 4: a later revision of the new path keeps the lines.
    r.w.fake()
        .change_files(&[(BAZ.to_owned(), body(0..6))], &[FOO]);
    r.w.fake()
        .show(3, shows(vec![op("rename-path", &[FOO, BAZ])]));
    r.w.fake().set(BAZ, &body_and(0..5, 1)); // 4
    r.warm();
    let got = holders(&r, 3);
    assert!(got.is_silent(), "{got:?}");
}

#[test]
fn more_than_ten_holders_are_ten_and_a_count() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    for own in 0..13 {
        r.w.fake().set(FOO, &body_and(0..6, own)); // 3 … 15
    }
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(got.listed.len(), MAX_HOLDERS_LISTED);
    assert_eq!(got.more, 3);
    assert_eq!(
        blocks(&got),
        (3..13).map(block_id).collect::<Vec<_>>(),
        "oldest first"
    );
    assert_eq!((got.unchecked, got.hidden), (0, 0));
}

#[test]
fn later_revisions_past_the_bound_are_counted_and_said_not_dropped() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    for own in 0..60 {
        r.w.fake().set(FOO, &body_and(0..6, own));
    }
    r.warm();
    let got = holders(&r, 2);
    // Fifty were read and held it (ten listed, forty more); ten were not read.
    assert_eq!(got.listed.len(), MAX_HOLDERS_LISTED);
    assert_eq!(got.more, MAX_HOLDER_CHECKS - MAX_HOLDERS_LISTED);
    assert_eq!(got.unchecked, 10);
    assert!(!got.unchecked_at_least);
    // A history longer than the list of later writers itself: at least.
    let r = world();
    r.w.fake().set(FOO, &body(0..6));
    for own in 0..120 {
        r.w.fake().set(FOO, &body_and(0..6, own));
    }
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(got.unchecked, MAX_LATER_WRITES - MAX_HOLDER_CHECKS);
    assert!(got.unchecked_at_least);
    assert!(!got.is_silent());
}

#[test]
fn paths_past_the_bound_are_counted_and_not_looked_at() {
    let r = world();
    let files: Vec<(String, String)> = (0..25)
        .map(|n| (format!("pages/P{n:02}.md"), body(0..4)))
        .collect();
    r.w.fake().set_files(&files); // 2: twenty-five pages
    // The pages in the order the change's record lists them: the bound is of that order.
    let listed = r
        .service
        .written_paths_of(&r.w.wiki, &block_id(2))
        .expect("paths")
        .expect("a change that is not too large");
    assert_eq!(listed.len(), 25);
    let twentieth = listed
        .get(MAX_HOLDER_PATHS - 1)
        .cloned()
        .expect("a twentieth");
    let last = listed.last().cloned().expect("a last");
    r.w.fake().set(&twentieth, &body_and(0..4, 1)); // 3: the twentieth page, held
    r.w.fake().set(&last, &body_and(0..4, 2)); // 4: the twenty-fifth, past the bound
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(got.paths_unchecked, 25 - MAX_HOLDER_PATHS);
    assert_eq!(blocks(&got), vec![block_id(3)], "{got:?}");
    assert_eq!(
        got.listed.first().map(|l| l.path.as_str()),
        Some(twentieth.as_str())
    );
    assert!(!got.is_silent());
}

#[test]
fn a_later_revision_that_is_hidden_itself_is_skipped_not_listed_and_not_counted_as_holding() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    r.w.fake().set(FOO, &body_and(0..6, 1)); // 3: holds it, and is hidden itself
    r.w.fake().set(FOO, &body_and(0..6, 2)); // 4: holds it
    r.warm();
    assert_eq!(blocks(&holders(&r, 2)), vec![block_id(3), block_id(4)]);
    r.hide(3);
    let got = holders(&r, 2);
    assert_eq!(blocks(&got), vec![block_id(4)]);
    assert_eq!(got.hidden, 1);
    assert_eq!(got.more, 0, "a skipped revision is not counted as a holder");
    assert!(!got.is_silent());
}

#[test]
fn an_index_that_does_not_reach_the_change_says_nothing_and_says_so() {
    let r = world();
    // Block 120 is the first to carry the line of its own, and every block after it keeps it.
    for at in 0..150 {
        // Each of them is a write (another text), and each keeps the three marker lines.
        let text = if at < 118 {
            body(0..6)
        } else {
            foo(&format!(
                "Marker one.\n\nMarker two.\n\nMarker three.\n\nOwn line {at}."
            ))
        };
        r.w.fake().set(FOO, &text);
    }
    // A first request builds one bound of the index from the oldest block, and the change is further on.
    let first = holders(&r, 120);
    assert!(first.unsettled, "{first:?}");
    assert!(first.listed.is_empty());
    assert!(!first.is_silent(), "an unsettled answer is not silence");
    r.warm();
    let later = holders(&r, 120);
    assert!(!later.unsettled);
    assert_eq!(later.listed.len(), MAX_HOLDERS_LISTED, "{later:?}");
    assert_eq!(later.more, 150 + 1 - 120 - MAX_HOLDERS_LISTED);
}

#[test]
fn a_block_of_no_view_is_unsettled() {
    let r = world();
    r.w.fake().set(FOO, &body(0..6));
    r.warm();
    let none = r
        .service
        .later_holders(&r.w.wiki, None, &"d".repeat(64))
        .expect("an answer");
    assert!(none.is_silent() || none.unsettled, "{none:?}");
}

#[test]
fn the_cost_is_the_same_on_a_page_with_sixty_later_revisions_and_with_a_thousand() {
    let mut costs = Vec::new();
    for later in [60_usize, 1000] {
        let r = world();
        r.w.fake().set(FOO, &body(0..6));
        for own in 0..later {
            r.w.fake().set(FOO, &body_and(0..6, own));
        }
        r.warm();
        let (got, calls) = r.w.counted(|| holders(&r, 2));
        assert_eq!(got.listed.len(), MAX_HOLDERS_LISTED, "{later}");
        costs.push(calls);
    }
    let [few, many]: [crate::read::Calls; 2] = costs.try_into().expect("two");
    assert_eq!(few, many, "no call grows with the history");
    assert_eq!(few.history + few.materialize + few.diff, 0, "{few:?}");
    // Fifty later revisions, the change's text and its parent's.
    assert_eq!(few.read, MAX_HOLDER_CHECKS as u64 + 2, "{few:?}");
}

/// A world with more changes than one warm-up builds (four calls of sixty-four blocks).
fn long_world() -> Rig {
    let r = world();
    for at in 0..300 {
        r.w.fake().set(FOO, &body_and(0..6, at));
    }
    r
}

#[test]
fn a_list_of_later_writers_is_no_answer_while_the_index_stops_short_of_the_tip() {
    // The index is built from the oldest block until the one asked about: the blocks after it are not in it yet.
    let r = long_world();
    let first = r
        .service
        .later_writers(&r.w.wiki, None, &block_id(2), FOO, 10)
        .expect("an answer");
    assert!(!first.settled, "{first:?}");
    assert!(first.writers.is_empty());
    // The next request goes on from where the first stopped.
    let next = r
        .service
        .later_writers(&r.w.wiki, None, &block_id(2), FOO, 10)
        .expect("an answer");
    assert!(
        next.settled && next.more && next.writers.len() == 10,
        "{next:?}"
    );
    // And the form's list says nothing rather than a list that leaves out what the index does not hold.
    let r = long_world();
    let held = holders(&r, 2);
    assert!(held.unsettled, "{held:?}");
    assert!(held.listed.is_empty());
}

/// An operation whose path prikk could not resolve: the block that has it may have written any page.
fn unresolved() -> inklkunn_prikk::Operation {
    inklkunn_prikk::Operation {
        kind: "edit-text".to_owned(),
        paths: vec![inklkunn_prikk::storage::PathRef {
            path: None,
            unresolved_node_id: Some("n".repeat(8)),
        }],
    }
}

#[test]
fn a_later_change_that_may_have_written_the_page_and_cannot_be_told_is_counted_not_left_out() {
    // L-6: a later block whose operation names no path may have rewritten the page; the list does not know, and says so.
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    r.w.fake().set(FOO, &body_and(0..6, 1)); // 3: holds it
    r.w.fake().set(FOO, &body_and(0..6, 2)); // 4: the record cannot say what this block did
    r.w.fake().show(4, shows(vec![unresolved()]));
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(blocks(&got), vec![block_id(3)], "{got:?}");
    assert_eq!(got.untraced, 1, "{got:?}");
    assert!(!got.untraced_at_least);
    assert!(!got.is_silent(), "an untraced block is not silence");
    // The writers' list says which block, and that it is no writer the index is sure of.
    let writers = r
        .service
        .later_writers(&r.w.wiki, None, &block_id(2), FOO, 10)
        .expect("writers");
    assert_eq!(writers.untraced, vec![4]);
    assert!(writers.writers.iter().all(|w| w.number != 4));
}

#[test]
fn a_block_that_may_have_written_any_page_is_counted_once_for_every_page_of_the_change() {
    let r = rig(&[(FOO, &foo("The page.")), (BAR, &foo("Bar."))]);
    let both = [(FOO.to_owned(), body(0..6)), (BAR.to_owned(), body(0..6))];
    r.w.fake().set_files(&both); // 2: two pages
    r.w.fake().set(BAZ, &body(0..2)); // 3: a block that may have written anything
    r.w.fake().show(3, shows(vec![unresolved()]));
    r.warm();
    let got = holders(&r, 2);
    assert_eq!(got.untraced, 1, "one block, however many pages: {got:?}");
}

#[test]
fn a_page_that_was_moved_with_its_bytes_is_followed_to_where_it_went() {
    // L-6: after FOO -> BAZ (a pure rename), the later changes of BAZ hold the text the change wrote at FOO.
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2: the change
    r.w.fake()
        .change_files(&[(BAZ.to_owned(), body(0..6))], &[FOO]); // 3: moved, bytes untouched
    r.w.fake()
        .show(3, shows(vec![op("rename-path", &[FOO, BAZ])]));
    r.w.fake().set(BAZ, &body_and(0..6, 1)); // 4: an edit of the new path that keeps the lines
    r.w.fake().set(BAZ, &foo("Something else entirely.")); // 5: rewritten
    r.warm();
    let got = holders(&r, 2);
    assert!(!got.unsettled && !got.moves_unfollowed, "{got:?}");
    assert_eq!(
        got.listed
            .iter()
            .map(|l| (l.block.clone(), l.path.clone()))
            .collect::<Vec<_>>(),
        vec![(block_id(3), BAZ.to_owned()), (block_id(4), BAZ.to_owned())],
        "the move carries the text, and the edit of the new path holds it"
    );
    // The writers' list carries the path each wrote.
    let writers = r
        .service
        .later_writers(&r.w.wiki, None, &block_id(2), FOO, 10)
        .expect("writers");
    assert_eq!(
        writers
            .writers
            .iter()
            .map(|w| (w.number, w.path.as_str(), w.carried))
            .collect::<Vec<_>>(),
        vec![(3, BAZ, true), (4, BAZ, false), (5, BAZ, false)]
    );
}

#[test]
fn a_chain_of_moves_is_followed_up_to_its_bound_and_the_bound_is_said() {
    let moved = |hops: usize| {
        let r = world();
        r.w.fake().set(FOO, &body(0..6)); // 2: the change
        let mut at = FOO.to_owned();
        for hop in 0..hops {
            let to = format!("pages/Q{hop}.md");
            r.w.fake()
                .change_files(&[(to.clone(), body(0..6))], &[at.as_str()]);
            r.w.fake().show(
                3 + hop as u64,
                shows(vec![op("rename-path", &[at.as_str(), to.as_str()])]),
            );
            at = to;
        }
        r.w.fake().set(&at, &body_and(0..6, 1)); // the last edit, at the last path
        r.warm();
        (holders(&r, 2), at)
    };
    // As many moves as are followed: every one is, and the edit after the last is found.
    let (got, last) = moved(MAX_FOLLOWED_MOVES);
    assert!(!got.moves_unfollowed, "{got:?}");
    assert_eq!(
        got.listed.last().map(|l| l.path.clone()),
        Some(last),
        "{got:?}"
    );
    assert_eq!(got.listed.len(), MAX_FOLLOWED_MOVES + 1);
    // One more is not, and the answer says so (and never lists what lies past it as if it were all).
    let (got, last) = moved(MAX_FOLLOWED_MOVES + 1);
    assert!(got.moves_unfollowed, "{got:?}");
    assert!(
        got.listed.iter().all(|l| l.path != last),
        "the edit past the bound is not listed: {got:?}"
    );
    assert!(!got.is_silent());
}

#[test]
fn a_move_that_also_edits_the_page_is_no_pure_move_and_is_not_followed() {
    // A rename and an edit in one block is a write of the new path and a removal of the old: nothing links them.
    let r = world();
    r.w.fake().set(FOO, &body(0..6)); // 2
    r.w.fake()
        .change_files(&[(BAZ.to_owned(), body_and(0..6, 1))], &[FOO]); // 3: moved and edited
    r.w.fake().show(
        3,
        shows(vec![
            op("rename-path", &[FOO, BAZ]),
            op("edit-text", &[BAZ]),
        ]),
    );
    r.w.fake().set(BAZ, &body_and(0..6, 2)); // 4
    r.warm();
    let got = holders(&r, 2);
    assert!(got.listed.is_empty(), "{got:?}");
}
