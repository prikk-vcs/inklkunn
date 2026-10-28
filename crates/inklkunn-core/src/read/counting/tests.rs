#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::path::Path;

use inklkunn_prikk::{ExtractBounds, Point, Storage};

use super::*;
use crate::read::testing::{Fake, TempDir};

fn main_view() -> Point {
    Point::View("heads/main".to_owned())
}

#[test]
fn every_call_is_counted_by_its_verb_whether_or_not_it_succeeds() {
    let repo = Path::new("/nowhere");
    let counting = CountingStorage::new(Fake::new(&[("pages/Home.md", "Hello\n")]));
    assert_eq!(counting.calls(), Calls::default());
    assert_eq!(counting.calls().total(), 0);

    counting.views(repo).expect("views");
    counting.views(repo).expect("views");
    counting
        .read(repo, &main_view(), "pages/Home.md", 1 << 20)
        .expect("read");
    // A call that fails still cost a process.
    assert!(
        counting
            .read(repo, &main_view(), "pages/Nope.md", 1 << 20)
            .is_err()
    );
    assert!(counting.tree(repo, &main_view(), None).is_err());
    assert!(counting.stat(repo, &main_view(), "pages/Home.md").is_err());
    assert!(counting.history(repo, &main_view(), None).is_err());
    assert!(counting.change(repo, &"a".repeat(64)).is_err());
    assert!(
        counting
            .diff(repo, &main_view(), &main_view(), &["pages/Home.md"])
            .is_err()
    );
    assert!(counting.plan(repo, &main_view()).is_err());
    assert!(counting.verify(repo).is_err());
    let tmp = TempDir::new("counting");
    counting
        .materialize(
            repo,
            &main_view(),
            &tmp.0,
            "out",
            ExtractBounds {
                max_bytes: None,
                max_files: None,
            },
        )
        .expect("materialize");

    assert_eq!(
        counting.calls(),
        Calls {
            views: 2,
            tree: 1,
            stat: 1,
            read: 2,
            history: 1,
            change: 1,
            diff: 1,
            plan: 1,
            materialize: 1,
            verify: 1,
        }
    );
    assert_eq!(counting.calls().total(), 12);
}

#[test]
fn calls_since_a_snapshot_are_the_difference() {
    let repo = Path::new("/nowhere");
    let counting = CountingStorage::new(Fake::new(&[("pages/Home.md", "Hello\n")]));
    counting.views(repo).expect("views");
    let before = counting.calls();
    counting
        .read(repo, &main_view(), "pages/Home.md", 1 << 20)
        .expect("read");
    counting.views(repo).expect("views");
    let delta = counting.calls().since(&before);
    assert_eq!(delta.read, 1);
    assert_eq!(delta.views, 1);
    assert_eq!(delta.total(), 2);
    // A snapshot taken later than the one asked about (or of another counter) cannot go below zero.
    assert_eq!(before.since(&counting.calls()), Calls::default());
}

#[test]
fn what_is_returned_is_the_wrapped_storages_answer() {
    let repo = Path::new("/nowhere");
    let counting = CountingStorage::new(Fake::new(&[("pages/Home.md", "Hello\n")]));
    let bytes = counting
        .read(repo, &main_view(), "pages/Home.md", 1 << 20)
        .expect("read");
    assert_eq!(bytes, b"Hello\n");
    let views = counting.views(repo).expect("views");
    assert_eq!(
        Some(counting.inner().state_id()),
        views.views.first().map(|v| v.state_id.clone())
    );
}
