//! The backlinks, the health lists and the recent changes in bounded pages: a walk visits every row once, an
//! offset cursor does not outlive the tip or the list it was made for, and the changes' cursor names a change
//! without carrying its id.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use super::super::super::testing::{World, block_id, page};
use super::super::*;
use crate::read::{BacklinkKind, ChangeFilter, ReadConfig};

fn ttl_zero() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::ZERO,
        ..ReadConfig::default()
    }
}

/// `Hub`, five pages that link to it (one of them twice, on two lines), a redirect to it, a page that wants
/// `Ghost`, and `Alone`.
fn linked() -> World {
    World::pages_with(
        &[
            ("Hub", &[], "The hub.\n"),
            ("A", &[], "See [[Hub]].\n"),
            ("B", &[], "See [[Hub]].\nAnd [[Hub]] again.\n"),
            ("C", &[], "See [[Hub]] and [[Ghost]].\n"),
            ("D", &[], "See [[Hub]].\n"),
            ("E", &[], "See [[Hub]].\n"),
            ("Via", &["kind: redirect", "redirect: Hub"], ""),
            ("Alone", &[], "Nothing.\n"),
        ],
        ttl_zero(),
    )
}

fn found(answer: BacklinkAnswer) -> Box<BacklinkPage> {
    match answer {
        BacklinkAnswer::Found(page) => page,
        other => panic!("{other:?}"),
    }
}

fn from_titles(page: &BacklinkPage) -> Vec<String> {
    page.items.iter().map(|b| b.from_title.clone()).collect()
}

#[test]
fn backlinks_are_walked_by_offset_once_each_in_the_graphs_order() {
    let w = linked();
    let whole = found(
        w.service
            .api_backlinks(&w.wiki, None, "Hub", None, 100)
            .unwrap(),
    );
    assert_eq!(whole.title, "Hub");
    assert!(whole.exists && !whole.cut && whole.next.is_none());
    assert_eq!(
        whole.total, 7,
        "five pages, one of them twice, and a redirect"
    );
    assert_eq!(from_titles(&whole), ["A", "B", "B", "C", "D", "E", "Via"]);
    assert_eq!(whole.items.last().unwrap().kind, BacklinkKind::Redirect);
    for size in [1, 2, 3, 7, 8] {
        let mut walked = Vec::new();
        let mut after: Option<Cursor> = None;
        let mut pages = 0;
        loop {
            let page = found(
                w.service
                    .api_backlinks(&w.wiki, None, "Hub", after.as_ref(), size)
                    .unwrap(),
            );
            assert!(page.items.len() <= size);
            assert_eq!(page.total, 7);
            walked.extend(from_titles(&page));
            pages += 1;
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
            assert!(pages < 20);
        }
        assert_eq!(walked, from_titles(&whole), "size {size}");
        assert_eq!(pages, 7_usize.div_ceil(size), "size {size}");
    }
}

#[test]
fn a_title_that_leads_nowhere_is_its_wanted_links_or_not_found() {
    let w = linked();
    let wanted = found(
        w.service
            .api_backlinks(&w.wiki, None, "Ghost", None, 10)
            .unwrap(),
    );
    assert!(!wanted.exists);
    assert_eq!(from_titles(&wanted), ["C"]);
    assert!(matches!(
        w.service
            .api_backlinks(&w.wiki, None, "Nobody", None, 10)
            .unwrap(),
        BacklinkAnswer::NotFound
    ));
    assert!(matches!(
        w.service.api_backlinks(&w.wiki, None, "..", None, 10),
        Err(ApiError::Read(ReadError::InvalidTitle(_)))
    ));
    // A page nothing links to is found, with no links.
    let alone = found(
        w.service
            .api_backlinks(&w.wiki, None, "Alone", None, 10)
            .unwrap(),
    );
    assert!(alone.exists && alone.items.is_empty() && alone.total == 0);
    // An alias or a redirect leads to the page it names.
    let via = found(
        w.service
            .api_backlinks(&w.wiki, None, "Via", None, 10)
            .unwrap(),
    );
    assert_eq!(via.title, "Hub");
}

#[test]
fn an_offset_cursor_is_for_its_tip_its_list_and_its_title() {
    let w = linked();
    let first = found(
        w.service
            .api_backlinks(&w.wiki, None, "Hub", None, 2)
            .unwrap(),
    );
    let cursor = first.next.clone().unwrap();
    // The same list, the same tip: followed.
    assert!(
        w.service
            .api_backlinks(&w.wiki, None, "Hub", Some(&cursor), 2)
            .is_ok()
    );
    // Another title, another list: refused.
    assert!(matches!(
        w.service
            .api_backlinks(&w.wiki, None, "Ghost", Some(&cursor), 2),
        Err(ApiError::InvalidCursor)
    ));
    assert!(matches!(
        w.service
            .api_health(&w.wiki, None, HealthList::Orphans, Some(&cursor), 2),
        Err(ApiError::InvalidCursor)
    ));
    // A hand-made offset past the end is refused, not an empty page, even with this list's own stamp; the
    // end itself is an empty page.
    let Some(Position::Offset { stamp, .. }) = cursor.position() else {
        panic!("{cursor:?}");
    };
    assert!(matches!(
        w.service.api_backlinks(
            &w.wiki,
            None,
            "Hub",
            Some(&Cursor::at_offset(999, &stamp)),
            2
        ),
        Err(ApiError::InvalidCursor)
    ));
    let end = found(
        w.service
            .api_backlinks(&w.wiki, None, "Hub", Some(&Cursor::at_offset(7, &stamp)), 2)
            .unwrap(),
    );
    assert!(end.items.is_empty() && end.next.is_none());
    // The wiki changes: the cursor of the old tip is not followed into the new list.
    w.fake()
        .set("pages/F.md", &page(&["title: F"], "See [[Hub]].\n"));
    assert!(matches!(
        w.service
            .api_backlinks(&w.wiki, None, "Hub", Some(&cursor), 2),
        Err(ApiError::InvalidCursor)
    ));
}

#[test]
fn every_health_list_is_the_graphs_and_is_walked_in_bounded_pages() {
    let w = linked();
    let list = |kind: HealthList, size: usize| -> (Vec<String>, usize, usize) {
        let mut titles = Vec::new();
        let mut after: Option<Cursor> = None;
        let mut pages = 0;
        loop {
            let page = w
                .service
                .api_health(&w.wiki, None, kind, after.as_ref(), size)
                .unwrap();
            assert_eq!(page.list, kind);
            let rows: Vec<String> = match &page.rows {
                HealthRows::Wanted(rows) => rows.iter().map(|r| r.title.clone()).collect(),
                HealthRows::Orphans(rows)
                | HealthRows::DeadEnds(rows)
                | HealthRows::WantsHelp(rows) => {
                    rows.iter().map(|r| r.entry.title.clone()).collect()
                }
                HealthRows::Redirects(rows) => rows.iter().map(|r| r.entry.title.clone()).collect(),
                HealthRows::Stale(rows) => rows.iter().map(|r| r.entry.title.clone()).collect(),
            };
            assert!(rows.len() <= size);
            titles.extend(rows);
            pages += 1;
            match page.next {
                Some(next) => after = Some(next),
                None => return (titles, page.total, pages),
            }
            assert!(pages < 30);
        }
    };
    let (wanted, total, _) = list(HealthList::Wanted, 10);
    assert_eq!(wanted, ["Ghost"]);
    assert_eq!(total, 1);
    // Orphans: nothing links to them.
    let (orphans, total, _) = list(HealthList::Orphans, 100);
    assert!(orphans.contains(&"Alone".to_owned()) && !orphans.contains(&"Hub".to_owned()));
    assert_eq!(total, orphans.len());
    for size in [1, 2, 3] {
        let (walked, walked_total, pages) = list(HealthList::Orphans, size);
        assert_eq!(walked, orphans, "size {size}");
        assert_eq!(walked_total, total);
        assert_eq!(pages, orphans.len().div_ceil(size));
    }
    let (dead_ends, _, _) = list(HealthList::DeadEnds, 100);
    assert!(dead_ends.contains(&"Alone".to_owned()) && dead_ends.contains(&"Hub".to_owned()));
    let (redirects, total, _) = list(HealthList::Redirects, 10);
    assert!(redirects.is_empty(), "the redirect leads to a page");
    assert_eq!(total, 0);
    let (help, _, _) = list(HealthList::WantsHelp, 10);
    assert!(help.is_empty());
    let (stale, _, _) = list(HealthList::Stale, 100);
    assert_eq!(stale.len(), 7, "the pages that are not redirects");
}

#[test]
fn the_stale_list_says_how_far_it_looked_and_the_others_do_not() {
    let w = linked();
    let stale = w
        .service
        .api_health(&w.wiki, None, HealthList::Stale, None, 10)
        .unwrap();
    let scan = stale.scan.expect("the stale list says how far it looked");
    assert!(scan.complete);
    assert!(scan.looked_through >= 1);
    let wanted = w
        .service
        .api_health(&w.wiki, None, HealthList::Wanted, None, 10)
        .unwrap();
    assert!(wanted.scan.is_none());
    assert!(matches!(
        w.service
            .api_health(&w.wiki, Some("heads/none"), HealthList::Stale, None, 10),
        Err(ApiError::Read(ReadError::NoSuchView(_)))
    ));
}

#[test]
fn the_words_of_the_lists_are_the_addresses_and_only_those() {
    for list in HealthList::ALL {
        assert_eq!(HealthList::from_slug(list.slug()), Some(list));
    }
    assert_eq!(HealthList::ALL.len(), 6);
    for bad in ["", "Wanted", "dead_ends", "other", "wanted "] {
        assert_eq!(HealthList::from_slug(bad), None, "{bad:?}");
    }
}

// ------------------------------------------------------------------------------------ changes

/// A wiki with `n` versions, each a change (version 1 is the creation).
fn changed(n: u64) -> World {
    let w = World::pages_with(&[("Home", &[], "v1.\n")], ttl_zero());
    for v in 2..=n {
        w.fake().set(
            "pages/Home.md",
            &page(&["title: Home"], &format!("v{v}.\n")),
        );
    }
    w
}

fn ids(page: &ChangePage) -> Vec<String> {
    page.rows.iter().map(|r| r.block_id.clone()).collect()
}

#[test]
fn the_changes_are_walked_by_the_last_change_seen_once_each_in_order() {
    let w = changed(7);
    let whole = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), None, 100)
        .unwrap();
    assert_eq!(whole.rows.len(), 7);
    assert!(whole.next.is_none() && !whole.beyond_window && whole.views == ["main"]);
    for size in [1, 2, 3, 7, 8] {
        let mut walked = Vec::new();
        let mut after: Option<Cursor> = None;
        let mut pages = 0;
        loop {
            let page = w
                .service
                .api_changes(&w.wiki, &ChangeFilter::default(), after.as_ref(), size)
                .unwrap();
            assert!(page.rows.len() <= size);
            walked.extend(ids(&page));
            pages += 1;
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
            assert!(pages < 20);
        }
        assert_eq!(walked, ids(&whole), "size {size}");
        assert_eq!(pages, 7_usize.div_ceil(size), "size {size}");
    }
}

#[test]
fn a_changes_cursor_follows_the_change_not_a_place_and_carries_no_id() {
    let w = changed(5);
    let first = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), None, 2)
        .unwrap();
    let cursor = first.next.clone().unwrap();
    for id in ids(&first) {
        assert!(!cursor.as_str().contains(&id[..16]), "{cursor:?}");
    }
    // A newer change arrives between the two requests: the second page is still the one after the cursor's
    // change, and the new change is not repeated into it.
    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "v6.\n"));
    let second = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), Some(&cursor), 2)
        .unwrap();
    let all_now = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), None, 100)
        .unwrap();
    let at = all_now
        .rows
        .iter()
        .position(|r| r.block_id == first.rows.last().unwrap().block_id)
        .unwrap();
    assert_eq!(
        ids(&second),
        all_now
            .rows
            .iter()
            .skip(at + 1)
            .take(2)
            .map(|r| r.block_id.clone())
            .collect::<Vec<_>>()
    );
    // A cursor that names no change of the list is refused.
    let unknown = Cursor::after_change(&block_id(999));
    assert!(matches!(
        w.service
            .api_changes(&w.wiki, &ChangeFilter::default(), Some(&unknown), 2),
        Err(ApiError::InvalidCursor)
    ));
}

#[test]
fn a_view_that_is_not_open_is_no_such_view_and_one_that_is_lists_its_own() {
    let w = changed(3);
    w.fake().add_view("heads/draft");
    let draft = ChangeFilter {
        view: Some("draft".to_owned()),
        ..ChangeFilter::default()
    };
    let page = w.service.api_changes(&w.wiki, &draft, None, 10).unwrap();
    assert!(
        page.rows.iter().all(|r| r.view == "heads/draft"),
        "{page:?}"
    );
    assert_eq!(page.views, ["main", "draft"]);
    let nope = ChangeFilter {
        view: Some("nope".to_owned()),
        ..ChangeFilter::default()
    };
    match w.service.api_changes(&w.wiki, &nope, None, 10) {
        Err(ApiError::Read(ReadError::NoSuchView(view))) => assert_eq!(view, "heads/nope"),
        other => panic!("{other:?}"),
    }
    // The full name of a view that is open is the same view.
    let full = ChangeFilter {
        view: Some("heads/draft".to_owned()),
        ..ChangeFilter::default()
    };
    assert!(w.service.api_changes(&w.wiki, &full, None, 10).is_ok());
}

#[test]
fn the_size_of_a_page_of_changes_is_bounded_whatever_is_asked() {
    let w = changed(MAX_PAGE_SIZE as u64 + 5);
    let big = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), None, usize::MAX)
        .unwrap();
    assert_eq!(big.rows.len(), MAX_PAGE_SIZE);
    // The list is the newest hundred of the view; there are older ones, and the answer says so.
    assert!(big.beyond_window);
    let none = w
        .service
        .api_changes(&w.wiki, &ChangeFilter::default(), None, 0)
        .unwrap();
    assert_eq!(none.rows.len(), 1, "a size of nothing is a size of one");
    assert!(none.next.is_some());
}
