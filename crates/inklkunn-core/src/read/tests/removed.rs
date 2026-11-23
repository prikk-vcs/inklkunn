//! What a visit to a removed page costs (RFC 003 D-2; G-T6 correction S-1): the removal is read and nothing else, so the calls
//! to prikk are the same for a change that removed 200 pages as for one that removed one, and the way back (a rollback plan, the
//! history behind each page) is a call of its own that the visit does not make.

use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{Change, Operation, Patch};

use super::super::testing::{World, page};
use super::*;

fn op(kind: &str, path: &str) -> Operation {
    Operation {
        kind: kind.to_owned(),
        paths: vec![PathRef {
            path: Some(path.to_owned()),
            unresolved_node_id: None,
        }],
        content: Default::default(),
    }
}

fn shows(ops: Vec<Operation>) -> inklkunn_prikk::Result<Change> {
    Ok(Change {
        schema_version: "show-report-v1".to_owned(),
        patches: vec![Patch {
            patch_id: "p".repeat(64),
            queued: false,
            operations: ops,
        }],
    })
}

/// A wiki of `count` pages `P0` ... whose second change removed them all. Nothing is held: the service forgets the wiki.
fn world(count: usize) -> World {
    let titles: Vec<String> = (0..count).map(|i| format!("P{i}")).collect();
    let list: Vec<(&str, &[&str], &str)> = titles
        .iter()
        .map(|t| (t.as_str(), &[][..], "Text.\n"))
        .collect();
    let w = World::pages_with(&list, ttl_zero());
    let paths: Vec<String> = titles.iter().map(|t| format!("pages/{t}.md")).collect();
    w.fake().say(1, "Create the wiki");
    w.fake().show(
        1,
        shows(paths.iter().map(|p| op("create-file", p)).collect()),
    );
    let gone: Vec<&str> = paths.iter().map(String::as_str).collect();
    w.fake().change_files(
        &[("pages/Home.md".to_owned(), page(&["title: Home"], "Hi.\n"))],
        &gone,
    );
    w.fake().say(2, "Remove them all");
    w.fake().show(
        2,
        shows(paths.iter().map(|p| op("delete-node", p)).collect()),
    );
    w.service.forget_wiki(&w.wiki);
    w
}

/// The calls of the first [`ReadService::removed_page`] of `P0`, and of the way back asked after it.
fn costs(count: usize) -> (Calls, Calls, Restore) {
    let w = world(count);
    let (removed, visit) = w.counted(|| {
        w.service
            .removed_page(&w.wiki, None, "P0")
            .expect("a read")
            .expect("a removed page")
    });
    let (way, back) = w.counted(|| w.service.way_back(&w.wiki, None, &removed).expect("a way"));
    (visit, back, way)
}

/// What a visit costs, the same whatever the change removed: the view's tip once, the index's one extract, the history twice
/// (the tip, then the page's history over it), two `show`s (the removal and the creation, to see what each did to the page),
/// and no `cat` at all.
const VISIT: Calls = Calls {
    views: 1,
    tree: 0,
    stat: 0,
    read: 0,
    history: 2,
    change: 2,
    diff: 0,
    plan: 0,
    materialize: 1,
    verify: 0,
};

#[test]
fn a_visit_to_a_removed_page_makes_the_same_calls_for_one_page_and_for_two_hundred() {
    let (one, one_back, _) = costs(1);
    let (many, many_back, way) = costs(200);
    assert_eq!(one, many, "the visit's calls followed the pages removed");
    assert_eq!(many, VISIT);
    assert_eq!(many.read, 0, "a visit reads no page");
    // The way back is the other half, and it is what grows with the change: it is not made by the visit.
    assert!(
        many_back.read > one_back.read && many_back.change >= one_back.change,
        "the way back does cost more for more pages: {one_back:?} {many_back:?}"
    );
    assert!(matches!(way, Restore::Rollback { .. }), "{way:?}");
}
