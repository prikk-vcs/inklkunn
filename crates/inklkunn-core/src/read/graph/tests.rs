#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use super::*;
use crate::read::index::entries_from_tree;
use crate::read::testing::{TempDir, page};

fn write(root: &Path, path: &str, text: &str) {
    let at = root.join(path);
    std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
    std::fs::write(at, text).expect("write");
}

/// A small wiki with one of everything the health views look for.
fn graph() -> (LinkGraph, TempDir) {
    let tmp = TempDir::new("graph");
    let r = &tmp.0;
    write(
        r,
        "pages/Home.md",
        &page(
            &["title: Home"],
            "Welcome. See [[Guide]] and [[Missing Page]] and [[missing page]].\n\n![[Included]]\n\nAlso [[Hub#Intro|the hub]].\n",
        ),
    );
    write(
        r,
        "pages/Guide.md",
        &page(
            &["title: Guide"],
            "Back to [[Home]].\nA line with [[Ghost]] in it.\n",
        ),
    );
    write(
        r,
        "pages/Hub.md",
        &page(
            &["title: Hub"],
            "Links out to [[Guide]]; [[Hub]] itself; ![[logo.png]].\n",
        ),
    );
    write(
        r,
        "pages/Included.md",
        &page(&["title: Included"], "Included text.\n"),
    );
    write(
        r,
        "pages/Lonely.md",
        &page(
            &["title: Lonely"],
            "Nobody links here and it links nowhere.\n",
        ),
    );
    write(
        r,
        "pages/Draft.md",
        &page(&["title: Draft", "status: draft"], "See [[Home]].\n"),
    );
    write(
        r,
        "pages/Stubby.md",
        &page(&["title: Stubby", "tags: [stub]"], "See [[Home]].\n"),
    );
    // Redirects: fine, double, broken, a loop.
    write(
        r,
        "pages/Old.md",
        &page(&["title: Old", "kind: redirect", "redirect: Guide"], ""),
    );
    write(
        r,
        "pages/Older.md",
        &page(&["title: Older", "kind: redirect", "redirect: Old"], ""),
    );
    write(
        r,
        "pages/Dead.md",
        &page(&["title: Dead", "kind: redirect", "redirect: Nowhere"], ""),
    );
    write(
        r,
        "pages/Ping.md",
        &page(&["title: Ping", "kind: redirect", "redirect: Pong"], ""),
    );
    write(
        r,
        "pages/Pong.md",
        &page(&["title: Pong", "kind: redirect", "redirect: Ping"], ""),
    );
    write(
        r,
        "pages/Binary.md",
        "---\ntitle: &anchor Binary\n---\n\nx\n",
    );
    let entries = entries_from_tree(r, 1000).expect("entries");
    let index = TitleIndex::new("tip", entries);
    (LinkGraph::build(&index, r, 1 << 20), tmp)
}

fn titles(pages: &[&GraphPage]) -> Vec<String> {
    pages.iter().map(|p| p.entry.title.clone()).collect()
}

#[test]
fn wanted_titles_are_ranked_by_the_pages_that_link_to_them_and_files_are_not_wanted() {
    let (g, _tmp) = graph();
    let wanted = g.wanted();
    // `Missing Page` and `missing page` are one wanted title, linked from one page (twice).
    assert_eq!(
        wanted
            .iter()
            .map(|w| (w.title.as_str(), w.linked_from))
            .collect::<Vec<_>>(),
        [("Ghost", 1), ("Missing Page", 1)]
    );
    // The image embed is not a missing page; the redirect to `Nowhere` is a redirect problem, not a want.
    assert!(wanted.iter().all(|w| w.title != "logo.png"));
}

#[test]
fn backlinks_carry_the_line_that_makes_the_link() {
    let (g, _tmp) = graph();
    let home = g
        .pages()
        .iter()
        .find(|p| p.entry.title == "Home")
        .expect("home");
    let back = g.backlinks(&home.entry.path);
    let from: Vec<(&str, BacklinkKind)> = back
        .iter()
        .map(|b| (b.from_title.as_str(), b.kind))
        .collect();
    assert_eq!(
        from,
        [
            ("Draft", BacklinkKind::Link),
            ("Guide", BacklinkKind::Link),
            ("Stubby", BacklinkKind::Link)
        ]
    );
    assert_eq!(back.get(1).unwrap().context, "Back to [[Home]].");
    // A wanted title's backlinks are the pages that want it, with their lines.
    let ghost = g.wanted_backlinks("ghost");
    assert_eq!(ghost.len(), 1);
    assert_eq!(ghost.first().unwrap().from_title, "Guide");
    assert_eq!(
        ghost.first().unwrap().context,
        "A line with [[Ghost]] in it."
    );
    // An inclusion and a redirect are backlinks of their kind.
    let included = g
        .pages()
        .iter()
        .find(|p| p.entry.title == "Included")
        .unwrap();
    assert_eq!(
        g.backlinks(&included.entry.path).first().unwrap().kind,
        BacklinkKind::Include
    );
    let guide = g.pages().iter().find(|p| p.entry.title == "Guide").unwrap();
    assert!(
        g.backlinks(&guide.entry.path)
            .iter()
            .any(|b| b.kind == BacklinkKind::Redirect && b.from_title == "Old")
    );
}

#[test]
fn orphans_dead_ends_and_pages_that_want_help_are_found() {
    let (g, _tmp) = graph();
    // Draft and Stubby are linked from nowhere either; Older redirects, so it is not a page to read.
    assert_eq!(titles(&g.orphans()), ["Draft", "Lonely", "Stubby"]);
    assert_eq!(titles(&g.dead_ends()), ["Included", "Lonely"]);
    assert_eq!(titles(&g.wants_help()), ["Draft", "Stubby"]);
}

#[test]
fn redirect_problems_are_named() {
    let (g, _tmp) = graph();
    let issues: Vec<(&str, &RedirectIssueKind)> = g
        .redirect_issues()
        .iter()
        .map(|i| (i.entry.title.as_str(), &i.kind))
        .collect();
    assert_eq!(
        issues,
        [
            ("Dead", &RedirectIssueKind::Broken),
            ("Older", &RedirectIssueKind::Double),
            ("Ping", &RedirectIssueKind::Loop),
            ("Pong", &RedirectIssueKind::Loop),
        ]
    );
}

#[test]
fn a_page_that_cannot_be_read_is_still_a_page_of_the_graph() {
    let (g, _tmp) = graph();
    assert!(g.pages().iter().any(|p| p.entry.title == "Binary"));
    // It is not listed as an orphan or a dead end: it says nothing either way.
    assert!(!titles(&g.orphans()).contains(&"Binary".to_owned()));
}
