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
