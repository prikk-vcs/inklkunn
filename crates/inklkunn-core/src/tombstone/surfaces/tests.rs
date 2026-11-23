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
    assert!(shows_none("/{slug}/{*title}").is_some());
    assert_eq!(shows_none("/nowhere"), None);
}
