//! A page's history, a change, a diff and a permalink (handoff 003 T5): which blocks are a page's, what the
//! messages say of the people, what the numbers of prikk calls are, and that a diff is always the page's own.

use inklkunn_prikk::storage::{DiffSide, PathRef};
use inklkunn_prikk::{Change, Diff, DiffEntry, Operation, Patch};

use super::super::testing::{Env, World, block_id, page, state_id};
use super::*;
use crate::message::{Actor, Message, Summary, SystemLabel};
use crate::read::history::ChangeKind;

/// The `i`th of a slice, or a failed test.
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an item")
}

fn op(kind: &str, paths: &[&str]) -> Operation {
    Operation {
        kind: kind.to_owned(),
        paths: paths
            .iter()
            .map(|p| PathRef {
                path: Some((*p).to_owned()),
                unresolved_node_id: None,
            })
            .collect(),
        content: Default::default(),
    }
}

fn change(ops: Vec<Operation>) -> inklkunn_prikk::Result<Change> {
    Ok(Change {
        schema_version: "show-report-v1".to_owned(),
        patches: vec![Patch {
            patch_id: "p".repeat(64),
            queued: false,
            operations: ops,
        }],
    })
}

/// A wiki with `Home` and `Guide` at version 1, then one block per `(message, what it did)`; each block
/// writes `Home` or `Guide` as the operations say, so the tip moves with it.
fn wiki_with(blocks: &[(&str, Vec<Operation>)]) -> World {
    let w = World::pages_with(
        &[("Home", &[], "Home v1.\n"), ("Guide", &[], "Guide v1.\n")],
        ttl_zero(),
    );
    w.fake().say(1, "Create the wiki");
    w.fake().show(
        1,
        change(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    for (i, (message, ops)) in blocks.iter().enumerate() {
        let version = i as u64 + 2;
        let written: Vec<(String, String)> = ops
            .iter()
            .flat_map(|o| o.paths.iter().filter_map(|p| p.path.clone()))
            .map(|p| {
                let title = p
                    .trim_start_matches("pages/")
                    .trim_end_matches(".md")
                    .to_owned();
                let text = page(&[&format!("title: {title}")], &format!("v{version}.\n"));
                (p, text)
            })
            .collect();
        w.fake().set_files(&written);
        w.fake().say(version, message);
        w.fake().show(version, change(ops.clone()));
    }
    w
}

fn history(w: &World, title: &str, after: Option<u64>) -> PageHistory {
    match w
        .service
        .page_history(&w.wiki, None, title, after)
        .expect("a history")
    {
        HistoryOutcome::Page(h) => *h,
        other => panic!("expected a history, got {other:?}"),
    }
}

fn numbers(h: &PageHistory) -> Vec<u64> {
    h.rows.iter().map(|r| r.change.number).collect()
}

fn ct03_proposal() -> String {
    let message = Message::proposal(
        Summary::new("Correct the opening").expect("summary"),
        inklkunn_store::ProposalId::parse("019a0000-0000-7000-8000-00000000aaaa").expect("id"),
        Actor::Account {
            id: inklkunn_store::AccountId::parse("019a0000-0000-7000-8000-0000000000a1")
                .expect("id"),
            name: inklkunn_store::DisplayName::new("Mika Tanaka").expect("name"),
        },
        Actor::Account {
            id: inklkunn_store::AccountId::parse("019a0000-0000-7000-8000-0000000000a2")
                .expect("id"),
            name: inklkunn_store::DisplayName::new("Ren Sato").expect("name"),
        },
        crate::message::BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a proposal message");
    message.to_text()
}

// ------------------------------------------------------------------------------------ a page's rows

#[test]
fn the_history_of_a_page_is_the_blocks_that_touched_it_newest_first() {
    let w = wiki_with(&[
        (
            "Edit the home page",
            vec![op("edit-text", &["pages/Home.md"])],
        ),
        ("Edit the guide", vec![op("edit-text", &["pages/Guide.md"])]),
        (
            "Edit the home page again",
            vec![op("edit-text", &["pages/Home.md"])],
        ),
    ]);
    let h = history(&w, "Home", None);
    assert_eq!(numbers(&h), vec![4, 2, 1]);
    assert_eq!(h.title, "Home");
    assert_eq!(h.path, "pages/Home.md");
    assert!(h.older.is_none() && !h.beyond_window);
    let events: Vec<&[PageEvent]> = h.rows.iter().map(|r| r.events.as_slice()).collect();
    assert_eq!(*nth(&events, 0), [PageEvent::Edited]);
    assert_eq!(*nth(&events, 2), [PageEvent::Created]);
    // The summary and the number are the block's; a block's parent is the one before it.
    let newest = &nth(&h.rows, 0).change;
    assert_eq!(newest.summary, "Edit the home page again");
    assert_eq!(newest.parent.as_deref(), Some(block_id(3).as_str()));
    assert!(nth(&h.rows, 2).change.first && nth(&h.rows, 2).change.parent.is_none());

    let g = history(&w, "Guide", None);
    assert_eq!(numbers(&g), vec![3, 1]);
}

#[test]
fn a_block_is_asked_about_once_however_many_times_the_history_is_read() {
    let w = wiki_with(&[
        ("Edit", vec![op("edit-text", &["pages/Home.md"])]),
        ("Edit again", vec![op("edit-text", &["pages/Home.md"])]),
    ]);
    let (_, first) = w.counted(|| history(&w, "Home", None));
    assert_eq!(first.change, 3, "one `show` for each of the three blocks");
    assert_eq!(first.diff, 0);
    let (_, second) = w.counted(|| history(&w, "Home", None));
    assert_eq!(
        second.change, 0,
        "a block never changes: its record is kept"
    );
    assert_eq!(
        second.history, 0,
        "the log and the pin are held for the tip"
    );
    let (_, guide) = w.counted(|| history(&w, "Guide", None));
    assert_eq!(guide.change, 0);
}

#[test]
fn a_request_makes_a_bounded_number_of_shows_and_says_where_to_continue() {
    let blocks: Vec<(&str, Vec<Operation>)> = (0..60)
        .map(|_| ("Edit the guide", vec![op("edit-text", &["pages/Guide.md"])]))
        .collect();
    let w = wiki_with(&blocks);
    let (h, first) = w.counted(|| history(&w, "Home", None));
    assert_eq!(first.change as usize, MAX_SHOWS_PER_REQUEST);
    assert!(
        h.rows.is_empty(),
        "the newest blocks touched only the guide"
    );
    let cursor = h.older.expect("stopped before the end");
    // The next step begins with the block before the last one inspected, and goes on to the first block.
    let mut after = Some(cursor);
    let mut rounds = 0;
    let mut seen = Vec::new();
    while let Some(step) = after {
        let h = history(&w, "Home", Some(step));
        seen.extend(numbers(&h));
        after = h.older;
        rounds += 1;
        assert!(rounds < 10);
    }
    assert_eq!(
        seen,
        vec![1],
        "the creation of the page is the one change that touched it"
    );
}

#[test]
fn a_long_history_is_paged_by_rows() {
    let blocks: Vec<(&str, Vec<Operation>)> = (0..29)
        .map(|_| ("Edit", vec![op("edit-text", &["pages/Home.md"])]))
        .collect();
    let w = wiki_with(&blocks);
    let first = history(&w, "Home", None);
    assert_eq!(first.rows.len(), HISTORY_PAGE_ROWS);
    assert_eq!(numbers(&first).first(), Some(&30));
    let older = first.older.expect("more");
    assert_eq!(
        older, 11,
        "the last change inspected is where the next step goes on from"
    );
    let second = history(&w, "Home", Some(older));
    assert_eq!(numbers(&second), (1..=10).rev().collect::<Vec<_>>());
    assert!(second.older.is_none());
}

#[test]
fn a_page_that_was_moved_is_followed_back_under_its_old_title() {
    let w = wiki_with(&[
        ("Edit", vec![op("edit-text", &["pages/Guide.md"])]),
        (
            "Rename",
            vec![op("rename-path", &["pages/Guide.md", "pages/Manual.md"])],
        ),
        (
            "Edit the manual",
            vec![op("edit-text", &["pages/Manual.md"])],
        ),
    ]);
    let h = history(&w, "Manual", None);
    assert_eq!(numbers(&h), vec![4, 3, 2, 1]);
    assert_eq!(
        nth(&h.rows, 1).events,
        vec![PageEvent::MovedFrom {
            title: "Guide".to_owned()
        }],
        "a rename is an event of its own"
    );
    assert_eq!(
        nth(&h.rows, 2).events,
        vec![PageEvent::Edited],
        "the old title's edit"
    );
    assert_eq!(nth(&h.rows, 3).events, vec![PageEvent::Created]);
    // The old title's own history shows where the page went, from the move on.
    let old = history(&w, "Guide", None);
    assert!(old.rows.iter().any(
        |r| matches!(r.events.as_slice(), [PageEvent::MovedTo { title }] if title == "Manual")
    ));
}

#[test]
fn a_removed_page_keeps_its_history() {
    let w = wiki_with(&[("Remove", vec![op("delete-node", &["pages/Guide.md"])])]);
    let h = history(&w, "Guide", None);
    assert_eq!(nth(&h.rows, 0).events, vec![PageEvent::Deleted]);
    assert_eq!(nth(&h.rows, 1).events, vec![PageEvent::Created]);
}

#[test]
fn a_block_too_large_to_describe_is_listed_as_not_inspected_and_never_skipped() {
    let w = wiki_with(&[("An import", vec![op("edit-text", &["pages/Guide.md"])])]);
    w.fake().show(
        2,
        Err(StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::OutputExceeded,
        }),
    );
    let h = history(&w, "Home", None);
    assert_eq!(nth(&h.rows, 0).events, vec![PageEvent::NotInspected]);
    assert_eq!(nth(&h.rows, 1).events, vec![PageEvent::Created]);
    // Asked again: the answer is kept (a block never changes), with no second `show`.
    let (_, again) = w.counted(|| history(&w, "Home", None));
    assert_eq!(again.change, 0);
}

#[test]
fn a_busy_prikk_is_could_not_be_read_now_and_nothing_is_kept_of_it() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    w.fake().show(
        2,
        Err(StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::Busy,
        }),
    );
    assert!(matches!(
        w.service.page_history(&w.wiki, None, "Home", None),
        Err(ReadError::CouldNotBeReadNow(_))
    ));
    w.fake()
        .show(2, change(vec![op("edit-text", &["pages/Home.md"])]));
    assert_eq!(numbers(&history(&w, "Home", None)), vec![2, 1]);
}

#[test]
fn an_invalid_title_is_refused_before_prikk_is_asked() {
    let w = wiki_with(&[]);
    let (out, calls) = w.counted(|| w.service.page_history(&w.wiki, None, "  ", None));
    assert!(matches!(out, Err(ReadError::InvalidTitle(_))));
    assert_eq!(calls, Calls::default());
}

// ------------------------------------------------------------------------------------ the people

#[test]
fn the_people_are_what_the_message_says_and_nothing_is_guessed() {
    let w = wiki_with(&[
        ("placeholder", vec![op("edit-text", &["pages/Home.md"])]),
        ("placeholder", vec![op("edit-text", &["pages/Home.md"])]),
        ("placeholder", vec![op("edit-text", &["pages/Home.md"])]),
    ]);
    w.fake().say(2, &ct03_proposal());
    w.fake().say(
        3,
        &Message::system(
            Summary::new("Import the old wiki").expect("summary"),
            SystemLabel::inklkunn(),
        )
        .to_text(),
    );
    // A message someone else wrote: not in the service's form, with control characters and a long first line.
    w.fake().say(
        4,
        &format!("Edit\u{7}the\tpage {}\nSecond line", "x".repeat(300)),
    );
    let h = history(&w, "Home", None);
    let by_number = |n: u64| h.rows.iter().find(|r| r.change.number == n).expect("row");

    let proposal = by_number(2);
    assert_eq!(proposal.change.summary, "Correct the opening");
    assert_eq!(
        proposal.change.provenance,
        Provenance::Recorded {
            kind: ChangeKind::Proposal,
            proposed_by: Who::Person {
                name: "Mika Tanaka".to_owned()
            },
            accepted_by: Who::Person {
                name: "Ren Sato".to_owned()
            },
        }
    );
    assert_eq!(
        by_number(3).change.provenance,
        Provenance::Recorded {
            kind: ChangeKind::System,
            proposed_by: Who::Service,
            accepted_by: Who::Service,
        }
    );
    let foreign = &by_number(4).change;
    assert_eq!(foreign.provenance, Provenance::NotRecorded);
    assert!(foreign.summary.starts_with("Edit the page xxx"));
    assert!(
        !foreign.summary.contains(['\u{7}', '\t', '\n']),
        "{:?}",
        foreign.summary
    );
    assert_eq!(foreign.summary.chars().count(), 120);
    assert!(foreign.summary.ends_with('…'));
    // The first change has the message the wiki was made with, which is no one's proposal either.
    assert_eq!(by_number(1).change.provenance, Provenance::NotRecorded);
}

// ------------------------------------------------------------------------------------ a change

#[test]
fn a_change_page_says_what_the_change_did_without_naming_a_file_that_is_not_a_page() {
    let w = wiki_with(&[("Mixed", vec![op("edit-text", &["pages/Home.md"])])]);
    let mut ops = vec![
        op("edit-text", &["pages/Home.md"]),
        op("edit-text", &["pages/Home.md"]),
        op("create-file", &["attachments/diagram.svg"]),
        op("rename-path", &["pages/Guide.md", "pages/Manual.md"]),
        op("delete-node", &["pages/Old.md"]),
        op("change-perm", &["pages/Home.md"]),
    ];
    ops.extend((0..300).map(|i| op("create-file", &[&format!("pages/Many {i}.md")])));
    w.fake().show(2, change(ops));
    let ChangeOutcome::Found(detail) = w
        .service
        .change(&w.wiki, None, &block_id(2))
        .expect("a change")
    else {
        panic!("no such change");
    };
    assert_eq!(detail.info.number, 2);
    assert_eq!(detail.did.len(), MAX_DID);
    assert_eq!(detail.omitted, 4 + 300 - MAX_DID);
    assert_eq!(
        detail.did.get(..4).expect("four"),
        &[
            Did::Edited(Subject::Page {
                title: "Home".to_owned()
            }),
            Did::Added(Subject::File),
            Did::Moved {
                from: Subject::Page {
                    title: "Guide".to_owned()
                },
                to: Subject::Page {
                    title: "Manual".to_owned()
                },
            },
            Did::Removed(Subject::Page {
                title: "Old".to_owned()
            }),
        ]
    );
    assert!(!detail.too_large);
}

#[test]
fn a_block_id_that_is_not_in_the_history_is_no_such_change_and_malformed_ones_cost_nothing() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    let (out, calls) = w.counted(|| w.service.change(&w.wiki, None, "../../etc"));
    assert_eq!(out.expect("no error"), ChangeOutcome::NoSuchChange);
    assert_eq!(
        calls,
        Calls::default(),
        "a malformed id never reaches prikk"
    );
    let upper = block_id(2).to_uppercase();
    assert_eq!(
        w.service.change(&w.wiki, None, &upper).expect("no error"),
        ChangeOutcome::NoSuchChange
    );
    let unknown = "a".repeat(64);
    assert_eq!(
        w.service.change(&w.wiki, None, &unknown).expect("no error"),
        ChangeOutcome::NoSuchChange
    );
}

// ------------------------------------------------------------------------------------ a diff

fn diff_of(path: &str, status: &str, hunks: &[&str]) -> Diff {
    Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: DiffSide {
            point: block_id(1),
            target_block_id: block_id(1),
        },
        to: DiffSide {
            point: block_id(2),
            target_block_id: block_id(2),
        },
        entries: vec![DiffEntry {
            path: path.to_owned(),
            status: status.to_owned(),
            from: None,
            to: None,
            hunks: hunks.iter().map(|h| (*h).to_owned()).collect(),
        }],
        unsupported_paths: Vec::new(),
    }
}

const HUNK: &str = "@@ -1,3 +1,3 @@\n context\n-Welcome to the wiki, friend.\n+Welcome to the whole wiki, friend.\n tail\n";

#[test]
fn a_diff_is_exactly_one_diff_and_always_restricted_to_the_pages_path() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    w.fake()
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[HUNK])));
    let (out, calls) = w.counted(|| {
        w.service
            .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
    });
    assert_eq!(calls.diff, 1, "exactly one diff");
    let asked = w.fake().diffs_asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        *nth(&asked, 0),
        (
            Point::Block(block_id(1)),
            Point::Block(block_id(2)),
            vec!["pages/Home.md".to_owned()]
        ),
        "from, to and the page's path, nothing else"
    );
    let DiffOutcome::Found(d) = out.expect("a diff") else {
        panic!("no diff");
    };
    assert_eq!((d.added, d.removed), (1, 1));
    assert_eq!(d.status, DiffStatus::Modified);
    // Whatever title was asked for, the path is the page's: an alias or a redirect leads to the page's own.
    assert_eq!(d.path, "pages/Home.md");
}

#[test]
fn the_marks_in_a_diff_never_change_the_lines_prikk_gave() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    let hunks = [
        HUNK,
        "@@ -9 +9 @@\n-日本語のページです。\n+日本語の新しいページです。\n\\ No newline at end of file\n",
        "@@ -1 +1,2 @@\n a\n+<script>alert(1)</script>\n",
    ];
    w.fake()
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &hunks)));
    let DiffOutcome::Found(d) = w
        .service
        .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
        .expect("a diff")
    else {
        panic!("no diff");
    };
    let round_trip: Vec<String> = d.hunks.iter().map(HunkView::to_unified).collect();
    assert_eq!(round_trip, hunks, "the model written back is prikk's hunks");
    // And the marks are there: the refinement is not a no-op.
    let HunkView::Lines(first) = nth(&d.hunks, 0) else {
        panic!("lines");
    };
    assert!(
        first
            .lines
            .iter()
            .any(|l| l.segments.iter().any(|s| s.marked)),
        "{first:?}"
    );
}

#[test]
fn a_block_prikk_does_not_know_is_no_such_change_and_a_binary_page_has_no_lines() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    w.fake().diff_says(Err(StorageError::Refused {
        verb: "diff",
        message: "no such block".to_owned(),
        kind: None,
        reason: None,
    }));
    assert_eq!(
        w.service
            .page_diff(&w.wiki, None, &block_id(1), &"c".repeat(64), "Home")
            .expect("no error"),
        DiffOutcome::NoSuchChange
    );
    w.fake()
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[])));
    let DiffOutcome::Found(d) = w
        .service
        .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
        .expect("a diff")
    else {
        panic!("no diff");
    };
    assert!(d.no_lines && d.hunks.is_empty());
    // Nothing differs.
    let mut none = diff_of("pages/Home.md", "modified", &[]);
    none.entries.clear();
    w.fake().diff_says(Ok(none));
    let DiffOutcome::Found(d) = w
        .service
        .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
        .expect("a diff")
    else {
        panic!("no diff");
    };
    assert_eq!(d.status, DiffStatus::Unchanged);
    assert!(!d.no_lines);
}

#[test]
fn a_diff_compares_only_changes_of_the_views_history() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    w.fake().add_view("heads/draft");
    w.fake()
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[HUNK])));
    // A block of another view is in the repository and not in this view's history: not a reader's to compare.
    let elsewhere = block_id(1_000_001);
    for (from, to) in [(block_id(1), elsewhere.clone()), (elsewhere, block_id(2))] {
        assert_eq!(
            w.service
                .page_diff(&w.wiki, None, &from, &to, "Home")
                .expect("no error"),
            DiffOutcome::NoSuchChange
        );
    }
    assert_eq!(
        w.service
            .page_diff(&w.wiki, None, &block_id(1), &"c".repeat(64), "Home")
            .expect("no error"),
        DiffOutcome::NoSuchChange
    );
    assert!(
        w.fake().diffs_asked().is_empty(),
        "no diff was asked for any of them"
    );
    // Both in the history: one diff, and the numbers of the changes.
    let DiffOutcome::Found(d) = w
        .service
        .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
        .expect("a diff")
    else {
        panic!("no diff");
    };
    assert_eq!((d.from_number, d.to_number), (1, 2));
    assert_eq!(w.fake().diffs_asked().len(), 1);
}

#[test]
fn malformed_block_ids_and_titles_never_reach_prikk() {
    let w = wiki_with(&[]);
    let (out, calls) = w.counted(|| {
        w.service
            .page_diff(&w.wiki, None, "--all", &block_id(1), "Home")
    });
    assert_eq!(out.expect("no error"), DiffOutcome::NoSuchChange);
    assert_eq!(calls, Calls::default());
    let (out, calls) = w.counted(|| {
        w.service
            .page_diff(&w.wiki, None, &block_id(1), &block_id(1), "")
    });
    assert!(matches!(out, Err(ReadError::InvalidTitle(_))));
    assert_eq!(calls, Calls::default());
}

#[test]
fn a_diff_longer_than_the_bound_is_cut_and_counted() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    let mut big = String::from("@@ -1,5000 +1,5000 @@\n");
    for i in 0..(MAX_DIFF_LINES + 10) {
        big.push_str(&format!("+line {i}\n"));
    }
    w.fake()
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[&big, HUNK])));
    let DiffOutcome::Found(d) = w
        .service
        .page_diff(&w.wiki, None, &block_id(1), &block_id(2), "Home")
        .expect("a diff")
    else {
        panic!("no diff");
    };
    assert!(d.truncated);
    assert!(d.hunks.is_empty(), "the first hunk is past the bound");
    assert_eq!(d.added as usize, MAX_DIFF_LINES + 10 + 1, "all counted");
}

// ------------------------------------------------------------------------------------ a permalink

#[test]
fn a_permalink_reads_the_page_at_the_block_and_keeps_the_render_for_the_next_reader() {
    let w = wiki_with(&[
        ("Edit", vec![op("edit-text", &["pages/Home.md"])]),
        ("Edit again", vec![op("edit-text", &["pages/Home.md"])]),
    ]);
    let AtOutcome::Found { view, change } = w
        .service
        .page_at(&w.wiki, None, &block_id(2), "Home", &Env::EN)
        .expect("a page")
    else {
        panic!("no page");
    };
    assert!(view.html.as_str().contains("v2."), "{}", view.html.as_str());
    assert!(!view.html.as_str().contains("v3."));
    assert_eq!(view.tip.block_id, block_id(2));
    assert_eq!(change.number, 2);
    assert!(!view.cached && !view.stale);

    let (again, calls) = w.counted(|| {
        w.service
            .page_at(&w.wiki, None, &block_id(2), "Home", &Env::EN)
    });
    let AtOutcome::Found { view, .. } = again.expect("a page") else {
        panic!("no page");
    };
    assert!(view.cached);
    assert_eq!(calls.read, 0, "no second `cat`");
    assert_eq!(calls.materialize, 0, "and never an extract of the block");
}

#[test]
fn a_permalink_to_a_point_before_the_page_existed_says_so() {
    let w = wiki_with(&[("Make", vec![op("create-file", &["pages/Fresh.md"])])]);
    let missing = w
        .service
        .page_at(&w.wiki, None, &block_id(1), "Fresh", &Env::EN)
        .expect("no error");
    assert!(
        matches!(missing, AtOutcome::PageMissing { .. }),
        "{missing:?}"
    );
    let unknown = w
        .service
        .page_at(&w.wiki, None, &"d".repeat(64), "Home", &Env::EN)
        .expect("no error");
    assert_eq!(unknown, AtOutcome::NoSuchChange);
}

#[test]
fn a_permalink_is_not_kept_in_the_derived_store_and_a_forgotten_log_is_read_again() {
    let w = wiki_with(&[("Edit", vec![op("edit-text", &["pages/Home.md"])])]);
    let _ = w
        .service
        .page_at(&w.wiki, None, &block_id(2), "Home", &Env::EN)
        .expect("a page");
    // The derived store holds the rows a page view put there; a permalink puts none.
    let presentation = format!("{}|d1", Env::EN.key());
    let derived: &dyn inklkunn_store::DerivedStore = &*w.store;
    let stored = derived.page(&inklkunn_store::PageKey {
        wiki: &w.wiki.id,
        tip: &state_id(2),
        path: "pages/Home.md",
        lang: "en",
        presentation: &presentation,
    });
    assert!(matches!(stored, Ok(None)), "{stored:?}");
    // A write path that forgets the wiki makes the next history read the log again.
    let _ = history(&w, "Home", None);
    w.service.forget_wiki(&w.wiki);
    let (_, calls) = w.counted(|| history(&w, "Home", None));
    assert!(calls.history >= 2, "the pin and the log: {calls:?}");
}
