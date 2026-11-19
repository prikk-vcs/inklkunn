//! The sources a change attached (RFC 005 part 2 D-8; maintenance M56): *Why is this here?* lists, for each change it
//! names, the source files that change created under `knowledge/sources/`, each as its reference **in text** with the
//! way to the claim's page that cites it; a change that attached none says nothing; a reference with markup is
//! printed as text; a file that cannot be read as a source is counted, never shown; and the page reads at most what
//! it is allowed to. Every sentence is invented.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Bearing, Link, Reference, ReferenceForm, SourceKind, Standing};

use super::common::{
    HOME_ID, World, claim, claim_id, get, link, op, page, shown, source, source_id,
};
use super::main_of;

const SAID: &str = "The bridge opened in 1950.";
const SAID_NOW: &str = "The bridge was opened in 1950.";
const RIVER: &str = "It crosses the river.";
const RIVER_NOW: &str = "It crosses the wide river.";

fn home(first: &str, second: &str) -> String {
    page(
        &["title: Home", &format!("id: {HOME_ID}")],
        &format!("{first} ^b1\n\n{second}\n"),
    )
}

fn cited(n: u8, value: &str) -> inklkunn_core::knowledge::Source {
    let mut s = source(n, SourceKind::Primary);
    s.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: value.to_owned(),
    });
    s
}

/// A wiki whose `Home` is created in change 1, its second paragraph reworded in change 2 (which also attaches the sources `attached`, and
/// a claim that cites the first of them) and its first reworded in change 3 (which attaches nothing, and edits the first
/// source). The paragraph `^b1` was written in 1 and last changed in 3; the second was written in 1 and last changed in 2.
fn story(attached: &[inklkunn_core::knowledge::Source]) -> World {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home(SAID, RIVER))],
    );
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));

    let mut c = claim(1, Standing::Contested, None, SAID);
    if !attached.is_empty() {
        c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    }
    let mut files: Vec<(String, String)> = vec![
        ("pages/Home.md".to_owned(), home(SAID, RIVER_NOW)),
        (inklkunn_core::knowledge::claim_path(&c.id), c.to_yaml()),
    ];
    files.extend(attached.iter().map(|s| (path_of_id(&s.id), s.to_yaml())));
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    w.fake.set_files(&borrowed);
    w.fake.say(2, "Reword it and cite sources");
    let paths: Vec<String> = files.iter().skip(1).map(|(p, _)| p.clone()).collect();
    let mut ops = vec![op("edit-text", &["pages/Home.md"])];
    ops.extend(paths.iter().map(|p| op("create-file", &[p.as_str()])));
    w.fake.show(2, shown(ops));

    // Change 3 rewords the paragraph again and attaches nothing; it edits a source file that is not new.
    let mut ops = vec![op("edit-text", &["pages/Home.md"])];
    let mut again: Vec<(String, String)> =
        vec![("pages/Home.md".to_owned(), home(SAID_NOW, RIVER_NOW))];
    if let Some(first) = attached.first() {
        let mut edited = first.clone();
        edited.availability = inklkunn_core::knowledge::Availability::Available;
        again.push((path_of_id(&edited.id), edited.to_yaml()));
        ops.push(op("edit-text", &[path_of_id(&first.id).as_str()]));
    }
    let borrowed: Vec<(&str, &str)> = again
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    w.fake.set_files(&borrowed);
    w.fake.say(3, "Reword it again");
    w.fake.show(3, shown(ops));
    w
}

fn path_of_id(id: &inklkunn_core::knowledge::SourceId) -> String {
    inklkunn_core::knowledge::source_path(id)
}

/// A change's own block on the page.
fn change_block(body: &str, number: u64) -> String {
    let start = body
        .find(&format!("<li id=\"c-{number}\">"))
        .expect("the change is listed");
    let rest = body.get(start..).expect("the rest");
    let end = ["</li>\n</ol>", "</li>\n<li id=\"c-"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .expect("its end");
    rest.get(..end).expect("the change").to_owned()
}

#[tokio::test]
async fn a_change_that_attached_two_sources_lists_both_by_their_references_and_links_the_one_a_claim_cites()
 {
    let w = story(&[
        cited(1, "The Harbour Survey, page 12"),
        cited(2, "A letter, 3 May"),
    ]);
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = main_of(&a);
    let two = change_block(&body, 2);
    assert!(
        two.contains("<h4>Sources attached in this change</h4>"),
        "{two}"
    );
    assert!(two.contains("<ul class=\"why-attached\">"), "{two}");
    // The first is cited by the claim: its reference, as text, and the way to its line on that claim's page.
    assert!(
        two.contains(&format!(
            "<li>The Harbour Survey, page 12 <a href=\"/handbook/-/claim/{}#source-{}\">See it on the page of a claim that cites it</a></li>",
            claim_id(1),
            source_id(1)
        )),
        "{two}"
    );
    // The second is cited by nobody: its reference, and no way anywhere.
    assert!(two.contains("<li>A letter, 3 May</li>"), "{two}");
    assert_eq!(two.matches("why-attached").count(), 1, "{two}");
    // A reference is text: no link goes anywhere but into the wiki.
    assert!(!two.contains("href=\"http"), "{two}");
}

#[tokio::test]
async fn a_change_that_attached_nothing_says_nothing_even_when_it_touches_the_paragraph_and_a_source_it_did_not_add()
 {
    let w = story(&[cited(1, "The Harbour Survey, page 12")]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    // Change 3 reworded the paragraph and edited the source's file, and added none.
    let three = change_block(&body, 3);
    assert!(!three.contains("why-attached"), "{three}");
    assert!(!three.contains("Sources attached"), "{three}");
    assert!(!three.contains("The Harbour Survey"), "{three}");
    assert!(!three.contains("No source"), "{three}");
    // Nor does the change that created the page.
    let one = change_block(&body, 1);
    assert!(!one.contains("<h4>"), "{one}");
    // Change 2 is the one that attached it.
    assert!(
        change_block(&body, 2).contains("The Harbour Survey, page 12"),
        "{body}"
    );
}

#[tokio::test]
async fn a_reference_with_markup_is_text_and_one_the_format_refuses_is_counted_and_never_shown() {
    let w = story(&[
        cited(1, "<b>bold</b> <a href=\"https://evil.example/\">click</a>"),
        cited(2, "fine\u{202e}gnp.exe"),
    ]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("&#60;b&#62;bold&#60;/b&#62; &#60;a href=&#34;https://evil.example/&#34;&#62;click&#60;/a&#62;"),
        "{two}"
    );
    assert!(!two.contains("<b>"), "{two}");
    assert!(!two.contains("href=\"https://evil"), "{two}");
    // The bidirectional control is refused as it is at acceptance: the file is not a source, so it is counted.
    assert!(!two.contains('\u{202e}'), "{two}");
    assert!(!two.contains("gnp.exe"), "{two}");
    assert!(
        two.contains("1 source file added in this change cannot be read."),
        "{two}"
    );
}

#[tokio::test]
async fn a_source_that_gives_no_reference_is_listed_by_what_it_lacks() {
    let w = story(&[source(1, SourceKind::Primary)]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    let two = change_block(&body, 2);
    assert!(two.contains("A source that gives no reference"), "{two}");
}

#[tokio::test]
async fn a_change_that_attached_more_than_one_change_reads_lists_the_first_and_counts_the_rest() {
    let many: Vec<_> = (1..=12)
        .map(|n| cited(n, &format!("Reference number {n}")))
        .collect();
    let w = story(&many);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    let two = change_block(&body, 2);
    assert_eq!(two.matches("Reference number").count(), 10, "{two}");
    assert!(two.contains("<li>Reference number 1 <a href="), "{two}");
    assert!(!two.contains("Reference number 11"), "{two}");
    assert!(
        two.contains("2 more sources attached in this change are not listed here."),
        "{two}"
    );
}

#[tokio::test]
async fn the_japanese_page_says_the_same_in_japanese() {
    let w = story(&[cited(1, "The Harbour Survey, page 12")]);
    let a = super::common::request(
        &w,
        super::common::APP,
        "GET",
        "/handbook/-/why/Home",
        &[("cookie", "inklkunn_lang=ja")],
    )
    .await;
    let two = change_block(&main_of(&a), 2);
    assert!(two.contains("<h4>この変更で付けられた出典</h4>"), "{two}");
    assert!(two.contains("The Harbour Survey, page 12"), "{two}");
    assert!(
        two.contains("この出典を引用している主張のページで見る"),
        "{two}"
    );
}

#[tokio::test]
async fn one_page_reads_a_bounded_share_of_source_files_whatever_the_number_of_changes() {
    // Six paragraphs, each reworded in a change of its own, and each of those changes attaches ten sources: sixty
    // files that the page is not asked to read all of. The newest changes are read first, forty files in all.
    let body = |reworded: usize| {
        let paragraphs: Vec<String> = (1..=6)
            .map(|n| {
                if n <= reworded {
                    format!("Paragraph {n} reworded.")
                } else {
                    format!("Paragraph {n}.")
                }
            })
            .collect();
        paragraphs.join("\n\n") + "\n"
    };
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &page(&["title: Home"], &body(0)))],
    );
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));
    for version in 2..=7u8 {
        let mut files = vec![(
            "pages/Home.md".to_owned(),
            page(&["title: Home"], &body(usize::from(version) - 1)),
        )];
        let mut ops = vec![op("edit-text", &["pages/Home.md"])];
        for n in 0..10u8 {
            let s = cited(version * 10 + n, &format!("Reference {version}-{n}"));
            ops.push(op("create-file", &[path_of_id(&s.id).as_str()]));
            files.push((path_of_id(&s.id), s.to_yaml()));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        w.fake.set_files(&borrowed);
        w.fake.say(u64::from(version), &format!("Change {version}"));
        w.fake.show(u64::from(version), shown(ops));
    }
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = main_of(&a);
    assert_eq!(body.matches("<li>Reference ").count(), 40, "{body}");
    assert_eq!(
        body.matches("10 more sources attached in this change are not listed here.")
            .count(),
        2,
        "{body}"
    );
    // The newest are the ones read.
    assert!(body.contains("<li>Reference 7-0</li>"), "{body}");
    assert!(body.contains("<li>Reference 4-9</li>"), "{body}");
    assert!(!body.contains("Reference 3-0"), "{body}");
}
