use std::collections::HashSet;

use super::*;

#[test]
fn every_surface_has_a_name_an_address_and_a_test() {
    let mut ids = HashSet::new();
    let mut tests = HashSet::new();
    for s in SURFACES {
        assert!(ids.insert(s.id), "{} is listed twice", s.id);
        assert!(
            tests.insert(s.test),
            "{} is the test of two surfaces",
            s.test
        );
        assert!(
            !s.routes.is_empty() || !s.api.is_empty(),
            "{} has no address",
            s.id
        );
        assert!(
            s.test.starts_with("surface_"),
            "{}: tests of surfaces are named `surface_…`",
            s.id
        );
        for path in s.routes.iter().chain(s.api) {
            assert!(path.starts_with('/'), "{} names {path}", s.id);
        }
    }
}

#[test]
fn a_route_is_a_surface_or_shows_none_and_never_both() {
    for (path, reason) in SHOWS_NONE {
        assert!(
            !is_surface_route(path),
            "{path} is both a surface and a route that shows none"
        );
        assert!(
            reason.chars().count() >= 16,
            "{path}: a reason is a sentence, not a word"
        );
    }
    let mut seen = HashSet::new();
    for (path, _) in SHOWS_NONE {
        assert!(seen.insert(*path), "{path} is listed twice");
    }
}

#[test]
fn each_part_of_a_revision_is_shown_by_some_surface() {
    assert!(SURFACES.iter().any(|s| s.shows.content));
    assert!(SURFACES.iter().any(|s| s.shows.summary));
    assert!(SURFACES.iter().any(|s| s.shows.author));
    assert_eq!(surface("diff").map(|s| s.shows.any()), Some(true));
    assert!(shows_none("/{slug}/-/search").is_some());
    assert_eq!(shows_none("/nowhere"), None);
}

#[test]
fn a_route_is_classified_by_the_list_it_is_in_and_a_stranger_is_unlisted() {
    assert_eq!(classify("/{slug}/-/history/{*title}"), Class::Surface);
    assert_eq!(
        classify("/{slug}/-/api/v1/history/{*title}"),
        Class::Surface
    );
    assert_eq!(classify("/{slug}/-/search"), Class::ShowsNone);
    assert_eq!(classify("/{slug}/-/moderate/show"), Class::Reveals);
    assert_eq!(
        classify("/{slug}/-/a-new-place-that-shows-text"),
        Class::Unlisted
    );
}

#[test]
fn a_removed_page_and_the_offer_to_restore_it_are_surfaces_and_not_routes_that_show_none() {
    // The page route also answers `410` with the removal's summary; the raw route and the API's page, raw and fragment
    // members answer for a removed page too: the row says so, and none of them is in the list of routes that show none.
    for path in [
        "/{slug}/{*title}",
        "/{slug}/-/raw/{*title}",
        "/{slug}/-/api/v1/pages/{*title}",
        "/{slug}/-/api/v1/raw/{*title}",
        "/{slug}/-/api/v1/fragment/{*title}",
    ] {
        assert_eq!(classify(path), Class::Surface, "{path}");
        assert_eq!(shows_none(path), None, "{path}");
    }
    assert_eq!(surface("removed-page").map(|s| s.shows.summary), Some(true));
    // The restore form copies the last text and the removal's summary into a draft.
    assert_eq!(classify("/{slug}/-/restore/{*title}"), Class::Surface);
    let offer = surface("restore-offer").expect("a row");
    assert!(offer.shows.content && offer.shows.summary);
    // The form for an unreadable knowledge file reads the tip only, for its stated reason.
    assert_eq!(classify("/{slug}/-/knowledge/replace"), Class::ShowsNone);
}
