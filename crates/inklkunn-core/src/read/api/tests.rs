//! The reads the API adds to the read service: the list's pages and cursors, the filters, the raw
//! information, and the stubs that say they are stubs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use super::super::testing::{World, page};
use super::*;
use crate::read::ReadConfig;

fn titles(list: &PageList) -> Vec<&str> {
    list.items.iter().map(|i| i.title.as_str()).collect()
}

fn many(n: usize) -> World {
    let names: Vec<String> = (0..n).map(|i| format!("Page {i:03}")).collect();
    let list: Vec<(&str, &[&str], &str)> = names
        .iter()
        .map(|n| (n.as_str(), &[][..], "Text.\n"))
        .collect();
    World::pages_with(
        &list,
        ReadConfig {
            tip_ttl: Duration::ZERO,
            ..ReadConfig::default()
        },
    )
}

/// Walk the whole list with pages of `size`, returning every page's titles.
fn walk(w: &World, filter: &PageFilter, size: usize) -> Vec<Vec<String>> {
    let mut pages = Vec::new();
    let mut after: Option<Cursor> = None;
    loop {
        let list = w
            .service
            .list_pages(&w.wiki, None, filter, after.as_ref(), size)
            .expect("a page of the list");
        assert!(list.items.len() <= size.min(MAX_PAGE_SIZE));
        pages.push(titles(&list).iter().map(|t| (*t).to_owned()).collect());
        match list.next {
            Some(next) => after = Some(next),
            None => return pages,
        }
        assert!(pages.len() < 1000, "the walk does not end");
    }
}

#[test]
fn a_cursor_round_trips_and_nothing_else_is_a_cursor() {
    let c = Cursor::after_path("pages/Guide.md");
    assert_eq!(Cursor::parse(c.as_str()), Some(c.clone()));
    for bad in [
        "",
        "c1",
        "c1zz",
        "c1abc",
        // another version tag
        "c2706167",
        // no tag
        "70616765732f47",
        // valid hex of a text that is not a page path
        "c1686f6d65",
        // valid hex that is not UTF-8
        "c1ff",
        // hex of `pages(`: begins like a page path but is not one
        "c1706167657328",
        "c1PAGES",
    ] {
        assert!(Cursor::parse(bad).is_none(), "{bad:?}");
    }
    assert!(Cursor::parse(&format!("c1{}", "ab".repeat(5000))).is_none());
    // Upper-case hex of a real path.
    let upper = c.as_str().to_uppercase();
    assert!(Cursor::parse(&upper).is_none(), "{upper}");
}

#[test]
fn a_walk_visits_every_page_once_in_a_fixed_order_in_bounded_pages() {
    let w = many(23);
    let pages = walk(&w, &PageFilter::default(), 5);
    assert_eq!(pages.len(), 5);
    assert_eq!(
        pages.iter().map(Vec::len).collect::<Vec<_>>(),
        [5, 5, 5, 5, 3]
    );
    let flat: Vec<String> = pages.into_iter().flatten().collect();
    let expected: Vec<String> = (0..23).map(|i| format!("Page {i:03}")).collect();
    assert_eq!(flat, expected);
    // The same walk again, with other page sizes, gives the same sequence.
    for size in [1, 7, 23, 24, 100] {
        let again: Vec<String> = walk(&w, &PageFilter::default(), size)
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(again, expected, "size {size}");
    }
}

#[test]
fn the_last_page_has_no_cursor_even_when_it_is_exactly_full() {
    let w = many(10);
    let list = w
        .service
        .list_pages(&w.wiki, None, &PageFilter::default(), None, 10)
        .unwrap();
    assert_eq!(list.items.len(), 10);
    assert!(list.next.is_none(), "{:?}", list.next);
    let list = w
        .service
        .list_pages(&w.wiki, None, &PageFilter::default(), None, 9)
        .unwrap();
    assert!(list.next.is_some());
}

#[test]
fn the_size_is_bounded_whatever_is_asked() {
    let w = many(MAX_PAGE_SIZE + 20);
    let huge = w
        .service
        .list_pages(&w.wiki, None, &PageFilter::default(), None, usize::MAX)
        .unwrap();
    assert_eq!(huge.items.len(), MAX_PAGE_SIZE);
    assert!(huge.next.is_some());
    let none = w
        .service
        .list_pages(&w.wiki, None, &PageFilter::default(), None, 0)
        .unwrap();
    assert_eq!(none.items.len(), 1, "a size of nothing is a size of one");
    const { assert!(DEFAULT_PAGE_SIZE <= MAX_PAGE_SIZE) };
}

#[test]
fn a_walk_does_not_repeat_or_skip_pages_that_stay_while_the_wiki_changes() {
    let w = many(6);
    let first = w
        .service
        .list_pages(&w.wiki, None, &PageFilter::default(), None, 3)
        .unwrap();
    assert_eq!(titles(&first), ["Page 000", "Page 001", "Page 002"]);
    // A page before the cursor and one after it, added between the two requests.
    let added = |t: &str| page(&[&format!("title: {t}")], "New.\n");
    w.fake().set("pages/Aaa.md", &added("Aaa"));
    w.fake().set("pages/Page 002b.md", &added("Page 002b"));
    let second = w
        .service
        .list_pages(
            &w.wiki,
            None,
            &PageFilter::default(),
            first.next.as_ref(),
            10,
        )
        .unwrap();
    assert_eq!(
        titles(&second),
        ["Page 002b", "Page 003", "Page 004", "Page 005"],
        "what was before the cursor is not repeated, what came after it is not skipped"
    );
}

#[test]
fn the_filters_narrow_the_list() {
    let w = World::pages(&[
        ("Home", &[][..], "x\n"),
        ("Docs/Intro", &[][..], "x\n"),
        ("Docs/Deep/Guide", &[][..], "x\n"),
        ("Docs", &[][..], "x\n"),
        ("Other/Intro", &[][..], "x\n"),
        ("Moved", &["kind: redirect", "redirect: Home"][..], ""),
    ]);
    let list = |filter: PageFilter| -> Vec<String> {
        walk(&w, &filter, 100).into_iter().flatten().collect()
    };
    let namespace = |ns: &str| PageFilter {
        namespace: Some(ns.to_owned()),
        ..PageFilter::default()
    };
    assert_eq!(list(namespace("Docs")), ["Docs/Deep/Guide", "Docs/Intro"]);
    assert_eq!(list(namespace("Docs/Deep")), ["Docs/Deep/Guide"]);
    assert_eq!(
        list(namespace("Doc")),
        Vec::<String>::new(),
        "a name, not a prefix of one"
    );
    assert_eq!(list(namespace("Nowhere")), Vec::<String>::new());
    assert_eq!(
        list(PageFilter {
            kind: Some("redirect".to_owned()),
            ..PageFilter::default()
        }),
        ["Moved"]
    );
    assert_eq!(
        list(PageFilter {
            kind: Some("page".to_owned()),
            namespace: Some("Other".to_owned()),
            ..PageFilter::default()
        }),
        ["Other/Intro"]
    );
}

#[test]
fn a_filtered_walk_pages_by_what_matches_not_by_what_it_skipped() {
    let w = World::pages(&[
        ("A/1", &[][..], "x\n"),
        ("B/1", &[][..], "x\n"),
        ("A/2", &[][..], "x\n"),
        ("B/2", &[][..], "x\n"),
        ("A/3", &[][..], "x\n"),
    ]);
    let filter = PageFilter {
        namespace: Some("A".to_owned()),
        ..PageFilter::default()
    };
    let pages = walk(&w, &filter, 2);
    assert_eq!(pages, [vec!["A/1", "A/2"], vec!["A/3"]]);
}

#[test]
fn filters_the_index_cannot_serve_are_refused_not_ignored() {
    let w = many(2);
    for (filter, feature) in [
        (
            PageFilter {
                tag: Some("x".to_owned()),
                ..PageFilter::default()
            },
            Feature::TagFilter,
        ),
        (
            PageFilter {
                changed_since: Some(1),
                ..PageFilter::default()
            },
            Feature::ChangedSince,
        ),
    ] {
        match w.service.list_pages(&w.wiki, None, &filter, None, 10) {
            Err(ApiError::NotAvailable(got)) => assert_eq!(got, feature),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_list_costs_the_index_and_no_page_read() {
    let w = many(30);
    let (list, calls) = w.counted(|| {
        w.service
            .list_pages(&w.wiki, None, &PageFilter::default(), None, 10)
    });
    list.unwrap();
    assert_eq!(calls.read, 0, "{calls:?}");
}

#[test]
fn the_raw_information_follows_the_title_to_its_page() {
    let w = World::pages(&[
        ("Guide", &["aliases: [How-to]"][..], "x\n"),
        ("Old", &["kind: redirect", "redirect: Guide"][..], ""),
    ]);
    let info = |t: &str| w.service.raw_info(&w.wiki, None, t).unwrap();
    let guide = info("Guide").expect("a page");
    assert_eq!(guide.title, "Guide");
    assert_eq!(guide.storage_path, "pages/Guide.md");
    assert_eq!(guide.kind, "page");
    assert_eq!(info("How-to").unwrap().storage_path, "pages/Guide.md");
    assert_eq!(info("Old").unwrap().storage_path, "pages/Guide.md");
    assert!(info("Nowhere").is_none());
    assert!(matches!(
        w.service.raw_info(&w.wiki, None, ""),
        Err(ApiError::Read(ReadError::InvalidTitle(_)))
    ));
    assert!(matches!(
        w.service.raw_info(&w.wiki, Some("heads/none"), "Guide"),
        Err(ApiError::Read(ReadError::NoSuchView(_)))
    ));
}

#[test]
fn the_reads_other_tasks_own_say_they_are_not_available() {
    let w = many(1);
    let s = &w.service;
    let wiki = &w.wiki;
    let gaps = [
        (
            s.api_history(wiki, None, "Page 000").map(|_| ()),
            Feature::History,
        ),
        (s.api_revision(wiki, "abc").map(|_| ()), Feature::Revision),
        (
            s.api_diff(wiki, None, None, "pages/Page 000.md")
                .map(|_| ()),
            Feature::Diff,
        ),
        (
            s.api_backlinks(wiki, None, "Page 000").map(|_| ()),
            Feature::Backlinks,
        ),
        (s.api_health(wiki, None).map(|_| ()), Feature::Health),
        (s.api_search(wiki, None, "x").map(|_| ()), Feature::Search),
        (s.api_changes(wiki, None).map(|_| ()), Feature::Changes),
    ];
    for (got, feature) in gaps {
        match got {
            Err(ApiError::NotAvailable(f)) => assert_eq!(f, feature),
            other => panic!("{feature:?}: {other:?}"),
        }
    }
}
