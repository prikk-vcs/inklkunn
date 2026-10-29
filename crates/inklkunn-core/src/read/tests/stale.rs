//! Serving the last page this server has when prikk cannot be asked (OP-03, CON-5): only what is in the index
//! and the page cache for the last tip this process saw, labelled stale, and never rendered from nothing.

use inklkunn_prikk::{StorageError, Unavailable};

use super::super::testing::{Env, World};
use super::*;

fn busy() -> StorageError {
    StorageError::Unavailable {
        verb: "branch",
        reason: Unavailable::Busy,
    }
}

fn world() -> World {
    World::pages_with(
        &[
            ("Home", &[], "Home text.\n"),
            ("Guide", &[], "Guide text.\n"),
        ],
        ttl_zero(),
    )
}

#[test]
fn a_page_this_server_had_is_served_stale_when_prikk_cannot_be_asked_and_says_so() {
    let w = world();
    let fresh = read_page(&w, "Home");
    assert!(!fresh.stale);
    w.fake().fail_views(Some(busy()));
    // The live read fails…
    assert!(matches!(
        w.service.page(&w.wiki, None, "Home", &Env::EN),
        Err(ReadError::CouldNotBeReadNow(_))
    ));
    // …and the stale read makes no prikk call at all.
    let (stale, calls) = w.counted(|| w.service.stale_page(&w.wiki, None, "Home", &Env::EN));
    assert_eq!(calls, Calls::default(), "stale reads ask prikk nothing");
    let stale = stale.expect("the page is in the cache");
    assert!(stale.stale && stale.cached);
    assert_eq!(stale.html, fresh.html);
    assert_eq!(stale.tip.state_id, fresh.tip.state_id);
}

#[test]
fn a_page_that_was_never_cached_is_not_made_up() {
    let w = world();
    read_page(&w, "Home"); // builds the index; Guide is in it, but its page was never rendered
    w.fake().fail_views(Some(busy()));
    assert!(
        w.service
            .stale_page(&w.wiki, None, "Guide", &Env::EN)
            .is_none()
    );
    assert!(
        w.service
            .stale_page(&w.wiki, None, "Nowhere", &Env::EN)
            .is_none(),
        "a title the index does not have"
    );
    assert!(
        w.service
            .stale_page(&w.wiki, None, "Home", &Env::JA)
            .is_none(),
        "another language is another cache entry, and nothing was rendered in it"
    );
}

#[test]
fn a_process_that_has_seen_nothing_has_nothing_stale() {
    let w = world();
    assert!(
        w.service
            .stale_page(&w.wiki, None, "Home", &Env::EN)
            .is_none()
    );
}

#[test]
fn the_last_known_tip_survives_the_write_path_forgetting_the_hold() {
    let w = world();
    let fresh = read_page(&w, "Home");
    w.service.forget_wiki(&w.wiki);
    w.fake().fail_views(Some(busy()));
    let stale = w
        .service
        .stale_page(&w.wiki, None, "Home", &Env::EN)
        .expect("still the last tip's page");
    assert_eq!(stale.html, fresh.html);
}

#[test]
fn a_title_that_cannot_name_a_page_or_a_page_that_cannot_be_shown_is_none() {
    let w = World::pages_with(&[("Home", &[], "x\n")], ttl_zero());
    read_page(&w, "Home");
    assert!(w.service.stale_page(&w.wiki, None, "", &Env::EN).is_none());
    assert!(
        w.service
            .stale_page(&w.wiki, None, "a/-/b", &Env::EN)
            .is_none()
    );
}
