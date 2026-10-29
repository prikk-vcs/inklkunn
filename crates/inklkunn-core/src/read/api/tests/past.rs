//! The history by cursor and the diff whose points may be left out: the cursor is the history's own position,
//! a walk visits every change once, and a diff always asks for one page's path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use inklkunn_prikk::storage::{DiffSide, PathRef};
use inklkunn_prikk::{Change, Diff, DiffEntry, Operation, Patch, Point};

use super::super::super::testing::{World, block_id, page};
use super::super::*;
use crate::read::{ChangeOutcome, DiffOutcome, HISTORY_PAGE_ROWS, ReadConfig};

fn edit(path: &str) -> inklkunn_prikk::Result<Change> {
    Ok(Change {
        schema_version: "show-report-v1".to_owned(),
        patches: vec![Patch {
            patch_id: "p".repeat(64),
            queued: false,
            operations: vec![Operation {
                kind: "edit-text".to_owned(),
                paths: vec![PathRef {
                    path: Some(path.to_owned()),
                    unresolved_node_id: None,
                }],
                content: Default::default(),
            }],
        }],
    })
}

/// `Home` and `Guide` at version 1, then one block for each of versions 2 to `last`: `Home` is edited by the
/// versions in `home`, `Guide` by the rest.
fn wiki(last: u64, home: &[u64]) -> World {
    let w = World::pages_with(
        &[("Home", &[], "Home v1.\n"), ("Guide", &[], "Guide v1.\n")],
        ReadConfig {
            tip_ttl: Duration::ZERO,
            ..ReadConfig::default()
        },
    );
    w.fake().say(1, "Create the wiki");
    w.fake().show(1, edit("pages/Home.md"));
    for v in 2..=last {
        let (title, path) = if home.contains(&v) {
            ("Home", "pages/Home.md")
        } else {
            ("Guide", "pages/Guide.md")
        };
        w.fake().set(
            path,
            &page(&[&format!("title: {title}")], &format!("v{v}.\n")),
        );
        w.fake().say(v, &format!("Edit {title}"));
        w.fake().show(v, edit(path));
    }
    w
}

fn numbers(answer: &HistoryAnswer) -> (Vec<u64>, Option<Cursor>) {
    match answer {
        HistoryAnswer::Page { history, next } => (
            history.rows.iter().map(|r| r.change.number).collect(),
            next.clone(),
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_history_is_walked_by_cursor_and_every_change_is_visited_once_in_order() {
    let w = wiki(60, &[2, 30, 31]);
    let mut seen = Vec::new();
    let mut after: Option<Cursor> = None;
    let mut steps = 0;
    loop {
        let answer = w
            .service
            .api_history(&w.wiki, None, "Home", after.as_ref())
            .unwrap();
        let (rows, next) = numbers(&answer);
        assert!(rows.len() <= HISTORY_PAGE_ROWS);
        seen.extend(rows);
        steps += 1;
        match next {
            Some(next) => after = Some(next),
            None => break,
        }
        assert!(steps < 20, "the walk does not end");
    }
    assert_eq!(seen, [31, 30, 2, 1]);
    assert!(steps >= 2, "the page's changes are far apart: {steps}");
}

#[test]
fn the_cursor_is_the_changes_number_and_a_step_does_not_repeat_what_it_passed() {
    let w = wiki(50, &[2]);
    let first = w.service.api_history(&w.wiki, None, "Home", None).unwrap();
    let (rows, next) = numbers(&first);
    assert!(rows.is_empty(), "no change of Home among the newest ones");
    let next = next.expect("a step that stopped says where to go on");
    assert!(next.as_str().starts_with("n1"));
    let second = w
        .service
        .api_history(&w.wiki, None, "Home", Some(&next))
        .unwrap();
    let (rows, _) = numbers(&second);
    assert!(rows.iter().all(|n| *n < 50), "{rows:?}");
}

#[test]
fn a_busy_pages_history_is_cut_into_steps_of_twenty_without_repeating_or_skipping_a_change() {
    let w = wiki(60, &(2..=60).collect::<Vec<u64>>());
    let mut seen: Vec<u64> = Vec::new();
    let mut after: Option<Cursor> = None;
    let mut sizes = Vec::new();
    loop {
        let answer = w
            .service
            .api_history(&w.wiki, None, "Home", after.as_ref())
            .unwrap();
        let (rows, next) = numbers(&answer);
        sizes.push(rows.len());
        seen.extend(rows);
        match next {
            Some(next) => after = Some(next),
            None => break,
        }
        assert!(sizes.len() < 10, "the walk does not end");
    }
    assert_eq!(seen, (1..=60).rev().collect::<Vec<u64>>());
    assert_eq!(sizes, [20, 20, 20]);
}

#[test]
fn a_title_that_several_pages_have_is_ambiguous_and_a_bad_title_is_refused_before_anything_is_read()
{
    let w = World::pages(&[("A/Same", &[], "x\n"), ("B/Same", &[], "x\n")]);
    match w.service.api_history(&w.wiki, None, "Same", None).unwrap() {
        HistoryAnswer::Ambiguous { candidates } => assert_eq!(candidates.len(), 2),
        other => panic!("{other:?}"),
    }
    let (result, calls) = w.counted(|| w.service.api_history(&w.wiki, None, "..", None));
    assert!(matches!(
        result,
        Err(ApiError::Read(ReadError::InvalidTitle(_)))
    ));
    assert_eq!(calls.views, 0, "{calls:?}");
}

fn diff_of(path: &str, from: u64, to: u64) -> Diff {
    Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: DiffSide {
            point: block_id(from),
            target_block_id: block_id(from),
        },
        to: DiffSide {
            point: block_id(to),
            target_block_id: block_id(to),
        },
        entries: vec![DiffEntry {
            path: path.to_owned(),
            status: "modified".to_owned(),
            from: None,
            to: None,
            hunks: Vec::new(),
        }],
        unsupported_paths: Vec::new(),
    }
}

fn found(outcome: DiffOutcome) -> Box<crate::read::PageDiff> {
    match outcome {
        DiffOutcome::Found(diff) => diff,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_diff_without_its_points_is_the_tip_against_the_change_before_it() {
    let w = wiki(4, &[2, 3, 4]);
    w.fake().diff_says(Ok(diff_of("pages/Home.md", 3, 4)));
    let both = found(
        w.service
            .api_diff(&w.wiki, None, None, None, "Home")
            .unwrap(),
    );
    assert_eq!(
        (both.from.as_str(), both.to.as_str()),
        (&*block_id(3), &*block_id(4))
    );
    let asked = w.fake().diffs_asked();
    assert_eq!(
        asked.last().unwrap(),
        &(
            Point::Block(block_id(3)),
            Point::Block(block_id(4)),
            vec!["pages/Home.md".to_owned()]
        )
    );
    // `to` alone: the change before it. `from` alone: up to the tip.
    let to = found(
        w.service
            .api_diff(&w.wiki, None, None, Some(&block_id(2)), "Home")
            .unwrap(),
    );
    assert_eq!(
        (to.from.as_str(), to.to.as_str()),
        (&*block_id(1), &*block_id(2))
    );
    let from = found(
        w.service
            .api_diff(&w.wiki, None, Some(&block_id(2)), None, "Home")
            .unwrap(),
    );
    assert_eq!(
        (from.from.as_str(), from.to.as_str()),
        (&*block_id(2), &*block_id(4))
    );
    // The tip moves, and so does the default.
    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "v5.\n"));
    w.fake().say(5, "Edit Home");
    w.fake().show(5, edit("pages/Home.md"));
    let moved = found(
        w.service
            .api_diff(&w.wiki, None, None, None, "Home")
            .unwrap(),
    );
    assert_eq!(
        (moved.from.as_str(), moved.to.as_str()),
        (&*block_id(4), &*block_id(5))
    );
}

#[test]
fn nothing_comes_before_the_first_change_and_a_point_that_is_not_the_views_is_no_change() {
    let w = wiki(3, &[2]);
    for outcome in [
        w.service
            .api_diff(&w.wiki, None, None, Some(&block_id(1)), "Home")
            .unwrap(),
        w.service
            .api_diff(&w.wiki, None, None, Some(&"e".repeat(64)), "Home")
            .unwrap(),
        w.service
            .api_diff(&w.wiki, None, Some("abc"), None, "Home")
            .unwrap(),
    ] {
        assert!(matches!(outcome, DiffOutcome::NoSuchChange), "{outcome:?}");
    }
    assert!(w.fake().diffs_asked().is_empty(), "nothing reached prikk");
    assert!(matches!(
        w.service.change(&w.wiki, None, &block_id(1)).unwrap(),
        ChangeOutcome::Found(_)
    ));
}

#[test]
fn a_diff_checks_the_title_before_it_reads_anything() {
    let w = wiki(2, &[2]);
    let (result, calls) = w.counted(|| w.service.api_diff(&w.wiki, None, None, None, ".."));
    assert!(matches!(
        result,
        Err(ApiError::Read(ReadError::InvalidTitle(_)))
    ));
    assert_eq!(calls.views, 0, "{calls:?}");
    assert!(matches!(
        w.service
            .api_diff(&w.wiki, Some("heads/none"), None, None, "Home"),
        Err(ApiError::Read(ReadError::NoSuchView(_)))
    ));
}
