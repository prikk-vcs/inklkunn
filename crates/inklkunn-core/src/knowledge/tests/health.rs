//! The health list *Knowledge files that cannot be read* (RFC 005 part 2 D-5), at the seam the API reads.

use std::time::Duration;

use super::*;
use crate::read::testing::World;
use crate::read::{HealthList, HealthRows, ReadConfig};

fn world(extra: &[(String, String)]) -> World {
    let mut files: Vec<(String, String)> = vec![(
        "pages/One.md".to_owned(),
        "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n".to_owned(),
    )];
    files.extend_from_slice(extra);
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with(
        &refs,
        ReadConfig {
            tip_ttl: Duration::from_secs(3600),
            ..ReadConfig::default()
        },
    )
}

fn listed(w: &World, limit: usize, after: Option<&crate::read::Cursor>) -> crate::read::HealthPage {
    w.service
        .api_health(&w.wiki, None, HealthList::KnowledgeFiles, after, limit)
        .expect("a list")
}

#[test]
fn the_list_is_among_the_eight_and_has_its_own_slug() {
    assert_eq!(HealthList::ALL.len(), 8);
    assert_eq!(HealthList::KnowledgeFiles.slug(), "knowledge-files");
    assert_eq!(
        HealthList::from_slug("knowledge-files"),
        Some(HealthList::KnowledgeFiles)
    );
}

#[test]
fn a_wiki_with_sound_files_has_an_empty_list() {
    let mut s = Source::new(source_id(1));
    s.kind = SourceKind::Primary;
    let w = world(&[(source_path(&s.id), s.to_yaml())]);
    let page = listed(&w, 50, None);
    assert_eq!(page.total, 0);
    assert!(matches!(page.rows, HealthRows::KnowledgeFiles(ref r) if r.is_empty()));
}

#[test]
fn each_file_that_cannot_be_read_is_listed_with_its_rule_and_the_pages_are_still_served() {
    let extra = vec![
        (
            claim_path(&claim_id(1)),
            "kind: observation\nscore: 4\n".to_owned(),
        ),
        (
            source_path(&source_id(2)),
            "kind: primary\nkind: other\n".to_owned(),
        ),
        ("knowledge/stray.txt".to_owned(), "x".to_owned()),
    ];
    let w = world(&extra);
    let page = listed(&w, 50, None);
    assert_eq!(page.total, 3);
    let HealthRows::KnowledgeFiles(rows) = page.rows else {
        panic!("rows of the wrong list");
    };
    let got: Vec<(&str, Rule)> = rows.iter().map(|r| (r.path.as_str(), r.rule)).collect();
    assert_eq!(
        got,
        [
            (claim_path(&claim_id(1)).as_str(), Rule::Unknown),
            (source_path(&source_id(2)).as_str(), Rule::Syntax),
            ("knowledge/stray.txt", Rule::Path),
        ]
    );
    assert!(matches!(
        w.service
            .page(&w.wiki, None, "One", &crate::read::testing::Env::EN)
            .expect("the page"),
        crate::read::PageOutcome::Found(_)
    ));
}

#[test]
fn the_list_is_walked_in_pages_by_a_cursor_stamped_with_the_tip() {
    let extra: Vec<(String, String)> = (1..=5u8)
        .map(|n| (claim_path(&claim_id(n)), "nope".to_owned()))
        .collect();
    let w = world(&extra);
    let first = listed(&w, 2, None);
    assert_eq!(first.total, 5);
    let second = listed(&w, 2, first.next.as_ref());
    let third = listed(&w, 2, second.next.as_ref());
    assert!(third.next.is_none());
    let paths = |p: &crate::read::HealthPage| match &p.rows {
        HealthRows::KnowledgeFiles(r) => r.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    let all: Vec<String> = [&first, &second, &third]
        .into_iter()
        .flat_map(paths)
        .collect();
    assert_eq!(all.len(), 5);
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
    // A cursor of another list is not followed.
    let other = w
        .service
        .api_health(&w.wiki, None, HealthList::Wanted, first.next.as_ref(), 2);
    assert!(other.is_err());
}
