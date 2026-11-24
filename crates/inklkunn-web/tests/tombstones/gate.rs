//! **INV-13's gate**, the way the route table is held to its shape: every route and API member that can show a revision's
//! content, summary or author, or a comment's body or author, is in the table of surfaces (`inklkunn_core::tombstone::surfaces`)
//! or states why it shows none. A **new** route that is in neither list fails here, so a surface cannot be added without its
//! row (and the row's test).

use inklkunn_core::tombstone::surfaces::{Class, REVEALS, SHOWS_NONE, SURFACES, classify, surface};
use inklkunn_core::{Guard, RouteTable};
use inklkunn_web::routes::{application_table, content_table};

fn every_route() -> Vec<(String, Guard)> {
    let mut all = Vec::new();
    let tables: [RouteTable; 2] = [application_table().unwrap(), content_table().unwrap()];
    for table in &tables {
        for entry in table.entries() {
            all.push((entry.path.to_owned(), entry.guard));
        }
    }
    all
}

#[test]
fn every_guarded_route_is_a_surface_shows_none_or_is_the_one_reveal() {
    let mut unlisted = Vec::new();
    let mut seen = 0;
    for (path, guard) in every_route() {
        // A public route serves no wiki data, by its own stated reason: the route table's rule.
        if matches!(guard, Guard::Public { .. }) {
            continue;
        }
        seen += 1;
        if classify(&path) == Class::Unlisted {
            unlisted.push(path);
        }
    }
    assert!(seen > 30, "the harness read only {seen} guarded routes");
    assert!(
        unlisted.is_empty(),
        "routes that are neither a surface nor stated to show none (INV-13): {unlisted:?}"
    );
}

#[test]
fn a_new_route_that_shows_revision_content_and_is_in_no_list_fails_the_gate() {
    // The mutation of the gate: a route added to the table and not to a list is `Unlisted`, which the test above fails on.
    for planted in [
        "/{slug}/-/revisions/{id}",
        "/{slug}/-/api/v1/revisions/{id}/raw",
        "/{slug}/-/history2/{*title}",
        "/{slug}/-/at/{block}",
    ] {
        assert_eq!(classify(planted), Class::Unlisted, "{planted}");
    }
    // And the real ones are found where they are listed.
    assert_eq!(classify("/{slug}/-/history/{*title}"), Class::Surface);
    assert_eq!(classify("/{slug}/-/search"), Class::ShowsNone);
    assert_eq!(classify("/{slug}/-/moderate/show"), Class::Reveals);
}

#[test]
fn the_lists_name_only_routes_that_exist() {
    let existing: Vec<String> = every_route().into_iter().map(|(p, _)| p).collect();
    let mut stale = Vec::new();
    for s in SURFACES {
        for path in s.routes.iter().chain(s.api) {
            if !existing.iter().any(|e| e == path) {
                stale.push((*path).to_owned());
            }
        }
    }
    for (path, _) in SHOWS_NONE.iter().chain(REVEALS) {
        if !existing.iter().any(|e| e == path) {
            stale.push((*path).to_owned());
        }
    }
    assert!(
        stale.is_empty(),
        "a list names a route that is not served: {stale:?}"
    );
}

/// **Every surface says what it does for a text that cannot be checked yet, and shows it** (correction L-11): a row that answers
/// *try again* or leaves out what needs the text has its test, by its name, in the unchecked file; and a surface that shows the text
/// of a page or a file at a block never says that it reads none.
#[test]
fn every_surface_has_its_unchecked_test_in_the_unchecked_file() {
    use inklkunn_core::tombstone::surfaces::Unchecked;
    let file = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tombstones/unchecked.rs"),
    )
    .unwrap();
    let mut tests = 0;
    for s in SURFACES {
        match s.unchecked {
            Unchecked::TryAgain(test) | Unchecked::LeavesOut(test) => {
                tests += 1;
                assert!(
                    file.contains(&format!("async fn {test}()")),
                    "the surface `{}` has no test `{test}` of a text that cannot be checked yet",
                    s.id
                );
            }
            Unchecked::NoBlockText(_) => {}
        }
    }
    assert!(tests >= 6, "only {tests} surfaces answer the outcome");
    // The surfaces that show a page's or a file's text at a block do not say that they read none.
    for id in [
        "page-as-of",
        "diff",
        "change-view",
        "proposal-page",
        "restore-offer",
        "why-is-this-here",
        "rules-history",
    ] {
        assert!(
            !matches!(
                inklkunn_core::tombstone::surfaces::surface(id).map(|s| s.unchecked),
                Some(Unchecked::NoBlockText(_)) | None
            ),
            "{id}"
        );
    }
}

#[test]
fn every_surface_has_its_test_in_the_surfaces_file() {
    let file = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tombstones/surfaces.rs"),
    )
    .unwrap();
    for s in SURFACES {
        assert!(
            file.contains(&format!("async fn {}()", s.test)),
            "the surface `{}` has no test `{}`",
            s.id,
            s.test
        );
        assert!(surface(s.id).is_some());
    }
}
