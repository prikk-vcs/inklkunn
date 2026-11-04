//! Comparing two views through the read service (RFC 005 part 2 D-9; handoff 005 K-T8): the tips, the two
//! knowledge indexes, ONE whole-tree `diff` of the two tips (pages only listed, their changed lines counted), the
//! page's own diff with its path, the bounds with what they leave out counted, and a view that is not one.

use std::time::Duration;

use inklkunn_prikk::storage::DiffSide;
use inklkunn_prikk::{Diff, DiffEntry, Point, StorageError};

use super::*;
use crate::knowledge::compare::Presence;
use crate::read::testing::World;
use crate::read::{
    DiffStatus, MAX_COMPARED_CLAIMS, MAX_COMPARED_PAGES, ReadConfig, ReadError, ViewPageOutcome,
};

const HOME: &str =
    "---\ntitle: Home\nid: p-home\n---\n\nThe bridge opened in 1950. ^b1\n\nOther.\n";
const HOME_ALT: &str =
    "---\ntitle: Home\nid: p-home\n---\n\nThe bridge opened in 1951. ^b1\n\nOther.\n";

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

/// A claim said at `p-home#b1`, judged on `judged`, with the standing given and one source.
fn claim_in_home(n: u8, standing: Standing, judged: &str, source: u8) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new("p-home", "b1").expect("an expression")];
    c.community_status = CommunityStatus {
        status: standing,
        proposal: None,
        judged_on: (standing != Standing::Unknown).then(|| digest_of_block(judged)),
    };
    c.sources = vec![SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: digest_of_block(judged),
    }];
    c
}

fn files_of(claims: &[Claim], sources: &[Source]) -> Vec<(String, String)> {
    claims
        .iter()
        .map(|c| (claim_path(&c.id), c.to_yaml()))
        .chain(sources.iter().map(|s| (source_path(&s.id), s.to_yaml())))
        .collect()
}

fn primary(n: u8) -> Source {
    let mut s = Source::new(source_id(n));
    s.kind = SourceKind::Primary;
    s
}

/// A wiki whose default view holds one claim, and a view `alt` that words it differently, decides it otherwise, cites
/// another source and has a claim of its own.
fn world() -> World {
    let base = files_of(
        &[claim_in_home(
            1,
            Standing::Accepted,
            "The bridge opened in 1950. ^b1",
            1,
        )],
        &[primary(1), primary(2)],
    );
    let mut main: Vec<(String, String)> = vec![("pages/Home.md".to_owned(), HOME.to_owned())];
    main.extend(base);
    let refs: Vec<(&str, &str)> = main.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let w = World::with(&refs, config());
    let alt = files_of(
        &[
            claim_in_home(1, Standing::Contested, "The bridge opened in 1951. ^b1", 2),
            claim_in_home(2, Standing::Unresolved, "The bridge opened in 1951. ^b1", 1),
        ],
        &[primary(1), primary(2)],
    );
    let mut files: Vec<(String, String)> = vec![
        ("pages/Home.md".to_owned(), HOME_ALT.to_owned()),
        (
            "pages/Extra.md".to_owned(),
            "---\ntitle: Extra\n---\n\nOnly here.\n".to_owned(),
        ),
    ];
    files.extend(alt);
    w.counting.inner().add_view_with("heads/alt", &files);
    w
}

fn entry(path: &str, status: &str, hunks: &[&str]) -> DiffEntry {
    DiffEntry {
        path: path.to_owned(),
        status: status.to_owned(),
        from: None,
        to: None,
        hunks: hunks.iter().map(|h| (*h).to_owned()).collect(),
    }
}

fn report(entries: Vec<DiffEntry>) -> Diff {
    let side = |p: &str| DiffSide {
        point: p.to_owned(),
        target_block_id: "b".repeat(64),
    };
    Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: side("a"),
        to: side("b"),
        entries,
        unsupported_paths: Vec::new(),
    }
}

const HUNK: &str =
    "@@ -3,1 +3,1 @@\n-The bridge opened in 1950. ^b1\n+The bridge opened in 1951. ^b1\n";

#[test]
fn two_views_that_differ_are_compared_in_claims_and_in_pages() {
    let w = world();
    w.counting.inner().diff_says(Ok(report(vec![
        entry("pages/Home.md", "modified", &[HUNK]),
        entry(
            "pages/New.md",
            "added",
            &["@@ -0,0 +1,2 @@\n+One.\n+Two.\n"],
        ),
        entry("pages/Gone.md", "removed", &["@@ -1,1 +0,0 @@\n-Bye.\n"]),
        // Not pages: a claim's file, an attachment. The page list does not name them.
        entry(
            "knowledge/claims/x.yaml",
            "modified",
            &["@@ -1,1 +1,1 @@\n-a\n+b\n"],
        ),
        entry("attachments/a.png", "added", &[]),
    ])));
    let got = w
        .service
        .compare_views(&w.wiki, None, Some("heads/alt"))
        .unwrap();
    assert!(!got.same);
    assert_eq!(
        (got.a.view.as_str(), got.b.view.as_str()),
        ("heads/main", "heads/alt")
    );
    // The pages: the three, in the order prikk gave them, lines counted, nothing else.
    let pages: Vec<(&str, DiffStatus, u64, u64)> = got
        .pages
        .iter()
        .map(|p| (p.title.as_str(), p.status, p.added, p.removed))
        .collect();
    assert_eq!(
        pages,
        [
            ("Home", DiffStatus::Modified, 1, 1),
            ("New", DiffStatus::Added, 2, 0),
            ("Gone", DiffStatus::Removed, 0, 1),
        ]
    );
    assert_eq!(got.pages_omitted, 0);
    // The claims: the first differs in all three ways, the second is only in `alt`; each says where it is said.
    assert_eq!(got.claims.len(), 2);
    let first = got.claims.first().unwrap();
    assert_eq!(first.difference.id, claim_id(1).to_string());
    assert_eq!(first.difference.presence, Presence::Both);
    assert!(first.difference.wording && first.difference.sources);
    assert_eq!(
        first.difference.status,
        Some(("accepted".to_owned(), "contested".to_owned()))
    );
    assert_eq!(first.said_in.as_deref(), Some("Home"));
    let second = got.claims.get(1).unwrap();
    assert_eq!(second.difference.presence, Presence::OnlyB);
    assert_eq!(second.said_in.as_deref(), Some("Home"));
    assert_eq!(got.claims_omitted, 0);
    // ONE diff, whole tree, between the two tips' blocks.
    let asked = w.counting.inner().diffs_asked();
    assert_eq!(asked.len(), 1);
    let (from, to, paths) = asked.first().unwrap();
    assert_eq!(*from, Point::Block(got.a.block_id.clone()));
    assert_eq!(*to, Point::Block(got.b.block_id.clone()));
    assert!(paths.is_empty());
}

#[test]
fn the_order_of_the_views_is_the_orders_of_the_sides_and_nothing_else() {
    let w = world();
    w.counting.inner().diff_says(Ok(report(vec![])));
    let ab = w
        .service
        .compare_views(&w.wiki, None, Some("heads/alt"))
        .unwrap();
    let ba = w
        .service
        .compare_views(&w.wiki, Some("heads/alt"), None)
        .unwrap();
    assert_eq!(ab.claims.len(), ba.claims.len());
    let presence = |c: &crate::read::ViewComparison| {
        c.claims
            .iter()
            .map(|c| c.difference.presence)
            .collect::<Vec<_>>()
    };
    assert_eq!(presence(&ab), [Presence::Both, Presence::OnlyB]);
    assert_eq!(presence(&ba), [Presence::Both, Presence::OnlyA]);
}

#[test]
fn a_view_compared_with_itself_or_with_a_view_at_the_same_change_asks_for_nothing() {
    let w = world();
    let same = w.service.compare_views(&w.wiki, None, None).unwrap();
    assert!(same.same && same.pages.is_empty() && same.claims.is_empty());
    assert!(w.counting.inner().diffs_asked().is_empty());
}

#[test]
fn a_view_that_is_not_open_is_no_such_view_whichever_side_it_is_on() {
    let w = world();
    for (a, b) in [(None, Some("heads/nope")), (Some("heads/nope"), None)] {
        assert!(
            matches!(
                w.service.compare_views(&w.wiki, a, b),
                Err(ReadError::NoSuchView(v)) if v == "heads/nope"
            ),
            "{a:?} {b:?}"
        );
    }
    assert!(matches!(
        w.service
            .compare_page(&w.wiki, None, Some("heads/nope"), "Home"),
        Err(ReadError::NoSuchView(_))
    ));
    assert!(w.counting.inner().diffs_asked().is_empty());
}

#[test]
fn a_diff_prikk_cannot_give_is_could_not_be_read_now_and_not_an_empty_comparison() {
    let w = world();
    w.counting.inner().diff_says(Err(StorageError::Driver {
        verb: "diff",
        detail: "gone".to_owned(),
    }));
    assert!(matches!(
        w.service.compare_views(&w.wiki, None, Some("heads/alt")),
        Err(ReadError::CouldNotBeReadNow(_))
    ));
}

#[test]
fn the_pages_listed_are_bounded_and_the_rest_are_counted() {
    let w = world();
    let many: Vec<DiffEntry> = (0..MAX_COMPARED_PAGES + 7)
        .map(|n| entry(&format!("pages/P{n:04}.md"), "modified", &[HUNK]))
        .collect();
    w.counting.inner().diff_says(Ok(report(many)));
    let got = w
        .service
        .compare_views(&w.wiki, None, Some("heads/alt"))
        .unwrap();
    assert_eq!(got.pages.len(), MAX_COMPARED_PAGES);
    assert_eq!(got.pages_omitted, 7);
    assert_eq!(got.pages.first().map(|p| p.title.as_str()), Some("P0000"));
}

#[test]
fn the_claims_listed_are_bounded_and_the_rest_are_counted() {
    let w = world();
    let claims: Vec<Claim> = (1..=u8::MAX)
        .take(MAX_COMPARED_CLAIMS + 9)
        .map(|n| claim_in_home(n, Standing::Contested, "The bridge opened in 1951. ^b1", 1))
        .collect();
    // 255 claim ids are the most a test id has; the bound is 200, so 209 asked for is capped at 209 <= 255.
    let mut files: Vec<(String, String)> = vec![("pages/Home.md".to_owned(), HOME_ALT.to_owned())];
    files.extend(files_of(&claims, &[primary(1)]));
    w.counting.inner().add_view_with("heads/many", &files);
    w.counting.inner().diff_says(Ok(report(vec![])));
    let got = w
        .service
        .compare_views(&w.wiki, None, Some("heads/many"))
        .unwrap();
    // `alt` is not involved: the default view has one claim, `many` has 209 (the first of them the same claim, read
    // differently), so 209 claims differ.
    assert_eq!(got.claims.len(), MAX_COMPARED_CLAIMS);
    assert_eq!(got.claims_omitted, 9);
}

#[test]
fn a_pages_own_diff_is_one_diff_with_its_path_and_bounded_as_the_history_pages_is() {
    let w = world();
    w.counting.inner().diff_says(Ok(report(vec![entry(
        "pages/Home.md",
        "modified",
        &[HUNK],
    )])));
    let ViewPageOutcome::Found(found) = w
        .service
        .compare_page(&w.wiki, None, Some("heads/alt"), "Home")
        .unwrap()
    else {
        panic!("the page is in both views")
    };
    assert_eq!(found.path, "pages/Home.md");
    assert_eq!(found.shape.status, DiffStatus::Modified);
    assert_eq!((found.shape.added, found.shape.removed), (1, 1));
    assert_eq!(found.shape.hunks.len(), 1);
    assert!(!found.shape.truncated && !found.shape.no_lines);
    let asked = w.counting.inner().diffs_asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked.first().unwrap().2, vec!["pages/Home.md".to_owned()]);
    // A page both views lack is absent, and nothing is asked of prikk for it.
    let before = w.counting.inner().diffs_asked().len();
    assert_eq!(
        w.service
            .compare_page(&w.wiki, None, Some("heads/alt"), "Nowhere")
            .unwrap(),
        ViewPageOutcome::Absent
    );
    assert_eq!(w.counting.inner().diffs_asked().len(), before);
    // A title that cannot be a page's is refused as such.
    assert!(matches!(
        w.service.compare_page(&w.wiki, None, Some("heads/alt"), ""),
        Err(ReadError::InvalidTitle(_))
    ));
}

#[test]
fn a_page_only_one_view_has_is_found_and_says_so() {
    let w = world();
    w.counting.inner().diff_says(Ok(report(vec![entry(
        "pages/Extra.md",
        "added",
        &["@@ -0,0 +1,1 @@\n+Only here.\n"],
    )])));
    // Only the second view has it, and only the first view has it, are both found.
    for (x, y) in [(None, Some("heads/alt")), (Some("heads/alt"), None)] {
        let found = w.service.compare_page(&w.wiki, x, y, "Extra").unwrap();
        assert!(
            matches!(&found, ViewPageOutcome::Found(f) if f.path == "pages/Extra.md"),
            "{x:?} {y:?}: {found:?}"
        );
    }
    let found = w
        .service
        .compare_page(&w.wiki, None, Some("heads/alt"), "Extra")
        .unwrap();
    assert!(matches!(
        found,
        ViewPageOutcome::Found(f) if f.shape.status == DiffStatus::Added
    ));
}
