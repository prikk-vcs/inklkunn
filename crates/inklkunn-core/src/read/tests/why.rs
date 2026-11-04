//! *Why is this here?* (handoff 005 K-T7; RFC 005 part 2 D-8): the change that last changed each paragraph and the
//! one that first wrote it, exactly through an `^id` and otherwise by matching the text, within the history page's
//! bounds. Over the in-memory repository, so that both test runs (with and without prikk) run all of it; the real
//! prikk is in `tests/read_path.rs`.

use inklkunn_prikk::StorageError;

use super::super::testing::{World, page};
use super::history::{change, ct03_proposal, op};
use super::*;
use crate::read::history::{HISTORY_PAGE_ROWS, MAX_SHOWS_PER_REQUEST};

fn home(body: &str) -> String {
    page(&["title: Home"], body)
}

/// A wiki whose `Home` has one version per body: the first creates it, each later one edits it. Every block says
/// what it did (`show`), so the page's history is the story.
fn story(bodies: &[&str]) -> World {
    let (first, rest) = bodies.split_first().expect("a first version");
    let w = World::pages_with(&[("Home", &[], first)], ttl_zero());
    w.fake().say(1, "Create the wiki");
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Home.md"])]));
    for (i, body) in rest.iter().enumerate() {
        let version = i as u64 + 2;
        w.fake()
            .set_files(&[("pages/Home.md".to_owned(), home(body))]);
        w.fake().say(version, &format!("Edit {version}"));
        w.fake()
            .show(version, change(vec![op("edit-text", &["pages/Home.md"])]));
    }
    w
}

fn why(w: &World, title: &str) -> WhyPage {
    match w.service.page_why(&w.wiki, None, title).expect("a why") {
        WhyOutcome::Page(page) => *page,
        other => panic!("expected a page, got {other:?}"),
    }
}

/// The paragraph that begins with `start`.
fn para<'a>(page: &'a WhyPage, start: &str) -> &'a WhyParagraph {
    page.paragraphs
        .iter()
        .find(|p| p.paragraph.excerpt.starts_with(start))
        .unwrap_or_else(|| panic!("no paragraph begins {start:?}: {:?}", page.paragraphs))
}

/// The number of the change a step of a chain is.
fn number(page: &WhyPage, step: Option<usize>) -> Option<u64> {
    step.map(|s| page.changes.get(s).expect("a change of the step").number)
}

fn last(page: &WhyPage, start: &str) -> Option<u64> {
    number(page, para(page, start).chain.last_changed)
}

fn first(page: &WhyPage, start: &str) -> Option<u64> {
    number(page, para(page, start).chain.first_written)
}

fn basis(page: &WhyPage, start: &str) -> Basis {
    para(page, start).chain.basis
}

const V1: &str = "The bridge opened in 1950. ^b1\n\nIt crosses the river.\n";
const V2: &str = "The bridge was opened in 1950. ^b1\n\nIt crosses the river.\n";
const V3: &str = "The bridge was opened in 1950. ^b1\n\nIt crosses the wide river.\n";
const V4: &str =
    "Intro added.\n\nThe bridge was opened in 1950. ^b1\n\nIt crosses the wide river.\n";

// ------------------------------------------------------------------------------------ the chains

#[test]
fn a_paragraph_with_an_anchor_is_followed_exactly_through_a_rewording() {
    let w = story(&[V1, V2, V3, V4]);
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(page.title, "Home");
    assert_eq!(page.path, "pages/Home.md");
    // The wording of the paragraph changed in change 2 and it was first written in change 1; its `^id` carried it
    // through the rewording that no unique text could have followed.
    let claimed = para(&page, "The bridge was opened");
    assert_eq!(claimed.paragraph.id.as_deref(), Some("b1"));
    assert_eq!(claimed.paragraph.ordinal, 2);
    assert_eq!(last(&page, "The bridge was opened"), Some(2));
    assert_eq!(first(&page, "The bridge was opened"), Some(1));
    assert_eq!(basis(&page, "The bridge was opened"), Basis::Anchor);
}

#[test]
fn an_ordinary_paragraph_is_followed_by_matching_the_text() {
    let w = story(&[V1, V2, V3, V4]);
    let page = why(&w, "Home");
    // Changed in change 3, written in change 1, followed by its text: its neighbours did not move.
    assert_eq!(last(&page, "It crosses"), Some(3));
    assert_eq!(first(&page, "It crosses"), Some(1));
    assert_eq!(basis(&page, "It crosses"), Basis::Text);
}

#[test]
fn a_paragraph_written_in_a_later_change_is_written_and_last_changed_there() {
    let w = story(&[V1, V2, V3, V4]);
    let page = why(&w, "Home");
    assert_eq!(first(&page, "Intro added"), Some(4));
    assert_eq!(last(&page, "Intro added"), Some(4));
    assert_eq!(basis(&page, "Intro added"), Basis::Text);
    // The paragraphs are the page's, in its order.
    let starts: Vec<&str> = page
        .paragraphs
        .iter()
        .map(|p| p.paragraph.excerpt.as_str())
        .collect();
    assert_eq!(
        starts,
        [
            "Intro added.",
            "The bridge was opened in 1950.",
            "It crosses the wide river."
        ]
    );
}

#[test]
fn a_paragraph_that_was_never_reworded_was_last_changed_where_it_was_written() {
    let w = story(&[
        "Alpha one.\n\nBeta two.\n",
        "Alpha one.\n\nBeta two, reworded.\n",
    ]);
    let page = why(&w, "Home");
    assert_eq!(last(&page, "Alpha"), Some(1));
    assert_eq!(first(&page, "Alpha"), Some(1));
    assert_eq!(last(&page, "Beta"), Some(2));
    assert_eq!(first(&page, "Beta"), Some(1));
}

#[test]
fn paragraphs_reworded_together_have_no_earlier_wording_to_match_and_the_chain_is_by_text() {
    let w = story(&["Alpha one.\n\nBeta two.\n", "Gamma three.\n\nDelta four.\n"]);
    let page = why(&w, "Home");
    // Nothing in the new text matches the old by anchor or by unique text, and two blocks against two are not
    // paired: each reads as written in the change that reworded them. The label is the surface's to say.
    assert_eq!(first(&page, "Gamma"), Some(2));
    assert_eq!(first(&page, "Delta"), Some(2));
    assert_eq!(basis(&page, "Gamma"), Basis::Text);
}

#[test]
fn a_paragraph_is_named_by_its_beginning_on_one_line_without_its_anchor() {
    let long = "word ".repeat(60);
    let w = story(&[&format!(
        "Heading words\n   spread over lines {long}\n\nShort one. ^s1\n"
    )]);
    let page = why(&w, "Home");
    let long_one = page.paragraphs.first().expect("a paragraph");
    assert!(
        long_one
            .paragraph
            .excerpt
            .starts_with("Heading words spread over lines word word")
    );
    assert_eq!(long_one.paragraph.excerpt.chars().count(), EXCERPT_CHARS);
    assert!(long_one.paragraph.excerpt.ends_with('…'));
    let short = page.paragraphs.get(1).expect("a paragraph");
    assert_eq!(short.paragraph.excerpt, "Short one.");
    assert_eq!(short.paragraph.id.as_deref(), Some("s1"));
    assert_eq!(short.paragraph.ordinal, 2);
}

#[test]
fn an_anchor_added_later_does_not_make_the_chain_exact() {
    let w = story(&["Said once.\n", "Said once. ^b1\n"]);
    let page = why(&w, "Home");
    // The paragraph has an `^id` now and had none before: matched by text across that step.
    assert_eq!(basis(&page, "Said once"), Basis::Text);
    assert_eq!(last(&page, "Said once"), Some(2));
    assert_eq!(first(&page, "Said once"), Some(1));
}

#[test]
fn an_anchor_that_two_blocks_carry_names_neither() {
    let w = story(&["One. ^b1\n\nTwo. ^b1\n", "One. ^b1\n\nTwo. ^b1\n\nThree.\n"]);
    let page = why(&w, "Home");
    assert_eq!(basis(&page, "One."), Basis::Text);
    assert_eq!(basis(&page, "Two."), Basis::Text);
}

#[test]
fn an_anchor_that_changed_between_two_wordings_does_not_carry_the_chain() {
    let w = story(&["Said once. ^b1\n", "Said once. ^b2\n"]);
    let page = why(&w, "Home");
    assert_eq!(basis(&page, "Said once"), Basis::Text);
}

#[test]
fn an_anchor_that_is_repeated_on_one_side_of_a_step_does_not_carry_the_chain() {
    // Repeated now, once before.
    let w = story(&["One. ^b1\n\nTwo.\n", "One. ^b1\n\nTwo. ^b1\n"]);
    assert_eq!(basis(&why(&w, "Home"), "One."), Basis::Text);
    // Once now, repeated before.
    let w = story(&["One. ^b1\n\nTwo. ^b1\n", "One. ^b1\n\nTwo.\n"]);
    assert_eq!(basis(&why(&w, "Home"), "One."), Basis::Text);
}

#[test]
fn a_paragraph_changed_and_then_found_not_to_exist_is_last_changed_at_the_change() {
    let w = story(&[
        "Alpha.\n",
        "Alpha.\n\nBeta words.\n",
        "Alpha.\n\nBeta words, edited.\n",
    ]);
    let page = why(&w, "Home");
    assert_eq!(last(&page, "Beta"), Some(3));
    assert_eq!(first(&page, "Beta"), Some(2));
}

#[test]
fn an_anchor_kept_by_a_paragraph_that_moved_is_still_exact() {
    let w = story(&[
        "First here.\n\nThe moved one. ^m1\n",
        "The moved one. ^m1\n\nFirst here.\n",
    ]);
    let page = why(&w, "Home");
    assert_eq!(basis(&page, "The moved one"), Basis::Anchor);
    assert_eq!(first(&page, "The moved one"), Some(1));
}

#[test]
fn the_chain_follows_a_page_through_a_move() {
    let w = World::pages_with(&[("Old", &[], "A fact.\n\nAnother.\n")], ttl_zero());
    w.fake().say(1, "Create");
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Old.md"])]));
    w.fake().change_files(
        &[("pages/Home.md".to_owned(), home("A fact.\n\nAnother.\n"))],
        &["pages/Old.md"],
    );
    w.fake().say(2, "Move");
    w.fake().show(
        2,
        change(vec![op("rename-path", &["pages/Old.md", "pages/Home.md"])]),
    );
    w.fake().set_files(&[(
        "pages/Home.md".to_owned(),
        home("A fact, reworded.\n\nAnother.\n"),
    )]);
    w.fake().say(3, "Edit");
    w.fake()
        .show(3, change(vec![op("edit-text", &["pages/Home.md"])]));
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "A fact"), Some(1));
    assert_eq!(last(&page, "A fact"), Some(3));
    assert_eq!(first(&page, "Another"), Some(1));
    assert_eq!(last(&page, "Another"), Some(1));
}

#[test]
fn a_page_deleted_and_made_again_is_written_where_it_was_made_again() {
    let w = story(&["Old words.\n"]);
    w.fake().change_files(&[], &["pages/Home.md"]);
    w.fake().say(2, "Delete");
    w.fake()
        .show(2, change(vec![op("delete-node", &["pages/Home.md"])]));
    w.fake()
        .set_files(&[("pages/Home.md".to_owned(), home("Old words.\n"))]);
    w.fake().say(3, "Create again");
    w.fake()
        .show(3, change(vec![op("create-file", &["pages/Home.md"])]));
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "Old words"), Some(3), "{page:?}");
}

#[test]
fn a_change_too_large_to_describe_that_made_the_page_again_is_not_a_guess() {
    let w = story(&["Old words.\n"]);
    w.fake().change_files(&[], &["pages/Home.md"]);
    w.fake().say(2, "Delete");
    w.fake()
        .show(2, change(vec![op("delete-node", &["pages/Home.md"])]));
    w.fake()
        .set_files(&[("pages/Home.md".to_owned(), home("Old words, again.\n"))]);
    w.fake().say(3, "A very large change");
    w.fake().show(
        3,
        Err(StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::OutputExceeded,
        }),
    );
    let page = why(&w, "Home");
    // The deletion before it left no page, so the large change is where the paragraph begins.
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "Old words"), Some(3));
}

#[test]
fn a_change_too_large_to_describe_that_made_a_page_again_after_it_was_moved_away_begins_it() {
    let w = story(&["Old words.\n"]);
    w.fake().change_files(
        &[(
            "pages/Other.md".to_owned(),
            page(&["title: Other"], "Old words.\n"),
        )],
        &["pages/Home.md"],
    );
    w.fake().say(2, "Move");
    w.fake().show(
        2,
        change(vec![op(
            "rename-path",
            &["pages/Home.md", "pages/Other.md"],
        )]),
    );
    w.fake()
        .set_files(&[("pages/Home.md".to_owned(), home("New words.\n"))]);
    w.fake().say(3, "A very large change");
    w.fake().show(
        3,
        Err(StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::OutputExceeded,
        }),
    );
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "New words"), Some(3));
}

// ------------------------------------------------------------------------------------ how far back

#[test]
fn a_page_whose_creation_the_history_does_not_list_is_not_followed_beyond_what_it_lists() {
    let w = story(&["Alpha.\n\nBeta.\n", "Alpha.\n\nBeta, again.\n"]);
    // The first block says it made only the guide: the history lists no change that wrote the page.
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Guide.md"])]));
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Unfollowed);
    assert_eq!(first(&page, "Beta"), None);
    assert_eq!(last(&page, "Beta"), Some(2));
    assert_eq!(para(&page, "Alpha").chain.last_changed, None);
}

#[test]
fn an_edit_that_no_listed_change_explains_ends_the_reading() {
    let w = story(&["Alpha.\n", "Alpha, reworded.\n"]);
    // A third block changes the page's text and says it touched only the guide.
    w.fake()
        .set_files(&[("pages/Home.md".to_owned(), home("Alpha, reworded twice.\n"))]);
    w.fake().say(3, "Edit the guide");
    w.fake()
        .show(3, change(vec![op("edit-text", &["pages/Guide.md"])]));
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Unfollowed);
    assert_eq!(para(&page, "Alpha").chain.first_written, None);
}

#[test]
fn nothing_is_read_once_every_paragraph_is_placed() {
    let w = story(&["Zero.\n", "One.\n", "Alpha new.\n\nBeta new.\n"]);
    let (page, calls) = w.counted(|| why(&w, "Home"));
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "Alpha"), Some(3));
    assert_eq!(first(&page, "Beta"), Some(3));
    // The page at the tip, and the page before the newest change: the older versions are not read.
    assert_eq!(calls.read, 2, "{calls:?} {page:?}");
}

#[test]
fn a_reading_that_could_not_be_followed_is_not_called_bounded_though_the_history_goes_on() {
    let bodies: Vec<String> = (1..=30).map(|n| format!("Words, edit {n}.\n")).collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let w = story(&refs);
    // A last block changes the page's text and says it touched only the guide.
    w.fake()
        .set_files(&[("pages/Home.md".to_owned(), home("Words, unexplained.\n"))]);
    w.fake().say(31, "Edit the guide");
    w.fake()
        .show(31, change(vec![op("edit-text", &["pages/Guide.md"])]));
    let page = why(&w, "Home");
    assert_eq!(page.reach, Reach::Unfollowed);
}

#[test]
fn a_tip_newer_than_the_newest_change_of_the_page_reads_the_page_after_that_change() {
    let w = story(&["Alpha.\n", "Alpha, reworded.\n"]);
    // A change to the guide moves the tip; the page is as it was.
    w.fake().set_files(&[(
        "pages/Guide.md".to_owned(),
        page(&["title: Guide"], "Guide v3.\n"),
    )]);
    w.fake().say(3, "Edit the guide");
    w.fake()
        .show(3, change(vec![op("edit-text", &["pages/Guide.md"])]));
    let (page, calls) = w.counted(|| why(&w, "Home"));
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(last(&page, "Alpha"), Some(2));
    assert_eq!(first(&page, "Alpha"), Some(1));
    // The page at the tip, after its newest change, and before it: three reads.
    assert_eq!(calls.read, 3);
}

#[test]
fn a_long_history_is_read_within_the_history_pages_bounds_and_says_where_it_stopped() {
    let bodies: Vec<String> = (1..=70)
        .map(|n| format!("First paragraph, edit {n}.\n\nSecond stays as it was.\n"))
        .collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let w = story(&refs);
    let (page, calls) = w.counted(|| why(&w, "Home"));
    assert_eq!(page.reach, Reach::Bounded);
    assert_eq!(page.changes.len(), HISTORY_PAGE_ROWS);
    assert_eq!(page.changes.first().map(|c| c.number), Some(70));
    // What the bounds did not reach is not claimed: no first writer, and no change for what did not change.
    assert_eq!(first(&page, "Second"), None);
    assert_eq!(last(&page, "Second"), None);
    assert_eq!(first(&page, "First"), None);
    assert_eq!(last(&page, "First"), Some(70));
    // The reads are the history page's own: at most one `show` per block of the step and one `cat` per version.
    assert!(calls.change as usize <= MAX_SHOWS_PER_REQUEST, "{calls:?}");
    // The page at the tip, the page after each of the other changes read, and the page before the oldest of them.
    assert_eq!(calls.read as usize, HISTORY_PAGE_ROWS + 1, "{calls:?}");
    assert_eq!(calls.diff, 0);
    // Asked again, the blocks are held: no further `show`.
    let (_, again) = w.counted(|| why(&w, "Home"));
    assert_eq!(again.change, 0);
}

#[test]
fn what_the_oldest_change_read_did_to_a_paragraph_is_known_from_the_page_before_it() {
    // Thirty changes; the second paragraph is reworded once, in change 11: the oldest of the twenty read.
    let bodies: Vec<String> = (1..=30)
        .map(|n| {
            let second = if n >= 11 {
                "reworded"
            } else {
                "as first written"
            };
            format!("First paragraph, edit {n}.\n\nMiddle holds still.\n\nSecond, {second}.\n")
        })
        .collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let w = story(&refs);
    let page = why(&w, "Home");
    assert_eq!(page.changes.last().map(|c| c.number), Some(11));
    assert_eq!(last(&page, "Second"), Some(11));
    assert_eq!(first(&page, "Second"), None);
}

#[test]
fn a_move_that_is_the_oldest_change_read_is_looked_through_at_the_old_title() {
    let w = World::pages_with(&[("Old", &[], "A fact.\n")], ttl_zero());
    w.fake().say(1, "Create");
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Old.md"])]));
    w.fake().change_files(
        &[("pages/Home.md".to_owned(), home("A fact.\n"))],
        &["pages/Old.md"],
    );
    w.fake().say(2, "Move");
    w.fake().show(
        2,
        change(vec![op("rename-path", &["pages/Old.md", "pages/Home.md"])]),
    );
    // Nineteen edits more: the move is the twentieth change of the page, the oldest the history page lists.
    for version in 3..=21_u64 {
        w.fake().set_files(&[(
            "pages/Home.md".to_owned(),
            home(&format!("A fact, edit {version}.\n")),
        )]);
        w.fake().say(version, "Edit");
        w.fake()
            .show(version, change(vec![op("edit-text", &["pages/Home.md"])]));
    }
    let page = why(&w, "Home");
    assert_eq!(page.changes.len(), HISTORY_PAGE_ROWS);
    assert_eq!(page.changes.last().map(|c| c.number), Some(2));
    assert_eq!(page.reach, Reach::Bounded);
    assert_eq!(first(&page, "A fact"), None);
}

#[test]
fn a_page_of_a_great_many_paragraphs_follows_the_first_ones_and_counts_the_rest() {
    let many = |edit: usize| -> String {
        (0..MAX_WHY_PARAGRAPHS + 7)
            .map(|n| {
                if n == 3 {
                    format!("Paragraph {n} edit {edit}.\n\n")
                } else {
                    format!("Paragraph {n}.\n\n")
                }
            })
            .collect()
    };
    let bodies: Vec<String> = (0..30).map(many).collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let w = story(&refs);
    let page = why(&w, "Home");
    assert_eq!(page.paragraphs.len(), MAX_WHY_PARAGRAPHS);
    assert_eq!(page.left_out, 7);
    assert_eq!(page.reach, Reach::Bounded);
    assert_eq!(last(&page, "Paragraph 3 "), Some(30));
    assert_eq!(last(&page, "Paragraph 2."), None);
}

#[test]
fn a_page_with_no_change_listed_in_the_bounds_says_it_stopped() {
    // Sixty changes to the guide after the page was written: one request's `show`s do not reach the page.
    let w = story(&["Alpha.\n"]);
    for version in 2..=62_u64 {
        w.fake().set_files(&[(
            "pages/Guide.md".to_owned(),
            page(&["title: Guide"], &format!("Guide v{version}.\n")),
        )]);
        w.fake().say(version, "Edit the guide");
        w.fake()
            .show(version, change(vec![op("edit-text", &["pages/Guide.md"])]));
    }
    let page = why(&w, "Home");
    assert!(page.changes.is_empty());
    assert_eq!(page.reach, Reach::Bounded);
    assert_eq!(first(&page, "Alpha"), None);
}

// ------------------------------------------------------------------------------------ what a change names

#[test]
fn the_changes_name_the_proposal_their_message_records_and_no_other() {
    let w = story(&["Alpha.\n", "Alpha, reworded.\n", "Alpha, reworded again.\n"]);
    w.fake().say(2, &ct03_proposal());
    let page = why(&w, "Home");
    let named: Vec<Option<&str>> = page.changes.iter().map(|c| c.proposal.as_deref()).collect();
    assert_eq!(
        named,
        [None, Some("019a0000-0000-7000-8000-00000000aaaa"), None]
    );
}

// ------------------------------------------------------------------------------------ other answers

#[test]
fn a_title_no_page_has_is_missing_and_costs_no_cat() {
    let w = story(&["Alpha.\n"]);
    let (out, calls) = w.counted(|| w.service.page_why(&w.wiki, None, "  Nowhere "));
    assert_eq!(
        out.expect("an answer"),
        WhyOutcome::Missing {
            title: "Nowhere".into()
        }
    );
    assert_eq!(calls.read, 0);
    assert_eq!(calls.change, 0);
}

#[test]
fn a_redirect_page_that_leads_nowhere_is_still_a_page_with_its_own_past() {
    let w = World::pages(&[
        ("Loop A", &["redirect: Loop B"], "x\n"),
        ("Loop B", &["redirect: Loop A"], "y\n"),
    ]);
    w.fake().say(1, "Create");
    w.fake().show(
        1,
        change(vec![
            op("create-file", &["pages/Loop A.md"]),
            op("create-file", &["pages/Loop B.md"]),
        ]),
    );
    let page = why(&w, "Loop A");
    assert_eq!(page.title, "Loop A");
    assert_eq!(page.reach, Reach::Whole);
    assert_eq!(first(&page, "x"), Some(1));
}

#[test]
fn a_title_that_several_pages_match_is_ambiguous() {
    let w = World::pages(&[("A/Same", &[], "x\n"), ("B/Same", &[], "x\n")]);
    assert!(matches!(
        w.service.page_why(&w.wiki, None, "Same").expect("an answer"),
        WhyOutcome::Ambiguous { candidates } if candidates.len() == 2
    ));
}

#[test]
fn a_title_that_cannot_name_a_page_is_refused_before_anything_is_read() {
    let w = story(&["Alpha.\n"]);
    let (out, calls) = w.counted(|| w.service.page_why(&w.wiki, None, ""));
    assert!(matches!(out, Err(ReadError::InvalidTitle(_))));
    assert_eq!(calls.total(), 0);
}

#[test]
fn a_page_that_is_not_text_is_unreadable_and_not_followed() {
    let w = story(&["Alpha.\n"]);
    w.fake().set_bytes("pages/Home.md", &[0xff, 0xfe, 0xfd]);
    w.fake().say(2, "Not text");
    w.fake()
        .show(2, change(vec![op("edit-text", &["pages/Home.md"])]));
    assert!(matches!(
        w.service.page_why(&w.wiki, None, "Home"),
        Err(ReadError::PageUnreadable(_))
    ));
}

#[test]
fn a_read_that_fails_fails_the_whole_page_and_never_a_partial_chain() {
    let w = story(&["Alpha.\n", "Alpha, again.\n"]);
    w.fake().fail_read(Some(StorageError::Driver {
        verb: "cat",
        detail: "gone".to_owned(),
    }));
    assert!(w.service.page_why(&w.wiki, None, "Home").is_err());
}
