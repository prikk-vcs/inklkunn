#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::cell::Cell;

use inklkunn_markup::scan::ScanOptions;
use inklkunn_markup::{LinkHref, render_page};
use inklkunn_prikk::StorageError;

use super::*;
use crate::read::testing::Env;

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

fn index() -> TitleIndex {
    let mut alias = entry("pages/Guide.md", "Guide");
    alias.aliases = vec!["How-to".into()];
    let mut redirect = entry("pages/Old.md", "Old");
    redirect.redirect = Some("Guide".into());
    let mut broken = entry("pages/Broken.md", "Broken");
    broken.problem = Some("frontmatter".into());
    TitleIndex::new(
        "tip",
        vec![
            alias,
            redirect,
            broken,
            entry("pages/Home.md", "Home"),
            entry("pages/A/Same.md", "A/Same"),
            entry("pages/B/Same.md", "B/Same"),
        ],
    )
}

/// Render `body` as the page `pages/Home.md` and give back the HTML and the links.
fn render(
    ix: &TitleIndex,
    body: &str,
    reads: &Cell<u32>,
) -> (String, DerivedLinks, Option<ReadError>) {
    let body_of = |e: &IndexEntry| -> Result<Option<String>, ReadError> {
        reads.set(reads.get() + 1);
        match e.path.as_str() {
            "pages/Guide.md" => Ok(Some("---\ntitle: Guide\n---\nThe guide.\n".to_owned())),
            "pages/Home.md" => Ok(Some("Home text.\n".to_owned())),
            "pages/Old.md" => Ok(Some("redirect page\n".to_owned())),
            "pages/Unreadable.md" => Err(ReadError::CouldNotBeReadNow(
                super::super::Cause::Storage(StorageError::Unavailable {
                    verb: "cat",
                    reason: inklkunn_prikk::Unavailable::Busy,
                }),
            )),
            _ => Ok(None),
        }
    };
    let links = PageLinks::new(ix, &Env::EN, "pages/Home.md", &body_of);
    let rendered = render_page(
        Some(("pages/Home.md", "Home")),
        body,
        &links,
        ScanOptions::default(),
    );
    let failure = links.take_failure();
    let sets = links.into_links(rendered.transcluded);
    (rendered.unsanitized_html, sets, failure)
}

#[test]
fn links_resolve_by_the_rules_and_the_sets_are_sorted_and_without_repeats() {
    let ix = index();
    let reads = Cell::new(0);
    let (html, sets, failure) = render(
        &ix,
        "[[Guide]] [[How-to]] [[guide]] [[Guide]] [[Missing Page]] [[Absent]] [[Same]] [[wp:Wiki]]",
        &reads,
    );
    assert!(failure.is_none());
    // Resolved links lead to the page's address, whichever rule found it.
    assert!(
        html.matches("<a class=\"wikilink\" href=\"/w/Guide\"")
            .count()
            >= 4,
        "{html}"
    );
    assert_eq!(
        sets.resolved,
        [
            ("Guide".to_owned(), "pages/Guide.md".to_owned()),
            ("How-to".to_owned(), "pages/Guide.md".to_owned()),
            ("guide".to_owned(), "pages/Guide.md".to_owned()),
        ],
        "sorted, each once"
    );
    // A dangling link invites the page and is recorded.
    assert!(
        html.contains(
            "<a class=\"wikilink wikilink-missing\" href=\"/w/-/create?title=Missing%20Page\""
        ),
        "{html}"
    );
    assert_eq!(sets.dangling, ["Absent", "Missing Page", "wp:Wiki"]);
    // An ambiguous link goes to the first candidate and the ambiguity is reported, not hidden.
    assert_eq!(
        sets.ambiguous,
        [(
            "Same".to_owned(),
            vec!["pages/A/Same.md".to_owned(), "pages/B/Same.md".to_owned()]
        )]
    );
    assert!(html.contains("href=\"/w/A/Same\""), "{html}");
    assert!(sets.interwiki.is_empty(), "no interwiki map is declared");
    assert!(sets.transcluded.is_empty());
    assert_eq!(reads.get(), 0, "links read no page");
}

#[test]
fn an_embed_of_an_image_is_not_a_page_that_is_missing() {
    let ix = index();
    let reads = Cell::new(0);
    let (html, sets, _) = render(&ix, "![[photo.png]]\n\n![[Missing Page]]\n", &reads);
    assert!(
        html.contains("<span class=\"wikilink wikilink-missing\" data-wiki-target=\"photo.png\">"),
        "no invitation to create a page named like a file: {html}"
    );
    assert!(!html.contains("create?title=photo"), "{html}");
    assert_eq!(
        sets.dangling,
        ["Missing Page"],
        "and the file is not a dangling page link"
    );
}

#[test]
fn an_included_page_is_read_once_and_a_redirect_is_followed_to_its_page() {
    let ix = index();
    let reads = Cell::new(0);
    let (html, sets, failure) = render(&ix, "![[Guide]]\n\n![[Old]]\n\n![[How-to]]\n", &reads);
    assert!(failure.is_none());
    assert_eq!(html.matches("class=\"transclusion\"").count(), 3, "{html}");
    assert!(html.contains("<p>The guide.</p>"), "{html}");
    assert!(html.contains("From Guide"), "{html}");
    assert_eq!(reads.get(), 1, "the same page three times is read once");
    assert_eq!(sets.transcluded, ["pages/Guide.md"]);
}

#[test]
fn what_cannot_be_included_stays_a_link_and_a_failure_to_ask_prikk_is_kept_for_the_caller() {
    let ix = index();
    let reads = Cell::new(0);
    // A page with a frontmatter problem, a missing page and an ambiguous one are links.
    let (html, sets, failure) = render(&ix, "![[Broken]]\n\n![[Nowhere]]\n\n![[Same]]\n", &reads);
    assert!(failure.is_none());
    assert!(!html.contains("class=\"transclusion\""), "{html}");
    assert_eq!(reads.get(), 0);
    assert!(sets.transcluded.is_empty());

    // A page whose source is not text stays a link, quietly.
    let mut entries = ix.entries().to_vec();
    entries.push(entry("pages/Binary.md", "Binary"));
    let with_binary = TitleIndex::new("tip", entries);
    let (html, _, failure) = render(&with_binary, "![[Binary]]\n", &reads);
    assert!(failure.is_none());
    assert!(html.contains("wikilink-embed"), "{html}");

    // A failure to ask prikk is not a link: the caller gets it and does not keep the page.
    let mut entries = ix.entries().to_vec();
    entries.push(entry("pages/Unreadable.md", "Unreadable"));
    let with_unreadable = TitleIndex::new("tip", entries);
    let (_, _, failure) = render(&with_unreadable, "![[Unreadable]]\n", &reads);
    assert!(
        matches!(
            failure,
            Some(ReadError::CouldNotBeReadNow(super::super::Cause::Storage(
                _
            )))
        ),
        "{failure:?}"
    );
}

#[test]
fn the_presentation_supplies_the_words_and_the_hrefs_that_end_up_in_the_page() {
    let ix = index();
    let reads = Cell::new(0);
    let body_of = |_: &IndexEntry| -> Result<Option<String>, ReadError> { Ok(None) };
    let ja = Env::JA;
    let links = PageLinks::new(&ix, &ja, "pages/Home.md", &body_of);
    let LinkHref::Page { href, title } = links.link(&scan_link("Guide")) else {
        panic!("a page");
    };
    assert_eq!(href, "/w/Guide");
    assert_eq!(title.as_deref(), Some("Guide"));
    assert_eq!(
        links.transclusion_notice(&TransclusionRefusal::Cycle {
            chain: vec!["A".into(), "A".into()]
        }),
        "Not included, it would include itself: A → A"
    );
    let _ = reads;
}

fn scan_link(target: &str) -> inklkunn_markup::scan::Link {
    inklkunn_markup::scan::scan(&format!("[[{target}]]"))
        .links
        .into_iter()
        .next()
        .expect("a link")
}
