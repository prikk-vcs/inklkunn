#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::path::Path;

use super::*;
use crate::read::testing::{TempDir, page};

fn write(root: &Path, path: &str, bytes: &[u8]) {
    let at = root.join(path);
    std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
    std::fs::write(at, bytes).expect("write");
}

fn entry(path: &str, title: &str) -> IndexEntry {
    IndexEntry {
        path: path.to_owned(),
        title: title.to_owned(),
        aliases: Vec::new(),
        redirect: None,
        page_id: None,
        kind: "page".to_owned(),
        problem: None,
    }
}

// ------------------------------------------------------------------------------------ from a tree

#[test]
fn every_page_under_pages_is_an_entry_with_what_its_frontmatter_says() {
    let tmp = TempDir::new("tree");
    let root = &tmp.0;
    write(
        root,
        "pages/Home.md",
        page(
            &[
                "title: Home",
                "id: 019a0000-0000-7000-8000-000000000001",
                "aliases: [Start, \"Front page\"]",
                "kind: page",
            ],
            "Body.\n",
        )
        .as_bytes(),
    );
    write(
        root,
        "pages/Old Name.md",
        page(&["title: Old Name", "kind: redirect", "redirect: Home"], "").as_bytes(),
    );
    write(
        root,
        "pages/Guides/Getting Started.md",
        b"No frontmatter at all.\n",
    );
    write(
        root,
        "pages/%E6%97%A5%E6%9C%AC%E8%AA%9E.md",
        page(&["title: 日本語"], "x\n").as_bytes(),
    );
    // Not pages: other directories, other extensions, the keep file.
    write(root, "attachments/diagram.svg", b"<svg/>");
    write(root, "pages/.keep", b"");
    write(root, "pages/notes.txt", b"not a page");
    write(root, "wiki-bundle.yaml", b"format: portable-wiki-bundle\n");

    let entries = entries_from_tree(root, 1000).expect("entries");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "pages/%E6%97%A5%E6%9C%AC%E8%AA%9E.md",
            "pages/Guides/Getting Started.md",
            "pages/Home.md",
            "pages/Old Name.md",
        ],
        "by path, byte for byte"
    );
    let by = |p: &str| entries.iter().find(|e| e.path == p).expect(p);
    let home = by("pages/Home.md");
    assert_eq!(home.title, "Home");
    assert_eq!(home.aliases, ["Start", "Front page"]);
    assert_eq!(
        home.page_id.as_deref(),
        Some("019a0000-0000-7000-8000-000000000001")
    );
    assert_eq!(home.kind, "page");
    assert_eq!(home.problem, None);
    let old = by("pages/Old Name.md");
    assert_eq!(old.redirect.as_deref(), Some("Home"));
    assert_eq!(old.kind, "redirect");
    // No frontmatter: the title is the one the path spells.
    assert_eq!(
        by("pages/Guides/Getting Started.md").title,
        "Guides/Getting Started"
    );
    assert_eq!(by("pages/Guides/Getting Started.md").kind, "page");
    // An encoded path and the frontmatter's title agree.
    assert_eq!(by("pages/%E6%97%A5%E6%9C%AC%E8%AA%9E.md").title, "日本語");
}

#[test]
fn a_page_with_no_title_in_its_frontmatter_takes_the_one_its_path_spells_decoded() {
    let tmp = TempDir::new("tree-title");
    write(
        &tmp.0,
        "pages/%E6%97%A5%E6%9C%AC%E8%AA%9E.md",
        page(&["status: draft"], "x\n").as_bytes(),
    );
    let entries = entries_from_tree(&tmp.0, 10).expect("entries");
    assert_eq!(entries.first().map(|e| e.title.as_str()), Some("日本語"));
}

#[test]
fn a_page_that_cannot_be_read_is_still_listed_with_the_reason() {
    let tmp = TempDir::new("tree-problems");
    let root = &tmp.0;
    write(
        root,
        "pages/Anchored.md",
        b"---\ntitle: &a Anchored\n---\nx\n",
    );
    write(root, "pages/Binary.md", &[0x66, 0xff, 0xfe, 0x00, 0x80]);
    let mut long = String::from("---\ntitle: Long\n");
    long.push_str(&format!("note: {}\n", "x".repeat(FRONTMATTER_READ_BYTES)));
    long.push_str("---\nbody\n");
    write(root, "pages/Long.md", long.as_bytes());
    // A long BODY is fine: only the frontmatter matters, and the head is enough for it.
    let mut big_body = String::from("---\ntitle: Big body\naliases: [Large]\n---\n");
    big_body.push_str(&"y".repeat(FRONTMATTER_READ_BYTES * 3));
    write(root, "pages/Big body.md", big_body.as_bytes());
    // A cut that falls inside a multi-byte character of the body is not an invalid file.
    let mut cut = String::from("---\ntitle: Cut\n---\n");
    cut.push_str(&"あ".repeat(FRONTMATTER_READ_BYTES));
    write(root, "pages/Cut.md", cut.as_bytes());

    let entries = entries_from_tree(root, 100).expect("entries");
    let by = |p: &str| entries.iter().find(|e| e.path == p).expect(p);
    assert_eq!(
        by("pages/Anchored.md").problem.as_deref(),
        Some("frontmatter")
    );
    assert_eq!(by("pages/Anchored.md").title, "Anchored");
    assert_eq!(by("pages/Binary.md").problem.as_deref(), Some("not-utf8"));
    assert_eq!(
        by("pages/Long.md").problem.as_deref(),
        Some("frontmatter-too-long")
    );
    assert_eq!(by("pages/Big body.md").problem, None);
    assert_eq!(by("pages/Big body.md").aliases, ["Large"]);
    assert_eq!(by("pages/Cut.md").problem, None);
    assert_eq!(by("pages/Cut.md").title, "Cut");
    assert_eq!(Problem::NotUtf8.code(), "not-utf8");
    assert_eq!(Problem::Frontmatter.code(), "frontmatter");
    assert_eq!(Problem::FrontmatterTooLong.code(), "frontmatter-too-long");
}

#[test]
fn a_tree_without_pages_has_none_and_the_walk_is_bounded() {
    let tmp = TempDir::new("tree-empty");
    assert_eq!(entries_from_tree(&tmp.0, 10).expect("no pages dir"), []);
    for n in 0..12 {
        write(&tmp.0, &format!("pages/P{n}.md"), b"x\n");
    }
    assert_eq!(
        entries_from_tree(&tmp.0, 12)
            .expect("exactly the bound")
            .len(),
        12
    );
    assert!(matches!(
        entries_from_tree(&tmp.0, 11),
        Err(TreeError::TooManyEntries(11))
    ));
    // Directories count too: a tree of many empty directories is not free.
    let deep = TempDir::new("tree-dirs");
    for n in 0..20 {
        std::fs::create_dir_all(deep.0.join(format!("pages/d{n}"))).expect("dir");
    }
    assert!(matches!(
        entries_from_tree(&deep.0, 10),
        Err(TreeError::TooManyEntries(_))
    ));
}

#[test]
fn a_file_that_cannot_be_read_fails_the_build_instead_of_becoming_an_entry() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new("tree-unreadable");
    write(&tmp.0, "pages/Fine.md", b"---\ntitle: Fine\n---\nx\n");
    write(&tmp.0, "pages/Locked.md", b"---\ntitle: Locked\n---\nx\n");
    let locked = tmp.0.join("pages/Locked.md");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // A user for whom the mode does not bind (root) can read it, and shows nothing.
    if std::fs::File::open(&locked).is_ok() {
        println!("skipping: running as a user for whom the mode does not bind");
        return;
    }
    let result = entries_from_tree(&tmp.0, 100);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).expect("chmod back");
    assert!(
        matches!(result, Err(TreeError::Io(_))),
        "an index that is stored carries a transient failure until the tip moves: {result:?}"
    );
}

#[test]
fn a_link_in_the_tree_is_not_followed() {
    let tmp = TempDir::new("tree-links");
    let outside = TempDir::new("tree-outside");
    write(&outside.0, "Secret.md", b"---\ntitle: Secret\n---\nx\n");
    std::fs::create_dir_all(tmp.0.join("pages")).expect("pages");
    std::os::unix::fs::symlink(outside.0.join("Secret.md"), tmp.0.join("pages/Link.md"))
        .expect("link");
    std::os::unix::fs::symlink(&outside.0, tmp.0.join("pages/Dir")).expect("dir link");
    assert_eq!(entries_from_tree(&tmp.0, 100).expect("entries"), []);
}

// ------------------------------------------------------------------------------------ lookups

fn wiki_index() -> TitleIndex {
    let mut guide = entry("pages/Guide.md", "Guide");
    guide.aliases = vec!["How-to".into(), "手引き".into()];
    let mut old = entry("pages/Old Name.md", "Old Name");
    old.kind = "redirect".into();
    old.redirect = Some("Guide".into());
    let mut hop1 = entry("pages/Hop 1.md", "Hop 1");
    hop1.redirect = Some("Hop 2".into());
    let mut hop2 = entry("pages/Hop 2.md", "Hop 2");
    hop2.redirect = Some("Guide".into());
    let mut loop_a = entry("pages/Loop A.md", "Loop A");
    loop_a.redirect = Some("Loop B".into());
    let mut loop_b = entry("pages/Loop B.md", "Loop B");
    loop_b.redirect = Some("Loop A".into());
    let mut lost = entry("pages/Lost.md", "Lost");
    lost.redirect = Some("Nowhere At All".into());
    // A chain that is too long: C0 → C1 → … → C7.
    let mut chain: Vec<IndexEntry> = (0..8)
        .map(|n| entry(&format!("pages/C{n}.md"), &format!("C{n}")))
        .collect();
    for n in 0..7 {
        chain.get_mut(n).expect("link").redirect = Some(format!("C{}", n + 1));
    }
    let mut entries = vec![
        guide,
        old,
        hop1,
        hop2,
        loop_a,
        loop_b,
        lost,
        entry("pages/Home.md", "Home"),
        entry("pages/Guides/Setup.md", "Guides/Setup"),
        entry("pages/A/Same.md", "A/Same"),
        entry("pages/B/Same.md", "B/Same"),
    ];
    entries.extend(chain);
    TitleIndex::new("tip-1", entries)
}

fn found(l: Lookup) -> Found {
    match l {
        Lookup::Found(f) => f,
        other => panic!("expected a page, got {other:?}"),
    }
}

#[test]
fn a_title_leads_to_its_page_and_an_alias_says_it_was_an_alias() {
    let ix = wiki_index();
    assert_eq!(ix.tip(), "tip-1");
    let f = found(ix.lookup("Guide"));
    assert_eq!(f.entry.path, "pages/Guide.md");
    assert_eq!(f.via, Via::Exact);
    assert_eq!(f.redirected_from, None);

    for alias in ["How-to", "手引き", "  How-to  "] {
        let f = found(ix.lookup(alias));
        assert_eq!(f.entry.path, "pages/Guide.md", "{alias}");
        assert_eq!(f.via, Via::Alias);
        assert_eq!(
            f.redirected_from,
            Some(RedirectedFrom {
                title: alias.trim().to_owned(),
                by: RedirectedBy::Alias
            })
        );
    }
}

#[test]
fn a_redirect_page_is_followed_and_the_reader_is_told_where_from() {
    let ix = wiki_index();
    let f = found(ix.lookup("Old Name"));
    assert_eq!(f.entry.path, "pages/Guide.md");
    assert_eq!(f.via, Via::Exact, "the title asked for was a page's own");
    assert_eq!(
        f.redirected_from,
        Some(RedirectedFrom {
            title: "Old Name".into(),
            by: RedirectedBy::Redirect
        })
    );
    // A double redirect is followed too, up to the bound.
    let f = found(ix.lookup("Hop 1"));
    assert_eq!(f.entry.path, "pages/Guide.md");
    assert_eq!(
        f.redirected_from.map(|r| (r.title, r.by)),
        Some(("Hop 1".to_owned(), RedirectedBy::Redirect))
    );
}

#[test]
fn a_redirect_that_does_not_end_at_a_page_says_why() {
    let ix = wiki_index();
    match ix.lookup("Loop A") {
        Lookup::BrokenRedirect { from, problem } => {
            assert_eq!(from.path, "pages/Loop A.md");
            assert_eq!(problem, RedirectProblem::Loop);
        }
        other => panic!("{other:?}"),
    }
    match ix.lookup("Lost") {
        Lookup::BrokenRedirect { from, problem } => {
            assert_eq!(from.title, "Lost");
            assert_eq!(
                problem,
                RedirectProblem::Dangling {
                    target: "Nowhere At All".into()
                }
            );
        }
        other => panic!("{other:?}"),
    }
    match ix.lookup("C0") {
        Lookup::BrokenRedirect { problem, .. } => assert_eq!(problem, RedirectProblem::TooLong),
        other => panic!("{other:?}"),
    }
    // Within the bound, the chain C3 → … → C7 ends at C7, a page.
    assert_eq!(found(ix.lookup("C3")).entry.path, "pages/C7.md");
    assert_eq!(MAX_REDIRECT_HOPS, 5);
}

#[test]
fn tolerant_matches_say_which_rule_matched_so_the_surface_can_send_the_reader_home() {
    let ix = wiki_index();
    assert_eq!(found(ix.lookup("guide")).via, Via::Case);
    assert_eq!(found(ix.lookup("GUIDE")).via, Via::Case);
    // A bare title that is the basename of exactly one page somewhere in the hierarchy (MKUP-5).
    let f = found(ix.lookup("Setup"));
    assert_eq!(f.entry.path, "pages/Guides/Setup.md");
    assert_eq!(f.via, Via::Hierarchy);
    assert_eq!(
        f.redirected_from, None,
        "a tolerant match is not an alias or a redirect"
    );
    // The page's own spelling is exact, including the hierarchy.
    assert_eq!(found(ix.lookup("Guides/Setup")).via, Via::Exact);
}

#[test]
fn several_matches_are_reported_and_no_match_is_missing() {
    let ix = wiki_index();
    match ix.lookup("Same") {
        Lookup::Ambiguous { candidates } => {
            let paths: Vec<&str> = candidates.iter().map(|c| c.path.as_str()).collect();
            assert_eq!(paths, ["pages/A/Same.md", "pages/B/Same.md"]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(ix.lookup("Nothing"), Lookup::Missing);
    assert_eq!(ix.lookup(""), Lookup::Missing);
    assert_eq!(ix.lookup("   "), Lookup::Missing);
}

#[test]
fn the_index_is_by_path_without_duplicates_and_answers_by_path() {
    let ix = TitleIndex::new(
        "t",
        vec![
            entry("pages/B.md", "B"),
            entry("pages/A.md", "A"),
            entry("pages/A.md", "A again"),
        ],
    );
    let paths: Vec<&str> = ix.entries().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["pages/A.md", "pages/B.md"]);
    assert_eq!(ix.entry("pages/B.md").map(|e| e.title.as_str()), Some("B"));
    assert_eq!(ix.entry("pages/C.md"), None);
    assert_eq!(ix.resolver().pages().len(), 2);
}
