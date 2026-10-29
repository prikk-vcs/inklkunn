#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::read::ChangeKind;

use super::*;
use crate::catalog::EN;

fn query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn row(view: &str, pages: Option<&[&str]>) -> ChangeRow {
    ChangeRow {
        block_id: "b".repeat(64),
        view: view.to_owned(),
        parent_block: None,
        update_seq: 1,
        summary: "Edit".to_owned(),
        kind: ChangeKind::Proposal,
        proposed_by: Some(Person::Account("Aiko".into())),
        accepted_by: Some(Person::Account("Ben".into())),
        rollback: false,
        minor: false,
        first_seen: 1_700_000_000,
        pages: pages.map(|ps| {
            ps.iter()
                .map(|p| ChangedPage {
                    path: format!("pages/{p}.md"),
                    title: (*p).to_owned(),
                    exists: true,
                })
                .collect()
        }),
    }
}

#[test]
fn a_query_becomes_a_filter_and_a_filter_becomes_a_query() {
    let f = parse_filter(&query(&[
        ("namespace", "  Guides "),
        ("view", "draft"),
        ("automated", "hide"),
        ("page", ""),
    ]));
    assert_eq!(f.namespace.as_deref(), Some("Guides"));
    assert_eq!(f.view.as_deref(), Some("draft"));
    assert_eq!(f.page, None);
    assert_eq!(f.automated, Tri::Hide);
    assert_eq!(query_of(&f), "?view=draft&namespace=Guides&automated=hide");
    assert_eq!(query_of(&ChangeFilter::default()), "");
    // Something that is not a choice is no filter, never a guess.
    assert_eq!(
        parse_filter(&query(&[("automated", "maybe")])).automated,
        Tri::Any
    );
    // A title with characters of a query is carried whole.
    let f = parse_filter(&query(&[("page", "A & B/日本")]));
    assert_eq!(query_of(&f), "?page=A%20%26%20B%2F%E6%97%A5%E6%9C%AC");
}

#[test]
fn what_the_reader_may_not_read_is_left_out_and_counted() {
    let rows = vec![
        row("heads/main", Some(&["Open"])),
        row("heads/main", Some(&["Secret"])),
        row("heads/main", Some(&["Open", "Secret"])),
        row("heads/hidden", Some(&["Open"])),
        row("heads/main", Some(&[])),
        row("heads/main", None),
    ];
    let (kept, hidden) = restrict(
        rows,
        |view| view != "heads/hidden",
        |path| path != "pages/Secret.md",
    );
    assert_eq!(
        hidden, 2,
        "the change of Secret alone, and the hidden view's"
    );
    assert_eq!(kept.len(), 4);
    // A change that touched a page the reader may and one it may not shows only the first.
    let both = kept.get(1).expect("the mixed change");
    let titles: Vec<&str> = both
        .pages
        .iter()
        .flatten()
        .map(|p| p.title.as_str())
        .collect();
    assert_eq!(titles, ["Open"]);
    // Nothing is left out when everything may be read.
    let (kept, hidden) = restrict(vec![row("heads/main", Some(&["A"]))], |_| true, |_| true);
    assert_eq!((kept.len(), hidden), (1, 0));
}

#[test]
fn the_people_of_a_change_are_said_in_words() {
    let t = T::new(EN);
    let mut r = row("heads/main", None);
    assert_eq!(people_lines(t, &r), ["Proposed by Aiko", "Accepted by Ben"]);
    r.proposed_by = Some(Person::Service("inklkunn".into()));
    r.accepted_by = Some(Person::Service("inklkunn".into()));
    assert_eq!(people_lines(t, &r), ["Made by inklkunn, the wiki service"]);
    r.proposed_by = Some(Person::Anonymous("~k3j".into()));
    assert_eq!(
        people_lines(t, &r),
        [
            "Proposed by an anonymous visitor (~k3j)",
            "Accepted by inklkunn, the wiki service"
        ]
    );
    r.kind = ChangeKind::Foreign;
    assert_eq!(
        people_lines(t, &r),
        ["Recorded outside this service, so the people are not known."]
    );
}

#[test]
fn a_page_of_another_view_keeps_its_view_in_the_address() {
    assert_eq!(
        page_href("w", "heads/main", "heads/main", "A B/ü"),
        "/w/A%20B/%C3%BC"
    );
    assert_eq!(
        page_href("w", "heads/main", "heads/draft", "Home"),
        "/w/~draft/Home"
    );
}
