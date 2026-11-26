#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn index() -> Index {
    let mut ix = Index::new("");
    ix.add(PageEntry {
        path: "Edit Conflicts.md".into(),
        title: "Edit Conflicts".into(),
        aliases: vec!["Edit conflict".into(), "Editing collisions".into()],
        kind: "page".into(),
    });
    ix.add(PageEntry::new(
        "Guides/Getting Started.md",
        "Getting Started",
    ));
    ix.add(PageEntry::new(
        "Admin/Getting Started.md",
        "Getting Started",
    ));
    ix.interwiki
        .insert("wp".into(), "https://en.wikipedia.org/wiki/{title}".into());
    ix
}

#[test]
fn the_mkup5_order() {
    let ix = index();
    let r = |t: &str| ix.resolve(t, Some("Home.md"));
    assert_eq!(r("Edit Conflicts").via, Via::Exact);
    assert_eq!(r("Editing collisions").via, Via::Alias);
    assert_eq!(r("edit conflicts").via, Via::Case);
    assert_eq!(r("EDITING  COLLISIONS").via, Via::Normalized);
    assert_eq!(r("Nope").via, Via::Dangling);
    assert_eq!(r("").via, Via::SamePage);
    assert!(
        !ix.resolve("Getting Started", Some("Home.md")).resolved(),
        "ambiguous is not resolved"
    );
    assert!(r("Edit Conflicts").resolved());
    assert_eq!(
        r("wp:Edit conflict").url.as_deref(),
        Some("https://en.wikipedia.org/wiki/Edit%20conflict")
    );
}

#[test]
fn nearest_then_ambiguous() {
    let ix = index();
    let near = ix.resolve("Getting Started", Some("Guides/Intro.md"));
    assert_eq!(
        (near.via, near.path.as_deref()),
        (Via::Exact, Some("Guides/Getting Started.md"))
    );
    let far = ix.resolve("Getting Started", Some("Home.md"));
    assert_eq!(far.via, Via::Ambiguous);
    assert_eq!(
        far.candidates,
        vec!["Guides/Getting Started.md", "Admin/Getting Started.md"]
    );
    let rel = ix.resolve("../Admin/Getting Started", Some("Guides/Intro.md"));
    assert_eq!(rel.via, Via::Relative);
}

#[test]
fn pages_prefix_is_transparent() {
    let mut ix = Index::new("pages");
    ix.add(PageEntry::new(
        "pages/Guides/Getting Started.md",
        "Getting Started",
    ));
    for t in [
        "Guides/Getting Started",
        "pages/Guides/Getting Started",
        "/Guides/Getting Started",
    ] {
        assert_eq!(ix.resolve(t, Some("pages/Home.md")).via, Via::Exact, "{t}");
    }
}

#[test]
fn a_title_with_a_dot_segment_is_no_interwiki_link_and_stays_under_the_templates_path() {
    let mut ix = index();
    ix.interwiki.insert(
        "gh".into(),
        "https://github.com/org/repo/wiki/{title}".into(),
    );
    // The climb of the review's example, in every place a segment can stand, and the plain `.` and `..`.
    for title in [
        "../../../x",
        "..",
        ".",
        "a/../b",
        "a/./b",
        "a/..",
        "./a",
        "  ../x  ",
    ] {
        let r = ix.resolve(&format!("gh:{title}"), None);
        assert_eq!(r.via, Via::Dangling, "{title}");
        assert!(r.url.is_none(), "{title}");
    }
    // What is not a dot segment is as it was: subpages keep their `/`, dots inside a name are names, empty segments stay.
    for (title, url) in [
        ("Home/Sub", "https://github.com/org/repo/wiki/Home/Sub"),
        ("v1.2", "https://github.com/org/repo/wiki/v1.2"),
        ("...", "https://github.com/org/repo/wiki/..."),
        (
            ".hidden/..x",
            "https://github.com/org/repo/wiki/.hidden/..x",
        ),
        ("a//b", "https://github.com/org/repo/wiki/a//b"),
        // A `%2E` an author types is a percent sign and two letters, and is encoded as one.
        ("%2E%2E", "https://github.com/org/repo/wiki/%252E%252E"),
    ] {
        let r = ix.resolve(&format!("gh:{title}"), None);
        assert_eq!(r.via, Via::Interwiki, "{title}");
        assert_eq!(r.url.as_deref(), Some(url), "{title}");
    }
}

#[test]
fn a_dot_segment_is_refused_in_a_query_template_too_and_one_map_serves_many_indexes() {
    let mut ix = index();
    ix.interwiki
        .insert("q".into(), "https://example.org/read?page={title}".into());
    assert_eq!(ix.resolve("q:../x", None).via, Via::Dangling);
    // `resolve_with` takes the prefixes from the map it is given and the pages from the index.
    let other: BTreeMap<String, String> = BTreeMap::from([(
        "zz".to_owned(),
        "https://zz.example.org/{title}".to_owned(),
    )]);
    assert_eq!(
        ix.resolve_with(&other, "zz:Rust", None).url.as_deref(),
        Some("https://zz.example.org/Rust")
    );
    assert_eq!(
        ix.resolve_with(&other, "wp:Rust", None).via,
        Via::Dangling,
        "the index's own map is not consulted"
    );
    assert_eq!(
        ix.resolve_with(&other, "Edit Conflicts", Some("Home.md"))
            .via,
        Via::Exact
    );
}
