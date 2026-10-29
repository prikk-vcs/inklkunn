//! How many prikk calls a request issues, and that the index is one extract per tip (RFC 003 D-2).

use std::time::Duration;

use super::super::testing::{Env, World, page};
use super::*;

#[test]
fn the_service_and_what_it_hands_out_can_be_shared_between_threads() {
    fn shareable<T: Send + Sync>() {}
    shareable::<ReadService>();
    shareable::<Sanitizer>();
    shareable::<SafeHtml>();
    shareable::<PageOutcome>();
    shareable::<ReadError>();
}

// ------------------------------------------------------------------------------------ call counts

#[test]
fn a_page_view_issues_no_prikk_call_on_a_hit_and_at_most_two_on_a_miss() {
    let w = World::pages(&[
        ("Home", &[], "See [[Guide]] and [[Glossary]].\n"),
        ("Guide", &[], "Guide text.\n"),
        ("Glossary", &[], "Terms.\n"),
    ]);

    // The first view at a tip also builds the index: the tip (`branch list` names the state, one `log` names
    // its block, so every read below is pinned to it), one extract of it (once per tip), the page.
    let (out, first) = w.counted(|| w.service.page(&w.wiki, None, "Home", &Env::EN));
    assert!(!found(out).cached);
    assert_eq!(
        (
            first.views,
            first.history,
            first.materialize,
            first.read,
            first.total()
        ),
        (1, 1, 1, 1, 4)
    );

    // A hit while the tip is held: no call at all.
    let (out, hit) = w.counted(|| w.service.page(&w.wiki, None, "Home", &Env::EN));
    assert!(found(out).cached);
    assert_eq!(hit, Calls::default(), "a hit asks prikk nothing");

    // A miss for another page at the same tip: the index is there and the tip is held, so one `cat`.
    for title in ["Guide", "Glossary"] {
        let (out, miss) = w.counted(|| w.service.page(&w.wiki, None, title, &Env::EN));
        assert!(!found(out).cached);
        assert_eq!(miss.read, 1, "{title}");
        assert!(miss.total() <= 2, "{title}: {miss:?}");
        assert_eq!(
            miss.materialize, 0,
            "{title}: the index is built once per tip"
        );
    }

    // The same page in another language is another cache entry: a miss, still one `cat`.
    let (out, ja) = w.counted(|| w.service.page(&w.wiki, None, "Home", &Env::JA));
    assert!(!found(out).cached);
    assert_eq!(ja.total(), 1);
    // And both languages hit afterwards.
    let (_, again) = w.counted(|| {
        read_page(&w, "Home");
        found(w.service.page(&w.wiki, None, "Home", &Env::JA));
    });
    assert_eq!(again, Calls::default());
}

#[test]
fn when_the_tip_is_not_held_a_miss_is_two_calls_and_a_hit_is_the_one_that_asks_for_the_tip() {
    let w = World::pages_with(
        &[("Home", &[], "Text.\n"), ("Guide", &[], "Text.\n")],
        ttl_zero(),
    );
    let (_, cold) = w.counted(|| read_page(&w, "Home"));
    assert_eq!(cold.total(), 4, "the tip (two calls), the index, the page");
    let (_, hit) = w.counted(|| read_page(&w, "Home"));
    // The list is asked again, the block of an unchanged state is not.
    assert_eq!((hit.views, hit.history, hit.total()), (1, 0, 1));
    let (_, miss) = w.counted(|| read_page(&w, "Guide"));
    assert_eq!((miss.views, miss.read, miss.total()), (1, 1, 2));
}

#[test]
fn the_index_is_built_from_one_extract_per_tip_however_many_pages_and_readers() {
    let list: Vec<(String, String)> = (0..30)
        .map(|n| (format!("P{n}"), format!("Page {n}.\n")))
        .collect();
    let pages: Vec<(&str, &[&str], &str)> = list
        .iter()
        .map(|(t, b)| (t.as_str(), &[][..], b.as_str()))
        .collect();
    let w = World::pages(&pages);
    for (title, _) in &list {
        read_page(&w, title);
    }
    let calls = w.calls();
    assert_eq!(calls.materialize, 1, "one extract for 30 pages");
    assert_eq!(calls.read, 30, "one `cat` for each page's miss");
    assert_eq!(calls.views, 1, "the tip was held");
    assert_eq!(calls.history, 1, "and its block was asked for once");
    assert_eq!(calls.total(), 33);

    // The tip moves: the next request is at a new tip, and so is the index.
    w.fake()
        .set("pages/P0.md", &page(&["title: P0"], "Changed.\n"));
    w.service.forget_wiki(&w.wiki);
    let (view, moved) = w.counted(|| read_page(&w, "P0"));
    assert_eq!(moved.materialize, 1);
    assert_eq!((moved.views, moved.history), (1, 1));
    assert!(view.html.as_str().contains("Changed."));
    let (_, other) = w.counted(|| read_page(&w, "P1"));
    assert_eq!(other.materialize, 0, "the new index serves every page");
    assert_eq!(other.total(), 1);
}

#[test]
fn readers_who_meet_a_new_tip_together_share_one_extract() {
    let list: Vec<(String, String)> = (0..8)
        .map(|n| (format!("P{n}"), format!("Page {n}.\n")))
        .collect();
    let pages: Vec<(&str, &[&str], &str)> = list
        .iter()
        .map(|(t, b)| (t.as_str(), &[][..], b.as_str()))
        .collect();
    let w = World::pages(&pages);
    // Long enough that every reader is inside the build, or waiting for it, at once.
    w.fake().slow_materialize(Duration::from_millis(250));
    std::thread::scope(|s| {
        for (title, _) in &list {
            let w = &w;
            s.spawn(move || {
                let view = read_page(w, title);
                assert!(view.html.as_str().contains("Page"));
            });
        }
    });
    assert_eq!(w.calls().materialize, 1, "one build, however many readers");
    assert_eq!(w.calls().read, 8);
}
